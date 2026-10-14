//! Operator-installed receipt authority and its epoch chain. The private keys stay
//! outside PostgreSQL. Epoch 1 is the key installed once; later epochs are signed
//! succession statements appended with operator credentials only (docs/REF-AUTHORITY-ROTATION.md).
use super::authority_epoch::{
    Kind, Seal, SealBuilder, SignedTransition, MAX_CHAIN_PAGE, MAX_EPOCH,
};
use anyhow::Context as _;
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use std::{collections::BTreeMap, io::Read, path::Path};
use uuid::Uuid;

pub fn load_key(path: &Path) -> anyhow::Result<Ed25519KeyPair> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(4097)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= 4096,
        "receipt signing key exceeds 4096 bytes"
    );
    Ed25519KeyPair::from_pkcs8_maybe_unchecked(&bytes)
        .map_err(|_| anyhow::anyhow!("invalid DER Ed25519 PKCS#8 receipt key"))
}

pub use super::authority_epoch::Revocation;
/// The epoch that signs new receipts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Active {
    pub authority: Uuid,
    pub epoch: u32,
    pub public_key: String,
    pub revocation: Option<Revocation>,
}
/// One epoch row with its stored statement (absent for epoch 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Epoch {
    pub epoch: u32,
    pub public_key: String,
    pub kind: String,
    pub installed_at_ms: i64,
    pub statement: Option<SignedTransition>,
    pub revocation: Option<Revocation>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub authority: Uuid,
    pub epoch: u32,
    pub statement_address: String,
    pub already_installed: bool,
}
/// Operator view used to prepare an offline signature. It is not trusted input:
/// installation re-checks everything against the database.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub version: u8,
    pub authority: Uuid,
    pub epoch: u32,
    pub public_key: String,
    pub sealed: Seal,
    pub revocation: Option<Revocation>,
    pub epochs: Vec<StateEpoch>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateEpoch {
    pub epoch: u32,
    pub kind: String,
    pub public_key: String,
    pub installed_at_ms: i64,
    pub statement_address: Option<String>,
    pub revoked: bool,
}

/// Requires operator credentials. An exact repeat is harmless; never overwrites.
pub async fn install(pool: &PgPool, public_key: &str) -> anyhow::Result<Uuid> {
    super::unhex(public_key, 32)?;
    crate::database::check_schema(pool).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("insert into project_ref_authority(singleton,public_key) values(true,$1) on conflict do nothing")
        .bind(public_key).execute(&mut *tx).await?;
    // The immutable anchor of epoch 1; a repeat stays valid after later rotations.
    let (id, installed): (Uuid, String) = sqlx::query_as(
        "select d.id,r.public_key from device_authority d join project_ref_authority r using(singleton)",
    )
    .fetch_one(&mut *tx)
    .await?;
    anyhow::ensure!(
        installed == public_key,
        "receipt authority already installed; replace its key with a supported signed rotation"
    );
    tx.commit().await?;
    Ok(id)
}

fn ms(time: DateTime<Utc>) -> i64 {
    time.timestamp_millis()
}
type RevocationRow = (i32, DateTime<Utc>, DateTime<Utc>, String);
/// Epoch row joined with its optional revocation marker.
type EpochRow = (
    i32,
    String,
    String,
    Option<Vec<u8>>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<String>,
);
fn marker(row: RevocationRow) -> Revocation {
    Revocation {
        epoch: row.0 as u32,
        effective_at_ms: ms(row.1),
        recorded_at_ms: ms(row.2),
        reason: row.3,
    }
}
async fn revocation(db: &mut PgConnection, epoch: u32) -> Result<Option<Revocation>, sqlx::Error> {
    Ok(sqlx::query_as::<_, RevocationRow>(
        "select epoch,effective_at,recorded_at,reason from project_ref_authority_revocation where epoch=$1",
    )
    .bind(epoch as i32)
    .fetch_optional(db)
    .await?
    .map(marker))
}

