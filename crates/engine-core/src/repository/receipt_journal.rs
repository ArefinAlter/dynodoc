//! Durable client trust journal, intentionally separate from a project/remote DB.
//! The local filesystem and the initial authority-key choice remain trusted.
use super::{
    attestation::{Pin, Scope, SignedReceipt, SignedRotation},
    Error,
};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqliteConnection, SqlitePool,
};
use std::{path::Path, time::Duration};

const SCHEMA: &str = "
create table if not exists receipt_journal_meta(singleton integer primary key check(singleton=1),version integer not null);
insert into receipt_journal_meta values(1,1) on conflict(singleton) do nothing;
create table if not exists pins(authority text not null,project text not null,branch text not null,body blob not null check(length(body)<=8192),primary key(authority,project,branch));
create table if not exists receipts(address text primary key,body blob not null check(length(body)<=8192));
create table if not exists rotations(address text primary key,body blob not null check(length(body)<=8192));
create trigger if not exists receipts_no_update before update on receipts begin select raise(abort,'immutable receipt'); end;
create trigger if not exists receipts_no_delete before delete on receipts begin select raise(abort,'immutable receipt'); end;
create trigger if not exists rotations_no_update before update on rotations begin select raise(abort,'immutable rotation'); end;
create trigger if not exists rotations_no_delete before delete on rotations begin select raise(abort,'immutable rotation'); end;
";

#[derive(Clone)]
pub struct ReceiptJournal {
    pool: SqlitePool,
}

