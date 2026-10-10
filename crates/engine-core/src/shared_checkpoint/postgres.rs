//! Transactional PostgreSQL baseline for bounded shared checkpoint objects.

use engine_shared::DocumentId;
use sqlx::{PgConnection, PgPool};

use super::{decode, read_state, write_state, Address, Error, Limits, ObjectStore, WriteStats};
use crate::{log::ZERO_HASH, materializer::DocumentState, snapshot::read_state_at};

/// Internal document-scoped store. Use a transaction when writing graphs;
/// individual `put` calls do not publish a checkpoint or check authorization.
pub struct PgStore<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgStore<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl ObjectStore for PgStore<'_> {
    async fn put(
        &mut self,
        document: DocumentId,
        address: &Address,
        bytes: &[u8],
    ) -> Result<bool, Error> {
        decode(address, bytes)?;
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
}

async fn chain_at(
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
    // KEY SHARE blocks erasure, but does not serialize with ordinary non-key
    // document updates/appends. This reader addresses a fixed immutable revision.
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
    protect_document(&mut tx, document).await?;
    let event_chain_hash = chain_at(&mut tx, document, through_seq).await?;
    let state = read_state_at(&mut tx, document, through_seq).await?.state;
    let mut store = PgStore::new(&mut tx);
    let (root, stats) = write_state(&mut store, document, &state, limits).await?;
    if read_state(&mut store, document, &root, limits).await? != state {
        return Err(Error::Invalid(
            "checkpoint reconstruction differs from history",
        ));
    }
    sqlx::query(
        "insert into shared_checkpoint(document_id,through_seq,codec_version,state_root,event_chain_hash)
         values($1,$2,1,$3,$4) on conflict(document_id,through_seq) do nothing",
    )
    .bind(document.0)
    .bind(through_seq)
    .bind(root.as_str())
    .bind(&event_chain_hash)
    .execute(&mut *tx)
    .await?;
    let stored: (String, Vec<u8>) = sqlx::query_as(
        "select state_root,event_chain_hash from shared_checkpoint where document_id=$1 and through_seq=$2",
    )
    .bind(document.0)
    .bind(through_seq)
    .fetch_one(&mut *tx)
    .await?;
    if stored.0 != root.as_str() || stored.1 != event_chain_hash {
        return Err(Error::Invalid(
            "different checkpoint already published at revision",
        ));
    }
    tx.commit().await?;
    Ok(PublishedCheckpoint {
        through_seq,
        root,
        event_chain_hash,
        stats,
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
    let (version, root, chain): (i16, String, Vec<u8>) = sqlx::query_as(
        "select codec_version,state_root,event_chain_hash from shared_checkpoint
         where document_id=$1 and through_seq=$2",
    )
    .bind(document.0)
    .bind(through_seq)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Error::Invalid("checkpoint not published at revision"))?;
    if version != 1 || chain != chain_at(&mut tx, document, through_seq).await? {
        return Err(Error::Invalid("checkpoint version or chain position"));
    }
    let state = read_state(
        &mut PgStore::new(&mut tx),
        document,
        &Address::try_from(root)?,
        limits,
    )
    .await?;
    tx.commit().await?;
    Ok(state)
}
