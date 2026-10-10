//! Transactional PostgreSQL baseline for bounded shared checkpoint objects.

use engine_shared::DocumentId;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    decode, read_state, write_state, Address, Error, Limits, ObjectStore, WriteStats,
    READ_BATCH_OBJECTS, WRITE_BATCH_BYTES, WRITE_BATCH_OBJECTS,
};
use crate::{log::ZERO_HASH, materializer::DocumentState, snapshot::read_legacy_state_at};

/// Internal document-scoped store. Use a transaction when writing graphs;
/// individual `put` calls do not publish a checkpoint or check authorization.
pub struct PgStore<'a> {
    connection: &'a mut PgConnection,
    prefetched: BTreeMap<(uuid::Uuid, Address), Vec<u8>>,
    read_queries: usize,
    write_queries: usize,
}

impl<'a> PgStore<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self {
            connection,
            prefetched: BTreeMap::new(),
            read_queries: 0,
            write_queries: 0,
        }
    }

    /// Actual object SELECTs, including reuse checks; useful for backend measurements.
    pub fn read_queries(&self) -> usize {
        self.read_queries
    }

    pub fn write_queries(&self) -> usize {
        self.write_queries
    }

    async fn matching_objects(
        &mut self,
        document: DocumentId,
        objects: &[(&str, &[u8])],
    ) -> Result<BTreeSet<String>, Error> {
        // Compare exact bytes in PostgreSQL and return only addresses/booleans.
        // A corrupt stored row can be much larger than its expected value; do
        // not hydrate up to 64 MiB of bad content to validate a 1 MiB input batch.
        let mut query =
            QueryBuilder::<Postgres>::new("select input.address, stored.bytes=input.bytes from (");
        query.push_values(objects, |mut row, (address, bytes)| {
            row.push_bind(*address).push_bind(*bytes);
        });
        query
            .push(") as input(address,bytes) join checkpoint_object stored on stored.document_id=");
        query
            .push_bind(document.0)
            .push(" and stored.address=input.address");
        self.read_queries += 1;
        let rows: Vec<(String, bool)> = query
            .build_query_as()
            .fetch_all(&mut *self.connection)
            .await?;
        let mut matching = BTreeSet::new();
        for (address, matches) in rows {
            if !matches {
                return Err(Error::HashMismatch);
            }
            matching.insert(address);
        }
        Ok(matching)
    }
}

