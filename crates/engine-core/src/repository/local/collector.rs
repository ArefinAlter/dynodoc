//! Explicit, hash-bound retention collector for one local project repository.
//!
//! The collector deletes exactly one class of data: stored objects that no stored
//! commit's file state graph references (the plan's `staged` category). Commits,
//! signatures, branch refs and every object any stored commit reaches, including
//! the history of unreachable commits and recovery/revert/cherry-pick provenance
//! sources, are never touched, so every retained commit stays fully verifiable and
//! transferable. Orphaned and superseded commits keep the objects only they reach.
//!
//! A run holds SQLite's writer lock (`BEGIN IMMEDIATE`) for its whole transaction:
//! it recomputes the plan from the rows it will modify, refuses unless the plan
//! hash equals the operator's, deletes, updates the persistent capacity counters
//! and checks them against a fresh recomputation. Any error rolls the transaction
//! back, including the temporary removal of the object immutability guard, so a
//! failed or interrupted run deletes nothing. See docs/LOCAL-RETENTION-PLAN.md.

use super::{
    retention::{self, Counts, ObjectKey},
    storage, GcPlanOptions, LocalRepository, StorageUsage,
};
use crate::{repository::Error, shared_checkpoint::Address};
use serde::Serialize;
use sqlx::{Connection, SqliteConnection};
use std::{collections::HashSet, path::Path};

pub const FORMAT: &str = "dynodoc.local-retention-gc.v1";
const SCAN_PAGE: i64 = 512;
const STALE: &str = "retention plan hash does not match the repository now (a branch moved or objects changed); run gc-plan again";
const GUARD_SQL: &str = "select sql from sqlite_master where type='trigger' and name='objects_no_delete' and tbl_name='objects'";
/// Created lazily by the first deleting run, in that run's transaction. It is an
/// append-only record of what was removed; local schema 2 and capacity counters
/// do not depend on it, and earlier clients ignore it.
const LOG_SCHEMA: &str = "
create table if not exists local_retention_log(
    seq integer primary key autoincrement,
    plan_hash text not null check(length(plan_hash)=64),
    deleted_objects integer not null check(deleted_objects>0),
    deleted_bytes integer not null check(deleted_bytes>0),
    objects_after integer not null check(objects_after>=0),
    bytes_after integer not null check(bytes_after>=0),
    applied_at_ms integer not null
);
create trigger if not exists local_retention_log_no_update before update on local_retention_log begin select raise(abort,'immutable retention log'); end;
create trigger if not exists local_retention_log_no_delete before delete on local_retention_log begin select raise(abort,'immutable retention log'); end;
";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GcApplyOptions {
    /// After the commit, checkpoint and truncate the write-ahead log and run
    /// `VACUUM` so SQLite returns the freed pages. Needs free disk space about the
    /// size of the database file while it runs.
    pub vacuum: bool,
}

/// Rows and encoded payload bytes removed by this run. `object_bytes` is logical
/// (canonical encoded bytes, what the capacity limits charge); `stored_bytes` is
/// what those rows physically held after compression at rest.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Deleted {
    pub objects: u64,
    pub object_bytes: u64,
    pub stored_bytes: u64,
}

/// Physical file sizes (database file plus write-ahead log), not payload bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Vacuum {
    pub file_bytes_before: u64,
    pub file_bytes_after: u64,
    /// Set when checkpoint or VACUUM failed after the deletion had committed. The
    /// deletion stands; only the file shrink did not happen.
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GcReport {
    pub format: &'static str,
    pub project: uuid::Uuid,
    pub plan_hash: String,
    /// True only when this run deleted rows and committed.
    pub applied: bool,
    /// True when the supplied hash was applied by an earlier run (replay): nothing
    /// was changed by this call.
    pub already_applied: bool,
    pub deleted: Deleted,
    /// Everything still retained, and why: reachable from a branch head or a
    /// recovery source, or an unreachable commit with the objects only it reaches.
    pub retained: Counts,
    pub reachable: Counts,
    pub unreachable_retained: Counts,
    pub capacity_before: StorageUsage,
    pub capacity_after: StorageUsage,
    pub vacuum: Option<Vacuum>,
}

