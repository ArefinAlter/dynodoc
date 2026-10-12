//! Persistent admission budgets for retained local history. These count encoded
//! payloads and rows, not SQLite pages, WAL, backups or the separate trust journal.
use crate::repository::Error;
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

pub const SCHEMA_VERSION: i64 = 2;
const CAPACITY: &str = "local repository capacity exceeded";
const HELP: &str = "local storage capacity; inspect repository storage and explicitly increase limits to retain more history";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageLimits {
    pub bytes: i64,
    pub objects: i64,
    pub commits: i64,
    pub branches: i64,
}

impl Default for StorageLimits {
    fn default() -> Self {
        Self {
            bytes: 4 * 1024 * 1024 * 1024,
            objects: 1_000_000,
            commits: 100_000,
            branches: 4096,
        }
    }
}

impl StorageLimits {
    pub(super) fn validate(self) -> Result<(), Error> {
        if [self.bytes, self.objects, self.commits, self.branches]
            .into_iter()
            .any(|value| value < 0)
        {
            return Err(Error::Invalid("negative local storage limit"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageUsage {
    pub bytes_used: i64,
    pub objects_used: i64,
    pub commits_used: i64,
    pub branches_used: i64,
    pub limits: StorageLimits,
}

pub(super) fn object_error(error: sqlx::Error) -> crate::shared_checkpoint::Error {
    if is_capacity(&error) {
        crate::shared_checkpoint::Error::Limit(HELP)
    } else {
        error.into()
    }
}

pub(super) fn error(error: sqlx::Error) -> Error {
    if is_capacity(&error) {
        Error::Limit(HELP)
    } else {
        error.into()
    }
}

fn is_capacity(error: &sqlx::Error) -> bool {
    error.as_database_error().is_some_and(|e| {
        // SQLITE_CONSTRAINT_TRIGGER. Do not turn an unrelated I/O or integrity
        // failure into a user-adjustable capacity error.
        e.code().as_deref() == Some("1811") && e.message() == CAPACITY
    })
}

pub(super) async fn upgrade(connection: &mut SqliteConnection) -> Result<(), Error> {
    sqlx::raw_sql(include_str!("storage_v2.sql"))
        .execute(connection)
        .await?;
    Ok(())
}

pub(super) async fn usage(connection: &mut SqliteConnection) -> Result<StorageUsage, Error> {
    let (bytes_used, objects_used, commits_used, branches_used, bytes, objects, commits, branches):
        (i64,i64,i64,i64,i64,i64,i64,i64) = sqlx::query_as(
            "select bytes_used,objects_used,commits_used,branches_used,byte_limit,object_limit,commit_limit,branch_limit from local_storage where singleton=1"
        ).fetch_one(connection).await?;
    Ok(StorageUsage {
        bytes_used,
        objects_used,
        commits_used,
        branches_used,
        limits: StorageLimits {
            bytes,
            objects,
            commits,
            branches,
        },
    })
}