impl ObjectStore for PgStore<'_> {
    async fn put_batch(
        &mut self,
        document: DocumentId,
        objects: &[(Address, Vec<u8>)],
    ) -> Result<Vec<bool>, Error> {
        if objects.len() > WRITE_BATCH_OBJECTS
            || objects.iter().map(|(_, bytes)| bytes.len()).sum::<usize>() > WRITE_BATCH_BYTES
        {
            return Err(Error::Limit("write batch"));
        }
        if objects.is_empty() {
            return Ok(Vec::new());
        }
        self.prefetched.clear();
        let mut unique = BTreeMap::new();
        for (address, bytes) in objects {
            decode(address, bytes)?;
            if let Some(prior) = unique.insert(address.as_str(), bytes.as_slice()) {
                if prior != bytes {
                    return Err(Error::HashMismatch);
                }
            }
        }
        let expected: Vec<_> = unique.iter().map(|(key, bytes)| (*key, *bytes)).collect();
        let existing = self.matching_objects(document, &expected).await?;
        let missing: Vec<_> = unique
            .iter()
            .filter(|(key, _)| !existing.contains(**key))
            .collect();
        let mut inserted = BTreeSet::new();
        if !missing.is_empty() {
            let mut query = QueryBuilder::<Postgres>::new(
                "insert into checkpoint_object(document_id,address,bytes) ",
            );
            query.push_values(&missing, |mut row, (address, bytes)| {
                row.push_bind(document.0)
                    .push_bind(**address)
                    .push_bind(**bytes);
            });
            query.push(" on conflict(document_id,address) do nothing returning address");
            self.write_queries += 1;
            let added: Vec<String> = query
                .build_query_scalar()
                .fetch_all(&mut *self.connection)
                .await?;
            inserted.extend(added);
            // A concurrent inserter may win after our initial read. Check those
            // bytes too; ON CONFLICT alone is not evidence of a valid reused object.
            let raced: Vec<_> = missing
                .iter()
                .filter(|(key, _)| !inserted.contains(**key))
                .map(|(key, bytes)| (**key, **bytes))
                .collect();
            if !raced.is_empty() {
                let rows = self.matching_objects(document, &raced).await?;
                for (key, _) in raced {
                    if !rows.contains(key) {
                        return Err(Error::Missing(key.to_owned()));
                    }
                }
            }
        }
        // Removing each address reports repeated input objects as reused after
        // their first insertion, exactly like the sequential backend contract.
        Ok(objects
            .iter()
            .map(|(address, _)| inserted.remove(address.as_str()))
            .collect())
    }

    async fn prefetch(&mut self, document: DocumentId, addresses: &[Address]) -> Result<(), Error> {
        self.prefetched.clear();
        if addresses.len() > READ_BATCH_OBJECTS {
            return Err(Error::Limit("read batch objects"));
        }
        if addresses.is_empty() {
            return Ok(());
        }
        let keys: Vec<_> = addresses.iter().map(Address::as_str).collect();
        self.read_queries += 1;
        let rows: Vec<(String, Vec<u8>)> = sqlx::query_as(
            "select address,bytes from checkpoint_object where document_id=$1 and address=any($2)",
        )
        .bind(document.0)
        .bind(&keys)
        .fetch_all(&mut *self.connection)
        .await?;
        for (address, bytes) in rows {
            self.prefetched
                .insert((document.0, Address::try_from(address)?), bytes);
        }
        Ok(())
    }

    async fn put(
        &mut self,
        document: DocumentId,
        address: &Address,
        bytes: &[u8],
    ) -> Result<bool, Error> {
        decode(address, bytes)?;
        self.write_queries += 1;
        let result = sqlx::query(
            "insert into checkpoint_object(document_id,address,bytes) values($1,$2,$3)
             on conflict(document_id,address) do nothing",
        )
        .bind(document.0)
        .bind(address.as_str())
        .bind(bytes)
        .execute(&mut *self.connection)
        .await?;
        if result.rows_affected() == 0 && self.get(document, address).await? != bytes {
            return Err(Error::HashMismatch);
        }
        Ok(result.rows_affected() == 1)
    }

    async fn get(&mut self, document: DocumentId, address: &Address) -> Result<Vec<u8>, Error> {
        if let Some(bytes) = self.prefetched.get(&(document.0, address.clone())) {
            return Ok(bytes.clone());
        }
        self.read_queries += 1;
        sqlx::query_scalar(
            "select bytes from checkpoint_object where document_id=$1 and address=$2",
        )
        .bind(document.0)
        .bind(address.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(|| Error::Missing(address.as_str().to_owned()))
    }
}

#[derive(Debug)]
pub struct PublishedCheckpoint {
    pub through_seq: i64,
    pub root: Address,
    pub event_chain_hash: Vec<u8>,
    pub stats: WriteStats,
    pub object_read_queries: usize,
    pub object_write_queries: usize,
}

pub(crate) fn publication_lock_key(document: DocumentId) -> String {
    format!("dynodoc.periodic-checkpoint:{}", document.0)
}

pub(crate) async fn chain_at(
    connection: &mut PgConnection,
    document: DocumentId,
    seq: i64,
) -> Result<Vec<u8>, Error> {
    if seq == 0 {
        return Ok(ZERO_HASH.to_vec());
    }
    sqlx::query_scalar("select chain_hash from event where document_id=$1 and seq=$2")
        .bind(document.0)
        .bind(seq)
        .fetch_optional(connection)
        .await?
        .ok_or(Error::Invalid("revision outside event history"))
}

async fn protect_document(
    connection: &mut PgConnection,
    document: DocumentId,
) -> Result<(), Error> {
    // KEY SHARE blocks erasure. The API append path takes FOR UPDATE on the same
    // row, so it also waits during publication; benchmark before enabling rollout.
    let exists: Option<uuid::Uuid> = sqlx::query_scalar(
        "select id from document where id=$1 and deleted_at is null for key share",
    )
    .bind(document.0)
    .fetch_optional(connection)
    .await?;
    if exists.is_none() {
        return Err(Error::Invalid("document missing or in trash"));
    }
    Ok(())
}