/// The newest epoch: the only key allowed to sign a new receipt.
pub async fn active(db: &mut PgConnection) -> Result<Active, sqlx::Error> {
    let (authority, epoch, public_key): (Uuid, i32, String) = sqlx::query_as(
        "select d.id,e.epoch,e.public_key from device_authority d, project_ref_authority_epoch e where d.singleton order by e.epoch desc limit 1",
    )
    .fetch_one(&mut *db)
    .await?;
    let revocation = revocation(db, epoch as u32).await?;
    Ok(Active {
        authority,
        epoch: epoch as u32,
        public_key,
        revocation,
    })
}
/// `(authority UUID, active public key)`.
pub async fn binding(db: &mut PgConnection) -> Result<(Uuid, String), sqlx::Error> {
    let active = active(db).await?;
    Ok((active.authority, active.public_key))
}
/// Every key that ever signed receipts, with its epoch (at most 1024 rows).
pub async fn keys(db: &mut PgConnection) -> Result<BTreeMap<String, u32>, sqlx::Error> {
    let rows: Vec<(String, i32)> =
        sqlx::query_as("select public_key,epoch from project_ref_authority_epoch order by epoch")
            .fetch_all(db)
            .await?;
    Ok(rows.into_iter().map(|(k, e)| (k, e as u32)).collect())
}
/// Epochs with `after < epoch <= upto`, ascending, at most `limit` (capped at 65 so
/// callers can detect a following page).
pub async fn epochs(
    db: &mut PgConnection,
    after: u32,
    upto: Option<u32>,
    limit: usize,
) -> Result<Vec<Epoch>, super::Error> {
    let rows: Vec<EpochRow> =
        sqlx::query_as(
            "select e.epoch,e.public_key,e.kind,e.statement,e.installed_at,r.effective_at,r.recorded_at,r.reason
             from project_ref_authority_epoch e left join project_ref_authority_revocation r using(epoch)
             where e.epoch>$1 and e.epoch<=$2 order by e.epoch limit $3",
        )
        .bind(after as i32)
        .bind(upto.unwrap_or(MAX_EPOCH) as i32)
        .bind(limit.min(MAX_CHAIN_PAGE + 1) as i64)
        .fetch_all(db)
        .await?;
    rows.into_iter()
        .map(
            |(epoch, public_key, kind, statement, installed, effective, recorded, reason)| {
                Ok(Epoch {
                    epoch: epoch as u32,
                    public_key,
                    kind,
                    installed_at_ms: ms(installed),
                    statement: statement
                        .map(|bytes| serde_json::from_slice::<SignedTransition>(&bytes))
                        .transpose()?,
                    revocation: match (effective, recorded, reason) {
                        (Some(effective), Some(recorded), Some(reason)) => Some(Revocation {
                            epoch: epoch as u32,
                            effective_at_ms: ms(effective),
                            recorded_at_ms: ms(recorded),
                            reason,
                        }),
                        _ => None,
                    },
                })
            },
        )
        .collect()
}
/// Cumulative seal over every stored receipt address, in the documented order.
pub async fn current_seal(db: &mut PgConnection) -> Result<Seal, super::Error> {
    let mut rows = sqlx::query_scalar::<_, String>(
        "select receipt_address from project_ref_event order by project_id,convert_to(branch,'UTF8'),generation",
    )
    .fetch(&mut *db);
    let mut seal = SealBuilder::new();
    while let Some(address) = rows.try_next().await? {
        seal.push(&address)?;
    }
    Ok(seal.finish())
}
/// API startup gate: the mounted key must be the active epoch's key.
pub async fn check_signer(db: &mut PgConnection, key: &Ed25519KeyPair) -> anyhow::Result<Active> {
    let active = active(db)
        .await
        .context("install the receipt public key with the separate operator command first")?;
    anyhow::ensure!(
        active.public_key == super::hex(key.public_key().as_ref()),
        "receipt signing key does not match the installed authority (active epoch {})",
        active.epoch
    );
    Ok(active)
}

/// Read-only operator view. Receipt writers are not paused: stop them before
/// signing, because a statement is valid only for the seal it names.
pub async fn state(pool: &PgPool) -> anyhow::Result<State> {
    crate::database::check_schema(pool).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("set transaction isolation level repeatable read read only")
        .execute(&mut *tx)
        .await?;
    let active = active(&mut tx)
        .await
        .context("no receipt authority is installed")?;
    let sealed = current_seal(&mut tx).await?;
    let mut epochs_out = Vec::new();
    let mut after = 0;
    loop {
        let page = epochs(&mut tx, after, None, MAX_CHAIN_PAGE).await?;
        let Some(last) = page.last() else { break };
        after = last.epoch;
        for epoch in page {
            let statement_address = epoch
                .statement
                .as_ref()
                .map(|s| s.verify().map(|a| a.as_str().to_owned()))
                .transpose()?;
            epochs_out.push(StateEpoch {
                epoch: epoch.epoch,
                kind: epoch.kind,
                public_key: epoch.public_key,
                installed_at_ms: epoch.installed_at_ms,
                statement_address,
                revoked: epoch.revocation.is_some(),
            });
        }
    }
    tx.commit().await?;
    Ok(State {
        version: 1,
        authority: active.authority,
        epoch: active.epoch,
        public_key: active.public_key,
        sealed,
        revocation: active.revocation,
        epochs: epochs_out,
    })
}