impl GcReport {
    /// Plain-text rendering of the same facts.
    pub fn summary(&self) -> String {
        let line = |name: &str, c: &Counts| {
            format!(
                "  {name:<20} objects {:>9} ({} bytes)  commits {:>7} ({} bytes)\n",
                c.objects, c.object_bytes, c.commits, c.commit_bytes
            )
        };
        let mut text = format!("Local retention collection\nproject {}\n", self.project);
        text.push_str(&format!("plan hash {}\n", self.plan_hash));
        text.push_str(if self.applied {
            "result: applied\n"
        } else if self.already_applied {
            "result: this plan hash was already applied; nothing changed\n"
        } else {
            "result: nothing to delete; nothing changed\n"
        });
        text.push_str(&format!(
            "deleted {} objects ({} encoded bytes, {} stored bytes)\nretained\n",
            self.deleted.objects, self.deleted.object_bytes, self.deleted.stored_bytes
        ));
        text.push_str(&line("total", &self.retained));
        text.push_str(&line("reachable", &self.reachable));
        text.push_str(&line("unreachable (kept)", &self.unreachable_retained));
        let usage = |name: &str, u: &StorageUsage| {
            format!(
                "  {name:<8} bytes {} / {} (stored {})  objects {} / {}  commits {} / {}  branches {} / {}\n",
                u.bytes_used,
                u.limits.bytes,
                u.stored_bytes,
                u.objects_used,
                u.limits.objects,
                u.commits_used,
                u.limits.commits,
                u.branches_used,
                u.limits.branches
            )
        };
        text.push_str("capacity\n");
        text.push_str(&usage("before", &self.capacity_before));
        text.push_str(&usage("after", &self.capacity_after));
        if let Some(vacuum) = &self.vacuum {
            text.push_str(&format!(
                "vacuum: database+wal {} -> {} bytes\n",
                vacuum.file_bytes_before, vacuum.file_bytes_after
            ));
            if let Some(error) = &vacuum.error {
                text.push_str(&format!("vacuum FAILED (deletion stands): {error}\n"));
            }
        }
        text
    }
}

/// Points inside one run where a test can inject a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Locked,
    Verified,
    Deleted,
    Accounted,
}

pub(super) type Hook<'a> = dyn FnMut(Stage) -> Result<(), Error> + Send + 'a;

impl LocalRepository {
    /// Delete the staged objects of the plan whose hash the operator reviewed.
    ///
    /// Refuses unless the plan recomputed inside this call's write transaction has
    /// the same hash. Never deletes commits, signatures, refs or any object a
    /// stored commit reaches. A replay of an already applied hash changes nothing.
    pub async fn gc_apply(
        &self,
        plan_hash: &str,
        options: GcApplyOptions,
    ) -> Result<GcReport, Error> {
        self.gc_apply_with(plan_hash, options, &mut |_| Ok(()))
            .await
    }