/// Derive, verify and atomically publish an exact historical checkpoint. Caller
/// must authorize the document. Does not rewrite or replace legacy snapshots.
pub async fn take(
    pool: &PgPool,
    document: DocumentId,
    through_seq: i64,
    limits: Limits,
) -> Result<PublishedCheckpoint, Error> {
    if through_seq < 0 {
        return Err(Error::Invalid("negative revision"));
    }
    let mut tx = pool.begin().await?;
    // Same lock/order as periodic publication, before the document row lock.
    // This also avoids cross-batch deadlocks for overlapping historical graphs.
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(publication_lock_key(document))
        .execute(&mut *tx)
        .await?;
    protect_document(&mut tx, document).await?;
    let event_chain_hash = chain_at(&mut tx, document, through_seq).await?;
    // Explicit operator publication checks against legacy checkpoints/event replay,
    // never against the shared manifest it is attempting to validate or replace.
    let state = read_legacy_state_at(&mut tx, document, through_seq)
        .await?
        .state;
    let published = publish_in_connection(
        &mut tx,
        document,
        through_seq,
        &state,
        event_chain_hash,
        limits,
    )
    .await?;
    tx.commit().await?;
    Ok(published)
}

/// Caller owns the transaction/savepoint and must protect the document against
/// erasure. Used only with a state derived at the exact immutable event revision.
pub(crate) async fn publish_in_connection(
    connection: &mut PgConnection,
    document: DocumentId,
    through_seq: i64,
    state: &DocumentState,
    event_chain_hash: Vec<u8>,
    limits: Limits,
) -> Result<PublishedCheckpoint, Error> {
    let mut store = PgStore::new(connection);
    let (root, stats) = write_state(&mut store, document, state, limits).await?;
    if read_state(&mut store, document, &root, limits).await? != *state {
        return Err(Error::Invalid(
            "checkpoint reconstruction differs from history",
        ));
    }
    let object_read_queries = store.read_queries();
    let object_write_queries = store.write_queries();
    sqlx::query(
        "insert into shared_checkpoint(document_id,through_seq,codec_version,state_root,event_chain_hash)
         values($1,$2,1,$3,$4) on conflict(document_id,through_seq) do nothing",
    )
    .bind(document.0)
    .bind(through_seq)
    .bind(root.as_str())
    .bind(&event_chain_hash)
    .execute(&mut *connection)
    .await?;
    let stored: (String, Vec<u8>) = sqlx::query_as(
        "select state_root,event_chain_hash from shared_checkpoint where document_id=$1 and through_seq=$2",
    )
    .bind(document.0)
    .bind(through_seq)
    .fetch_one(&mut *connection)
    .await?;
    if stored.0 != root.as_str() || stored.1 != event_chain_hash {
        return Err(Error::Invalid(
            "different checkpoint already published at revision",
        ));
    }
    Ok(PublishedCheckpoint {
        through_seq,
        root,
        event_chain_hash,
        stats,
        object_read_queries,
        object_write_queries,
    })
}

/// Load an exact published checkpoint, checking chain-position metadata and every
/// object. This is not an independent proof that the root equals event replay.
pub async fn load(
    pool: &PgPool,
    document: DocumentId,
    through_seq: i64,
    limits: Limits,
) -> Result<DocumentState, Error> {
    let mut tx = pool.begin().await?;
    protect_document(&mut tx, document).await?;
    let state = load_in_connection(&mut tx, document, through_seq, limits).await?;
    tx.commit().await?;
    Ok(state)
}

/// Read within the caller's existing transaction/connection. No second pool
/// acquisition or document lock: authorized write callers already hold theirs.
pub(crate) async fn load_in_connection(
    connection: &mut PgConnection,
    document: DocumentId,
    through_seq: i64,
    limits: Limits,
) -> Result<DocumentState, Error> {
    let (version, root, chain): (i16, String, Vec<u8>) = sqlx::query_as(
        "select codec_version,state_root,event_chain_hash from shared_checkpoint
         where document_id=$1 and through_seq=$2",
    )
    .bind(document.0)
    .bind(through_seq)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or(Error::Invalid("checkpoint not published at revision"))?;
    if version != 1 || chain != chain_at(connection, document, through_seq).await? {
        return Err(Error::Invalid("checkpoint version or chain position"));
    }
    read_state(
        &mut PgStore::new(connection),
        document,
        &Address::try_from(root)?,
        limits,
    )
    .await
}