/// Append a dual-signed rotation. Serialised under a table lock that also pauses
/// receipt writers, so the seal in the statement is exact. An identical retry of
/// the currently active statement is a no-op.
pub async fn install_rotation(
    pool: &PgPool,
    statement: &SignedTransition,
) -> anyhow::Result<Installed> {
    install_transition(pool, statement, Kind::Rotation).await
}
/// Append a recovery (a recorded break). The previous epoch must carry a marker.
pub async fn install_recovery(
    pool: &PgPool,
    statement: &SignedTransition,
) -> anyhow::Result<Installed> {
    install_transition(pool, statement, Kind::Recovery).await
}

async fn install_transition(
    pool: &PgPool,
    statement: &SignedTransition,
    kind: Kind,
) -> anyhow::Result<Installed> {
    let address = statement
        .verify()
        .map_err(|e| anyhow::anyhow!("authority statement is invalid: {e}"))?;
    let t = &statement.transition;
    anyhow::ensure!(
        t.kind == kind,
        "this command installs a {} statement, not a {}",
        kind.as_str(),
        t.kind.as_str()
    );
    crate::database::check_schema(pool).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("lock table project_ref_event in share row exclusive mode")
        .execute(&mut *tx)
        .await?;
    let active = active(&mut tx)
        .await
        .context("install the first receipt authority key with install-ref-authority first")?;
    anyhow::ensure!(
        t.authority == active.authority,
        "statement names a different authority installation"
    );
    let stored: Option<String> = sqlx::query_scalar(
        "select statement_address from project_ref_authority_epoch where epoch=$1",
    )
    .bind(t.to_epoch as i32)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(stored) = stored {
        anyhow::ensure!(
            stored == address.as_str() && t.to_epoch == active.epoch,
            "epoch {} already exists with a different or superseded statement",
            t.to_epoch
        );
        return Ok(Installed {
            authority: active.authority,
            epoch: t.to_epoch,
            statement_address: stored,
            already_installed: true,
        });
    }
    anyhow::ensure!(
        t.from_epoch == active.epoch && t.old_key == active.public_key,
        "statement does not start at the active epoch {} (its key)",
        active.epoch
    );
    match kind {
        Kind::Rotation => anyhow::ensure!(
            active.revocation.is_none(),
            "epoch {} is revoked; only a recovery statement can replace it",
            active.epoch
        ),
        Kind::Recovery => anyhow::ensure!(
            active.revocation.is_some(),
            "record the revocation of epoch {} first (revoke-ref-authority)",
            active.epoch
        ),
    }
    let reused: Option<i32> =
        sqlx::query_scalar("select epoch from project_ref_authority_epoch where public_key=$1")
            .bind(&t.new_key)
            .fetch_optional(&mut *tx)
            .await?;
    anyhow::ensure!(reused.is_none(), "the new key is already used by an epoch");
    let seal = current_seal(&mut tx).await?;
    anyhow::ensure!(
        seal == t.sealed,
        "seal is stale: the database holds {} receipts; export the state again with writers stopped",
        seal.receipts
    );
    let bytes = serde_json::to_vec(statement)?;
    sqlx::query("insert into project_ref_authority_epoch(epoch,public_key,kind,predecessor,statement,statement_address,sealed_receipts,sealed_digest) values($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(t.to_epoch as i32)
        .bind(&t.new_key)
        .bind(kind.as_str())
        .bind(t.from_epoch as i32)
        .bind(bytes)
        .bind(address.as_str())
        .bind(seal.receipts as i64)
        .bind(&seal.digest)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Installed {
        authority: active.authority,
        epoch: t.to_epoch,
        statement_address: address.as_str().to_owned(),
        already_installed: false,
    })
}