    pub(super) async fn gc_apply_with(
        &self,
        plan_hash: &str,
        options: GcApplyOptions,
        hook: &mut Hook<'_>,
    ) -> Result<GcReport, Error> {
        let expected = plan_hash.to_ascii_lowercase();
        if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Invalid("retention plan hash"));
        }
        let mut report = self.apply(&expected, hook).await?;
        if options.vacuum {
            report.vacuum = Some(self.vacuum().await);
        }
        Ok(report)
    }

    async fn apply(&self, expected: &str, hook: &mut Hook<'_>) -> Result<GcReport, Error> {
        // IMMEDIATE: the writer lock is held before the first read, so nothing can
        // change between recomputing the plan and deleting.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        hook(Stage::Locked)?;
        let query_only: i64 = sqlx::query_scalar("pragma query_only")
            .fetch_one(&mut *tx)
            .await?;
        if query_only != 0 {
            return Err(Error::Invalid(
                "retention collection needs a writable repository",
            ));
        }
        let schema: i64 = sqlx::query_scalar("select version from local_meta where singleton=1")
            .fetch_one(&mut *tx)
            .await?;
        if schema != storage::SCHEMA_VERSION {
            return Err(Error::Invalid(
                "retention collection requires local schema 3",
            ));
        }
        let before = storage::usage(&mut tx, schema).await?;
        let analysis = retention::analyze(
            &mut tx,
            self.project,
            GcPlanOptions {
                top_reasons: 1,
                list: 0,
            },
        )
        .await?;
        let plan = &analysis.plan;
        if plan.accounting.consistent != Some(true) {
            return Err(Error::Invalid(
                "capacity accounting differs from the stored rows; refusing to collect",
            ));
        }
        let staged = plan
            .unreachable
            .categories
            .iter()
            .find(|c| c.category == "staged")
            .map(|c| c.counts.clone())
            .ok_or(Error::Invalid("retention plan accounting"))?;
        let unchanged = |plan: &retention::GcPlan, already_applied: bool| GcReport {
            format: FORMAT,
            project: self.project,
            plan_hash: expected.into(),
            applied: false,
            already_applied,
            deleted: Deleted::default(),
            retained: plan.retained.clone(),
            reachable: plan.reachable.clone(),
            unreachable_retained: plan.unreachable.counts.clone(),
            capacity_before: before.clone(),
            capacity_after: before.clone(),
            vacuum: None,
        };
        if plan.plan_hash != expected {
            let replay = already_applied(&mut tx, expected).await?;
            if replay {
                return Ok(unchanged(plan, true));
            }
            return Err(Error::Invalid(STALE));
        }
        if staged.objects == 0 {
            return Ok(unchanged(plan, false));
        }
        hook(Stage::Verified)?;

        // The immutability guard is dropped and restored inside this one
        // transaction: no other connection can observe the table unguarded, and a
        // rollback restores it by itself. Compare against the stored definition
        // afterwards so a corrupted guard is never silently recreated.
        let guard: String = sqlx::query_scalar(GUARD_SQL)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::Invalid("object immutability guard is missing"))?;
        sqlx::query("drop trigger objects_no_delete")
            .execute(&mut *tx)
            .await?;
        let deleted = delete_staged(&mut tx, &analysis.visited).await?;
        sqlx::query(&guard).execute(&mut *tx).await?;
        let restored: Option<String> = sqlx::query_scalar(GUARD_SQL)
            .fetch_optional(&mut *tx)
            .await?;
        if restored.as_deref() != Some(guard.as_str()) {
            return Err(Error::Invalid("object immutability guard was not restored"));
        }
        if deleted.objects != staged.objects || deleted.object_bytes != staged.object_bytes {
            return Err(Error::Invalid(
                "deleted rows differ from the retention plan",
            ));
        }
        hook(Stage::Deleted)?;

        let changed = sqlx::query(
            "update local_storage set bytes_used=bytes_used-?,stored_bytes=stored_bytes-?,objects_used=objects_used-? where singleton=1",
        )
        .bind(deleted.object_bytes as i64)
        .bind(deleted.stored_bytes as i64)
        .bind(deleted.objects as i64)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(Error::Invalid("capacity accounting row missing"));
        }
        let after = storage::usage(&mut tx, schema).await?;
        let (objects, object_bytes, stored_object_bytes): (i64, i64, i64) = sqlx::query_as(
            "select count(*),coalesce(sum(coalesce(size,length(bytes))),0),coalesce(sum(length(bytes)),0) from objects",
        )
        .fetch_one(&mut *tx)
        .await?;
        let (commits, commit_bytes): (i64, i64) =
            sqlx::query_as("select count(*),coalesce(sum(length(body)),0) from commits")
                .fetch_one(&mut *tx)
                .await?;
        let branches: i64 = sqlx::query_scalar("select count(*) from refs")
            .fetch_one(&mut *tx)
            .await?;
        if after.objects_used != objects
            || after.bytes_used != object_bytes + commit_bytes
            || after.stored_bytes != stored_object_bytes + commit_bytes
            || after.commits_used != commits
            || after.branches_used != branches
            || objects as u64 != analysis.visited.len() as u64
            || commits != before.commits_used
            || branches != before.branches_used
        {
            return Err(Error::Invalid(
                "capacity counters differ from a fresh recomputation after collection",
            ));
        }
        sqlx::raw_sql(LOG_SCHEMA).execute(&mut *tx).await?;
        sqlx::query(
            "insert into local_retention_log(plan_hash,deleted_objects,deleted_bytes,objects_after,bytes_after,applied_at_ms) values(?,?,?,?,?,cast(strftime('%s','now') as integer)*1000)",
        )
        .bind(expected)
        .bind(deleted.objects as i64)
        .bind(deleted.object_bytes as i64)
        .bind(objects)
        .bind(object_bytes + commit_bytes)
        .execute(&mut *tx)
        .await?;
        hook(Stage::Accounted)?;
        let mut retained = plan.retained.clone();
        retained.objects -= deleted.objects;
        retained.object_bytes -= deleted.object_bytes;
        retained.bytes -= deleted.object_bytes;
        let mut unreachable_retained = plan.unreachable.counts.clone();
        unreachable_retained.objects -= deleted.objects;
        unreachable_retained.object_bytes -= deleted.object_bytes;
        unreachable_retained.bytes -= deleted.object_bytes;
        let reachable = plan.reachable.clone();
        tx.commit().await?;
        Ok(GcReport {
            format: FORMAT,
            project: self.project,
            plan_hash: expected.into(),
            applied: true,
            already_applied: false,
            deleted,
            retained,
            reachable,
            unreachable_retained,
            capacity_before: before,
            capacity_after: after,
            vacuum: None,
        })
    }

    /// Runs after the deletion has committed, on its own connection, so a failure
    /// here can never undo or hide the deletion.
    pub(super) async fn vacuum(&self) -> Vacuum {
        let path = self.options.get_filename().to_path_buf();
        let size = |path: &Path| {
            let mut wal = path.as_os_str().to_owned();
            wal.push("-wal");
            let file = std::fs::metadata(path).map_or(0, |m| m.len());
            file + std::fs::metadata(std::path::PathBuf::from(wal)).map_or(0, |m| m.len())
        };
        let file_bytes_before = size(&path);
        let run = async {
            let mut connection =
                SqliteConnection::connect_with(&self.options.clone().create_if_missing(false))
                    .await?;
            sqlx::query("pragma wal_checkpoint(truncate)")
                .execute(&mut connection)
                .await?;
            sqlx::query("vacuum").execute(&mut connection).await?;
            sqlx::query("pragma wal_checkpoint(truncate)")
                .execute(&mut connection)
                .await?;
            connection.close().await
        };
        let error = run.await.err().map(|e| e.to_string());
        Vacuum {
            file_bytes_before,
            file_bytes_after: size(&path),
            error,
        }
    }
}