async fn load(connection: &mut SqliteConnection, scope: &Scope) -> Result<Option<Pin>, Error> {
    let bytes: Option<Vec<u8>> = sqlx::query_scalar(
        "select body from pins where authority=? and project=? and branch=? and length(body)<=8192",
    )
    .bind(scope.authority.to_string())
    .bind(scope.project.to_string())
    .bind(&scope.branch)
    .fetch_optional(connection)
    .await?;
    bytes
        .map(|bytes| {
            let pin: Pin = serde_json::from_slice(&bytes)?;
            pin.validate()?;
            if pin.scope() != scope {
                return Err(Error::Invalid("stored pin scope"));
            }
            Ok(pin)
        })
        .transpose()
}
async fn save(connection: &mut SqliteConnection, pin: &Pin) -> Result<(), Error> {
    let scope = pin.scope();
    sqlx::query("update pins set body=? where authority=? and project=? and branch=?")
        .bind(serde_json::to_vec(pin)?)
        .bind(scope.authority.to_string())
        .bind(scope.project.to_string())
        .bind(&scope.branch)
        .execute(connection)
        .await?;
    Ok(())
}
async fn record(connection: &mut SqliteConnection, receipt: &SignedReceipt) -> Result<(), Error> {
    let address = receipt.verify()?;
    let bytes = serde_json::to_vec(receipt)?;
    sqlx::query("insert into receipts(address,body) values(?,?) on conflict(address) do nothing")
        .bind(address.as_str())
        .bind(&bytes)
        .execute(&mut *connection)
        .await?;
    let existing: Vec<u8> =
        sqlx::query_scalar("select body from receipts where address=? and length(body)<=8192")
            .bind(address.as_str())
            .fetch_one(connection)
            .await?;
    if existing != bytes {
        return Err(Error::Invalid("stored receipt mismatch"));
    }
    Ok(())
}
impl ReceiptJournal {
    pub async fn open(path: &Path) -> Result<Self, Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true)
                    .journal_mode(SqliteJournalMode::Wal)
                    .synchronous(SqliteSynchronous::Full)
                    .foreign_keys(true)
                    .busy_timeout(Duration::from_secs(5)),
            )
            .await?;
        let mut tx = pool.begin().await?;
        let (occupied, journal): (bool, bool) = sqlx::query_as("select exists(select 1 from sqlite_master where type='table' and name not like 'sqlite_%'),exists(select 1 from sqlite_master where type='table' and name='receipt_journal_meta')")
            .fetch_one(&mut *tx).await?;
        if occupied && !journal {
            return Err(Error::Invalid("use a separate client receipt journal"));
        }
        if journal {
            let version: i64 =
                sqlx::query_scalar("select version from receipt_journal_meta where singleton=1")
                    .fetch_one(&mut *tx)
                    .await?;
            if version != 1 {
                return Err(Error::Invalid("receipt journal version"));
            }
        }
        sqlx::raw_sql(SCHEMA).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Self { pool })
    }
    pub async fn close(self) {
        self.pool.close().await;
    }
    async fn write(&self) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>, Error> {
        let mut tx = self.pool.begin().await?;
        // Reserve the single writer before reading the current pin.
        sqlx::query("update receipt_journal_meta set version=version where singleton=1")
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
    pub async fn pin(&self, scope: &Scope) -> Result<Option<Pin>, Error> {
        let mut connection = self.pool.acquire().await?;
        load(&mut connection, scope).await
    }
    /// One explicit bootstrap per scope. Never overwrites an existing/revoked pin.
    pub async fn trust_checkpoint(
        &self,
        scope: &Scope,
        key: &str,
        receipt: SignedReceipt,
    ) -> Result<Pin, Error> {
        let pin = Pin::trust_checkpoint(scope, key, receipt)?;
        let mut tx = self.write().await?;
        if load(&mut tx, scope).await?.is_some() {
            return Err(Error::StaleRef);
        }
        record(&mut tx, pin.receipt()).await?;
        sqlx::query("insert into pins(authority,project,branch,body) values(?,?,?,?)")
            .bind(scope.authority.to_string())
            .bind(scope.project.to_string())
            .bind(&scope.branch)
            .bind(serde_json::to_vec(&pin)?)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(pin)
    }
    /// Verify a bounded contiguous proof and persist it with the pin atomically.
    pub async fn advance(&self, scope: &Scope, proof: &[SignedReceipt]) -> Result<Pin, Error> {
        let mut tx = self.write().await?;
        let pin = load(&mut tx, scope)
            .await?
            .ok_or(Error::Invalid("untrusted receipt scope"))?;
        let next = pin.advance(proof)?;
        for receipt in proof {
            record(&mut tx, receipt).await?;
        }
        save(&mut tx, &next).await?;
        tx.commit().await?;
        Ok(next)
    }
    pub async fn rotate(
        &self,
        scope: &Scope,
        transition: &SignedRotation,
        receipt: &SignedReceipt,
    ) -> Result<Pin, Error> {
        let mut tx = self.write().await?;
        let pin = load(&mut tx, scope)
            .await?
            .ok_or(Error::Invalid("untrusted receipt scope"))?;
        let address = transition.rotation.address()?;
        let bytes = serde_json::to_vec(transition)?;
        let existing: Option<Vec<u8>> =
            sqlx::query_scalar("select body from rotations where address=? and length(body)<=8192")
                .bind(address.as_str())
                .fetch_optional(&mut *tx)
                .await?;
        // A lost-ACK retry only succeeds for an already recorded exact rotation.
        if existing.as_ref() == Some(&bytes) && pin.receipt() == receipt && !pin.revoked() {
            tx.commit().await?;
            return Ok(pin);
        }
        let next = pin.rotate(transition, receipt)?;
        sqlx::query(
            "insert into rotations(address,body) values(?,?) on conflict(address) do nothing",
        )
        .bind(address.as_str())
        .bind(&bytes)
        .execute(&mut *tx)
        .await?;
        let existing: Vec<u8> =
            sqlx::query_scalar("select body from rotations where address=? and length(body)<=8192")
                .bind(address.as_str())
                .fetch_one(&mut *tx)
                .await?;
        if existing != bytes {
            return Err(Error::Invalid("stored rotation mismatch"));
        }
        record(&mut tx, receipt).await?;
        save(&mut tx, &next).await?;
        tx.commit().await?;
        Ok(next)
    }
    /// Durable local revocation. No receipt, retry or signed rotation can undo it.
    pub async fn revoke(&self, scope: &Scope) -> Result<Pin, Error> {
        let mut tx = self.write().await?;
        let mut pin = load(&mut tx, scope)
            .await?
            .ok_or(Error::Invalid("untrusted receipt scope"))?;
        pin.revoke();
        save(&mut tx, &pin).await?;
        tx.commit().await?;
        Ok(pin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{
        attestation::{
            tests::{key, scope, signed},
            Receipt,
        },
        hex,
        local::LocalRepository,
    };
    use ring::signature::KeyPair;

    fn path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "dynodoc-receipt-test-{}.sqlite",
            uuid::Uuid::new_v4()
        ))
    }
    async fn remove(path: &Path) {
        for attempt in 0..20 {
            match std::fs::remove_file(path) {
                Ok(()) => return,
                Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) && attempt < 19 => {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                Err(error) => panic!("owned receipt journal cleanup: {error}"),
            }
        }
    }
    #[tokio::test]
    async fn durable_pins_retry_atomically_and_only_one_concurrent_fork_wins() {
        let path = path();
        let scope = scope(uuid::Uuid::new_v4());
        let journal = ReceiptJournal::open(&path).await.unwrap();
        let a = signed(scope.clone(), 1, None, 7);
        let key = hex(key(7).public_key().as_ref());
        journal
            .trust_checkpoint(&scope, &key, a.clone())
            .await
            .unwrap();
        let b = signed(scope.clone(), 2, Some(a.verify().unwrap()), 7);
        let c = signed(scope.clone(), 3, Some(b.verify().unwrap()), 7);
        let proof = [b.clone(), c.clone()];
        let accepted = journal.advance(&scope, &proof).await.unwrap();
        assert_eq!(journal.advance(&scope, &proof).await.unwrap(), accepted);
        let d = signed(scope.clone(), 4, Some(c.verify().unwrap()), 7);
        let bad = signed(scope.clone(), 5, Some(a.verify().unwrap()), 7);
        assert!(journal.advance(&scope, &[d.clone(), bad]).await.is_err());
        assert_eq!(journal.pin(&scope).await.unwrap().unwrap(), accepted);
        let count: i64 = sqlx::query_scalar("select count(*) from receipts")
            .fetch_one(&journal.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 3,
            "invalid proof must roll back all rows and the pin"
        );
        let mut alternative: Receipt = d.receipt.clone();
        alternative.issued_at_ms += 1;
        let alternative =
            SignedReceipt::sign(alternative, &super::super::attestation::tests::key(7)).unwrap();
        let left = [d.clone()];
        let right = [alternative.clone()];
        let (first, second) = tokio::join!(
            journal.advance(&scope, &left),
            journal.advance(&scope, &right)
        );
        assert_ne!(first.is_ok(), second.is_ok());
        assert!(sqlx::query("update receipts set body=body")
            .execute(&journal.pool)
            .await
            .is_err());
        journal.close().await;
        let reopened = ReceiptJournal::open(&path).await.unwrap();
        let pin = reopened.pin(&scope).await.unwrap().unwrap();
        assert_eq!(pin.receipt().receipt.head.generation, 4);
        assert!(reopened.advance(&scope, &[a]).await.is_err());
        reopened.close().await;
        remove(&path).await;
    }
    #[tokio::test]
    async fn rotation_and_revocation_survive_reopen_and_cannot_be_reset_by_bootstrap() {
        let path = path();
        let scope = scope(uuid::Uuid::new_v4());
        let journal = ReceiptJournal::open(&path).await.unwrap();
        let a = signed(scope.clone(), 1, None, 7);
        let trusted = hex(key(7).public_key().as_ref());
        let pin = journal
            .trust_checkpoint(&scope, &trusted, a.clone())
            .await
            .unwrap();
        let transition = SignedRotation::sign(&pin, &key(7), &key(8)).unwrap();
        let b = signed(scope.clone(), 2, Some(a.verify().unwrap()), 8);
        let rotated = journal.rotate(&scope, &transition, &b).await.unwrap();
        assert_eq!(
            journal.rotate(&scope, &transition, &b).await.unwrap(),
            rotated
        );
        journal.revoke(&scope).await.unwrap();
        journal.close().await;
        let journal = ReceiptJournal::open(&path).await.unwrap();
        assert!(journal.pin(&scope).await.unwrap().unwrap().revoked());
        assert!(journal.advance(&scope, &[]).await.is_err());
        assert!(journal.rotate(&scope, &transition, &b).await.is_err());
        assert!(journal.trust_checkpoint(&scope, &trusted, a).await.is_err());
        assert!(sqlx::query("delete from rotations")
            .execute(&journal.pool)
            .await
            .is_err());
        journal.close().await;
        remove(&path).await;
    }
    #[tokio::test]
    async fn journal_refuses_the_project_database_and_wrong_bootstrap_authority() {
        let file = path();
        let project = uuid::Uuid::new_v4();
        let repo = LocalRepository::open(&file, project).await.unwrap();
        assert!(ReceiptJournal::open(&file).await.is_err());
        repo.close().await;
        remove(&file).await;
        let file = path();
        let journal = ReceiptJournal::open(&file).await.unwrap();
        let scope = scope(project);
        let receipt = signed(scope.clone(), 1, None, 7);
        assert!(journal
            .trust_checkpoint(&scope, &hex(key(8).public_key().as_ref()), receipt.clone())
            .await
            .is_err());
        let mut wrong_scope = scope.clone();
        wrong_scope.project = uuid::Uuid::new_v4();
        assert!(journal
            .trust_checkpoint(&wrong_scope, &hex(key(7).public_key().as_ref()), receipt)
            .await
            .is_err());
        assert!(journal.pin(&scope).await.unwrap().is_none());
        journal.close().await;
        remove(&file).await;
    }
}