/// Record that an epoch's key must not be trusted (default: the active epoch,
/// effective now). Stops receipt issuance at once when it is the active epoch.
pub async fn revoke(
    pool: &PgPool,
    reason: &str,
    effective_at: Option<&str>,
    epoch: Option<u32>,
) -> anyhow::Result<Revocation> {
    anyhow::ensure!(
        !reason.trim().is_empty() && reason.len() <= 500 && !reason.chars().any(char::is_control),
        "a reason of 1 to 500 characters without control characters is required"
    );
    crate::database::check_schema(pool).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("lock table project_ref_event in share row exclusive mode")
        .execute(&mut *tx)
        .await?;
    let active = active(&mut tx)
        .await
        .context("no receipt authority is installed")?;
    let target = epoch.unwrap_or(active.epoch);
    let exists: bool = sqlx::query_scalar(
        "select exists(select 1 from project_ref_authority_epoch where epoch=$1)",
    )
    .bind(target as i32)
    .fetch_one(&mut *tx)
    .await?;
    anyhow::ensure!(exists, "epoch {target} does not exist");
    anyhow::ensure!(
        revocation(&mut tx, target).await?.is_none(),
        "epoch {target} already has a revocation marker"
    );
    let row: RevocationRow = sqlx::query_as(
        "insert into project_ref_authority_revocation(epoch,effective_at,reason) values($1,coalesce($2::timestamptz,now()),$3) returning epoch,effective_at,recorded_at,reason",
    )
    .bind(target as i32)
    .bind(effective_at)
    .bind(reason)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(marker(row))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{attestation::tests::key, authority_epoch::Transition, hex};

    fn public(seed: u8) -> String {
        hex(key(seed).public_key().as_ref())
    }
    async fn statement(pool: &PgPool, kind: Kind, old: u8, new: u8) -> SignedTransition {
        let state = state(pool).await.unwrap();
        let transition = Transition {
            version: 1,
            kind,
            authority: state.authority,
            from_epoch: state.epoch,
            to_epoch: state.epoch + 1,
            old_key: public(old),
            new_key: public(new),
            sealed: state.sealed,
            reason: if kind == Kind::Recovery {
                "key lost".into()
            } else {
                String::new()
            },
            declared_at_ms: 1_700_000_000_000,
        };
        match kind {
            Kind::Rotation => {
                SignedTransition::sign_rotation(transition, &key(old), &key(new)).unwrap()
            }
            Kind::Recovery => SignedTransition::sign_recovery(transition, &key(new)).unwrap(),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn authority_install_is_idempotent_and_replacement_is_refused(pool: PgPool) {
        let key = Ed25519KeyPair::from_seed_unchecked(&[43; 32]).unwrap();
        let public = super::super::hex(key.public_key().as_ref());
        let authority = install(&pool, &public).await.unwrap();
        assert_eq!(install(&pool, &public).await.unwrap(), authority);
        assert!(install(&pool, &"f".repeat(64)).await.is_err());
        assert!(install(&pool, "invalid").await.is_err());
        assert!(
            sqlx::query("update project_ref_authority set public_key=repeat('f',64)")
                .execute(&pool)
                .await
                .is_err()
        );
        let mut connection = pool.acquire().await.unwrap();
        let active = active(&mut connection).await.unwrap();
        assert_eq!(
            (active.epoch, active.public_key.as_str()),
            (1, public.as_str())
        );
        check_signer(&mut connection, &key).await.unwrap();
        assert!(
            check_signer(&mut connection, &super::super::attestation::tests::key(44))
                .await
                .is_err()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn rotation_recovery_and_revocation_follow_the_documented_rules(pool: PgPool) {
        install(&pool, &public(7)).await.unwrap();
        // A rotation must be signed by the active key and start at the active epoch.
        let first = statement(&pool, Kind::Rotation, 7, 8).await;
        let mut wrong_old = first.clone();
        wrong_old.transition.old_key = public(9);
        assert!(install_rotation(&pool, &wrong_old).await.is_err());
        let installed = install_rotation(&pool, &first).await.unwrap();
        assert_eq!((installed.epoch, installed.already_installed), (2, false));
        assert!(
            install_rotation(&pool, &first)
                .await
                .unwrap()
                .already_installed
        );
        // Reordered: epoch 3 -> 4 before 2 -> 3 cannot exist.
        let mut ahead = statement(&pool, Kind::Rotation, 8, 9).await.transition;
        ahead.from_epoch = 3;
        ahead.to_epoch = 4;
        let skip = SignedTransition::sign_rotation(ahead, &key(8), &key(9)).unwrap();
        skip.verify().unwrap();
        assert!(install_rotation(&pool, &skip).await.is_err());
        let second = statement(&pool, Kind::Rotation, 8, 9).await;
        install_rotation(&pool, &second).await.unwrap();
        // A superseded statement is a replay, not a retry; key reuse is refused.
        assert!(install_rotation(&pool, &first).await.is_err());
        let reuse = statement(&pool, Kind::Rotation, 9, 7).await;
        assert!(install_rotation(&pool, &reuse).await.is_err());
        // A recovery needs a marker first; after one, rotation from that epoch is refused.
        let lost = statement(&pool, Kind::Recovery, 9, 10).await;
        assert!(install_recovery(&pool, &lost).await.is_err());
        let rotation_from_marked = statement(&pool, Kind::Rotation, 9, 11).await;
        let marker = revoke(&pool, "private key lost", None, None).await.unwrap();
        assert_eq!(marker.epoch, 3);
        assert!(revoke(&pool, "again", None, None).await.is_err());
        assert!(install_rotation(&pool, &rotation_from_marked)
            .await
            .is_err());
        assert!(install_recovery(&pool, &first).await.is_err());
        let installed = install_recovery(&pool, &lost).await.unwrap();
        assert_eq!(installed.epoch, 4);
        let mut connection = pool.acquire().await.unwrap();
        let active = active(&mut connection).await.unwrap();
        assert_eq!((active.epoch, active.public_key), (4, public(10)));
        assert!(active.revocation.is_none());
        let page = epochs(&mut connection, 1, None, MAX_CHAIN_PAGE)
            .await
            .unwrap();
        assert_eq!(
            page.iter()
                .map(|e| (e.epoch, e.kind.as_str()))
                .collect::<Vec<_>>(),
            vec![(2, "rotation"), (3, "rotation"), (4, "recovery")]
        );
        assert_eq!(page[1].revocation.as_ref().unwrap().epoch, 3);
        assert_eq!(keys(&mut connection).await.unwrap().len(), 4);
        // Statements are append-only for every login, including the owner.
        for sql in [
            "update project_ref_authority_epoch set kind='rotation'",
            "delete from project_ref_authority_epoch",
            "update project_ref_authority_revocation set reason='x'",
            "delete from project_ref_authority_revocation",
            "truncate project_ref_authority_epoch",
        ] {
            assert!(sqlx::query(sql).execute(&pool).await.is_err(), "{sql}");
        }
    }

    #[sqlx::test(migrations = false)]
    async fn an_installation_made_before_epochs_becomes_epoch_one_with_its_key(pool: PgPool) {
        use crate::database::MIGRATOR;
        // Schema 40 as shipped, an installed authority, then migration 41.
        for migration in MIGRATOR.iter().filter(|m| m.version <= 40) {
            sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
        }
        sqlx::query("insert into project_ref_authority(singleton,public_key) values(true,$1)")
            .bind(public(7))
            .execute(&pool)
            .await
            .unwrap();
        let epochs = MIGRATOR.iter().find(|m| m.version == 41).unwrap();
        sqlx::raw_sql(&epochs.sql).execute(&pool).await.unwrap();
        let rows: Vec<(i32, String, String)> =
            sqlx::query_as("select epoch,public_key,kind from project_ref_authority_epoch")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(rows, vec![(1, public(7), "genesis".to_owned())]);
        // The backfilled epoch behaves like a fresh install: rotation works.
        let mut connection = pool.acquire().await.unwrap();
        let active = active(&mut connection).await.unwrap();
        assert_eq!((active.epoch, active.public_key), (1, public(7)));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn stale_seal_and_concurrent_installs_are_refused(pool: PgPool) {
        install(&pool, &public(7)).await.unwrap();
        let signed = statement(&pool, Kind::Rotation, 7, 8).await;
        let competitor = statement(&pool, Kind::Rotation, 7, 9).await;
        let (a, b) = tokio::join!(
            install_rotation(&pool, &signed),
            install_rotation(&pool, &competitor)
        );
        assert_ne!(a.is_ok(), b.is_ok(), "exactly one concurrent rotation wins");
        let mut connection = pool.acquire().await.unwrap();
        assert_eq!(active(&mut connection).await.unwrap().epoch, 2);
        // The database itself refuses a stale seal even without the CLI's checks.
        let stale = statement(&pool, Kind::Rotation, 8, 10).await;
        let mut tampered = stale.clone();
        tampered.transition.sealed.receipts += 1;
        assert!(install_rotation(&pool, &tampered).await.is_err());
        sqlx::query("insert into project_ref_authority_epoch(epoch,public_key,kind,predecessor,statement,statement_address,sealed_receipts,sealed_digest) values(3,$1,'rotation',2,'{}','00',5,$2)")
            .bind(public(10)).bind("0".repeat(64)).execute(&pool).await.unwrap_err();
    }
}