/// True when an earlier deleting run recorded exactly this plan hash.
async fn already_applied(connection: &mut SqliteConnection, hash: &str) -> Result<bool, Error> {
    let exists: bool = sqlx::query_scalar(
        "select exists(select 1 from sqlite_master where type='table' and name='local_retention_log')",
    )
    .fetch_one(&mut *connection)
    .await?;
    if !exists {
        return Ok(false);
    }
    Ok(
        sqlx::query_scalar("select exists(select 1 from local_retention_log where plan_hash=?)")
            .bind(hash)
            .fetch_one(&mut *connection)
            .await?,
    )
}

/// Delete every stored object outside `reached`, in key order. Returns rows and
/// payload bytes removed.
async fn delete_staged(
    connection: &mut SqliteConnection,
    reached: &HashSet<ObjectKey>,
) -> Result<Deleted, Error> {
    let (mut document, mut address) = (String::new(), String::new());
    let mut deleted = Deleted::default();
    loop {
        let rows: Vec<(String, String, i64, i64)> = sqlx::query_as(
            "select document,address,coalesce(size,length(bytes)),length(bytes) from objects where (document,address)>(?,?) order by document,address limit ?",
        )
        .bind(&document)
        .bind(&address)
        .bind(SCAN_PAGE)
        .fetch_all(&mut *connection)
        .await?;
        let Some(last) = rows.last() else { break };
        (document, address) = (last.0.clone(), last.1.clone());
        for (doc, addr, bytes, stored) in rows {
            let parsed =
                uuid::Uuid::parse_str(&doc).map_err(|_| Error::Invalid("stored document id"))?;
            let key = (
                parsed.as_u128(),
                retention::raw(&Address::try_from(addr.clone())?)?,
            );
            if reached.contains(&key) {
                continue;
            }
            let affected = sqlx::query("delete from objects where document=? and address=?")
                .bind(&doc)
                .bind(&addr)
                .execute(&mut *connection)
                .await?
                .rows_affected();
            if affected != 1 {
                return Err(Error::Invalid("object row vanished during collection"));
            }
            deleted.objects += 1;
            deleted.object_bytes += bytes as u64;
            deleted.stored_bytes += stored as u64;
        }
    }
    Ok(deleted)
}
