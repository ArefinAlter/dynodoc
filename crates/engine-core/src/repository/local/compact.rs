//! One-time, resumable re-encoding of existing identity object rows into their
//! compressed form (`repository ... compact`).
//!
//! Nothing a hash, an address, a quota or a transfer packet depends on changes: a
//! row's logical bytes stay byte for byte what they were and only `bytes` and
//! `codec` are rewritten. Work is done in short `BEGIN IMMEDIATE` transactions of
//! a bounded number of rows and bytes. Each row is verified before it is replaced
//! (the stored object must hash to its address, the compressed form must inflate
//! to exactly the same bytes), each batch checks the persistent counters against
//! the rows it changed, and an interrupted run leaves every committed batch valid.
//! Running it again examines the same rows and changes only what is left.
//!
//! The immutability trigger on `objects` permits an update only while a row exists
//! in `local_compaction`, which a batch inserts and removes inside its own
//! transaction; no other connection can ever observe it.

use super::{collector::Vacuum, pack, storage, LocalRepository, ObjectKey};
use crate::{
    repository::Error,
    shared_checkpoint::{self, Address},
};
use serde::Serialize;

pub const FORMAT: &str = "dynodoc.local-compact.v1";
/// Rows examined per transaction at most.
pub const MAX_BATCH_OBJECTS: usize = 1024;
/// A batch stops after reading about this many object bytes, so peak memory and
/// writer-lock time stay small whatever the row sizes.
const BATCH_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct CompactOptions {
    /// Rows examined per transaction (1..=[`MAX_BATCH_OBJECTS`]).
    pub batch_objects: usize,
    /// Stop after examining about this many rows in this call; the report's
    /// `next` resumes from there. Unset runs to the end.
    pub max_objects: Option<u64>,
    /// Resume after this key (exclusive).
    pub after: Option<ObjectKey>,
    /// After the run, checkpoint and truncate the write-ahead log and `VACUUM`, so
    /// the file shrinks. Needs free disk space about the size of the database.
    pub vacuum: bool,
}

impl Default for CompactOptions {
    fn default() -> Self {
        Self {
            batch_objects: 256,
            max_objects: None,
            after: None,
            vacuum: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Cursor {
    pub document: uuid::Uuid,
    pub address: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CompactReport {
    pub format: &'static str,
    pub project: uuid::Uuid,
    /// Rows looked at in this call.
    pub examined: u64,
    /// Identity rows rewritten as compressed rows.
    pub compacted: u64,
    /// Rows that were already compressed.
    pub already_compressed: u64,
    /// Identity rows left alone: shorter than the threshold or not compressible
    /// by at least a tenth.
    pub kept_identity: u64,
    /// Logical bytes of the rows that were rewritten, and what they saved.
    pub logical_bytes_compacted: u64,
    pub stored_bytes_saved: u64,
    pub batches: u64,
    /// False when `max_objects` stopped the run; resume with `next`.
    pub complete: bool,
    pub next: Option<Cursor>,
    pub capacity_before: super::StorageUsage,
    pub capacity_after: super::StorageUsage,
    /// Physical file sizes (database plus write-ahead log), when requested.
    pub vacuum: Option<Vacuum>,
}

impl CompactReport {
    pub fn summary(&self) -> String {
        let mut text = format!(
            "Local compaction\nproject {}\nexamined {} rows in {} batches: {} rewritten, {} already compressed, {} left as they were\n",
            self.project,
            self.examined,
            self.batches,
            self.compacted,
            self.already_compressed,
            self.kept_identity
        );
        text.push_str(&format!(
            "rewritten rows held {} logical bytes and now save {} stored bytes\n",
            self.logical_bytes_compacted, self.stored_bytes_saved
        ));
        let usage = |name: &str, u: &super::StorageUsage| {
            format!(
                "  {name:<7} logical bytes {} (limit {})  stored bytes {}\n",
                u.bytes_used, u.limits.bytes, u.stored_bytes
            )
        };
        text.push_str("capacity\n");
        text.push_str(&usage("before", &self.capacity_before));
        text.push_str(&usage("after", &self.capacity_after));
        if let Some(vacuum) = &self.vacuum {
            text.push_str(&format!(
                "vacuum: database+wal {} -> {} bytes
",
                vacuum.file_bytes_before, vacuum.file_bytes_after
            ));
            if let Some(error) = &vacuum.error {
                text.push_str(&format!(
                    "vacuum FAILED (compaction stands): {error}
"
                ));
            }
        }
        match (&self.complete, &self.next) {
            (true, _) => text.push_str("complete: every row was examined\n"),
            (false, Some(next)) => text.push_str(&format!(
                "stopped early; resume after document {} address {}\n",
                next.document, next.address
            )),
            (false, None) => text.push_str("stopped early\n"),
        }
        text
    }
}

/// Points inside a batch where a test can inject a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Locked,
    Rewritten,
    Accounted,
    Committed,
}

pub(super) type Hook<'a> = dyn FnMut(Stage) -> Result<(), Error> + Send + 'a;

impl LocalRepository {
    /// Re-encode identity rows as compressed rows, in bounded verified batches.
    /// Needs a writable schema-3 repository. Safe to repeat and to interrupt.
    pub async fn compact(&self, options: CompactOptions) -> Result<CompactReport, Error> {
        self.compact_with(options, &mut |_| Ok(())).await
    }

    pub(super) async fn compact_with(
        &self,
        options: CompactOptions,
        hook: &mut Hook<'_>,
    ) -> Result<CompactReport, Error> {
        if options.batch_objects == 0 || options.batch_objects > MAX_BATCH_OBJECTS {
            return Err(Error::Limit("compaction batch size"));
        }
        let capacity_before = {
            let mut connection = self.pool.acquire().await?;
            storage::usage(&mut connection, self.schema).await?
        };
        let mut report = CompactReport {
            format: FORMAT,
            project: self.project,
            examined: 0,
            compacted: 0,
            already_compressed: 0,
            kept_identity: 0,
            logical_bytes_compacted: 0,
            stored_bytes_saved: 0,
            batches: 0,
            complete: false,
            next: None,
            capacity_before: capacity_before.clone(),
            capacity_after: capacity_before,
            vacuum: None,
        };
        let (mut document, mut address) = match &options.after {
            Some(key) => (key.document.0.to_string(), key.address.as_str().to_owned()),
            None => (String::new(), String::new()),
        };
        loop {
            let remaining = options
                .max_objects
                .map(|max| max.saturating_sub(report.examined));
            if remaining == Some(0) {
                break;
            }
            let limit = remaining
                .map_or(options.batch_objects as u64, |r| {
                    r.min(options.batch_objects as u64)
                })
                .max(1);
            match self
                .compact_batch(&mut document, &mut address, limit as i64, &mut report, hook)
                .await?
            {
                0 => {
                    report.complete = true;
                    break;
                }
                _ => report.batches += 1,
            }
        }
        if !report.complete {
            report.next = Some(Cursor {
                document: uuid::Uuid::parse_str(&document)
                    .map_err(|_| Error::Invalid("stored document id"))?,
                address: address.clone(),
            });
        }
        let mut connection = self.pool.acquire().await?;
        report.capacity_after = storage::usage(&mut connection, self.schema).await?;
        verify_counters(&mut connection, &report.capacity_after).await?;
        drop(connection);
        if options.vacuum {
            report.vacuum = Some(self.vacuum().await);
        }
        Ok(report)
    }

    /// One transaction. Returns how many rows it examined (0 = nothing after the
    /// cursor) and advances the cursor past them.
    async fn compact_batch(
        &self,
        document: &mut String,
        address: &mut String,
        limit: i64,
        report: &mut CompactReport,
        hook: &mut Hook<'_>,
    ) -> Result<usize, Error> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let query_only: i64 = sqlx::query_scalar("pragma query_only")
            .fetch_one(&mut *tx)
            .await?;
        if query_only != 0 {
            return Err(Error::Invalid("compaction needs a writable repository"));
        }
        let schema: i64 = sqlx::query_scalar("select version from local_meta where singleton=1")
            .fetch_one(&mut *tx)
            .await?;
        if schema != storage::SCHEMA_VERSION {
            return Err(Error::Invalid("compaction requires local schema 3"));
        }
        let before = storage::usage(&mut tx, schema).await?;
        sqlx::query("insert into local_compaction(singleton) values(1)")
            .execute(&mut *tx)
            .await?;
        hook(Stage::Locked)?;

        let keys: Vec<(String, String, i64, i64)> = sqlx::query_as(
            "select document,address,codec,length(bytes) from objects where (document,address)>(?,?) order by document,address limit ?",
        )
        .bind(&*document)
        .bind(&*address)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        let mut examined = 0usize;
        let mut read_bytes = 0usize;
        let mut saved = 0u64;
        let (mut compacted, mut already, mut kept, mut logical_total) = (0u64, 0u64, 0u64, 0u64);
        for (doc, addr, codec, stored_len) in &keys {
            if read_bytes >= BATCH_BYTES && examined > 0 {
                break;
            }
            examined += 1;
            *document = doc.clone();
            *address = addr.clone();
            if *codec != pack::IDENTITY {
                already += 1;
                continue;
            }
            let stored_len = *stored_len as usize;
            if stored_len < pack::MIN_LOGICAL_BYTES {
                kept += 1;
                continue;
            }
            read_bytes += stored_len;
            let bytes: Vec<u8> = sqlx::query_scalar(
                "select bytes from objects where document=? and address=? and codec=0",
            )
            .bind(doc)
            .bind(addr)
            .fetch_one(&mut *tx)
            .await?;
            // Verify before touching the row: the stored object must be the one its
            // address names. A corrupt row is an error, never silently "kept".
            let parsed = Address::try_from(addr.clone())?;
            shared_checkpoint::validate_stored(&parsed, &bytes)?;
            let Some(stored) = pack::pack(&bytes) else {
                kept += 1;
                continue;
            };
            // ... and the replacement must give back exactly those bytes.
            let back = pack::unpack(pack::DEFLATE, stored.clone(), Some(bytes.len() as i64))?;
            if back != bytes {
                return Err(Error::Invalid("compressed object does not round-trip"));
            }
            let changed = sqlx::query(
                "update objects set bytes=?,codec=1,size=? where document=? and address=? and codec=0 and length(bytes)=?",
            )
            .bind(&stored)
            .bind(bytes.len() as i64)
            .bind(doc)
            .bind(addr)
            .bind(bytes.len() as i64)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(Error::Invalid("object row changed during compaction"));
            }
            compacted += 1;
            logical_total += bytes.len() as u64;
            saved += (bytes.len() - stored.len()) as u64;
        }
        hook(Stage::Rewritten)?;

        // Logical quota counters must be exactly as they were; the physical one must
        // have fallen by exactly what this batch saved.
        let after = storage::usage(&mut tx, schema).await?;
        let expected_stored = before
            .stored_bytes
            .checked_sub(saved as i64)
            .ok_or(Error::Invalid("compaction accounting"))?;
        if after.bytes_used != before.bytes_used
            || after.objects_used != before.objects_used
            || after.commits_used != before.commits_used
            || after.branches_used != before.branches_used
            || after.limits != before.limits
            || after.stored_bytes != expected_stored
        {
            return Err(Error::Invalid(
                "capacity counters changed unexpectedly during compaction",
            ));
        }
        hook(Stage::Accounted)?;
        sqlx::query("delete from local_compaction")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        hook(Stage::Committed)?;
        report.examined += examined as u64;
        report.compacted += compacted;
        report.already_compressed += already;
        report.kept_identity += kept;
        report.logical_bytes_compacted += logical_total;
        report.stored_bytes_saved += saved;
        Ok(examined)
    }
}

/// The persisted counters equal a fresh recomputation from the rows.
async fn verify_counters(
    connection: &mut sqlx::SqliteConnection,
    usage: &super::StorageUsage,
) -> Result<(), Error> {
    let (objects, logical, stored): (i64, i64, i64) = sqlx::query_as(
        "select count(*),coalesce(sum(coalesce(size,length(bytes))),0),coalesce(sum(length(bytes)),0) from objects",
    )
    .fetch_one(&mut *connection)
    .await?;
    let (commits, commit_bytes): (i64, i64) =
        sqlx::query_as("select count(*),coalesce(sum(length(body)),0) from commits")
            .fetch_one(&mut *connection)
            .await?;
    if usage.objects_used != objects
        || usage.commits_used != commits
        || usage.bytes_used != logical + commit_bytes
        || usage.stored_bytes != stored + commit_bytes
    {
        return Err(Error::Invalid(
            "capacity counters differ from a fresh recomputation",
        ));
    }
    Ok(())
}
