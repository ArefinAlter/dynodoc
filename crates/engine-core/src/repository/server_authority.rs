//! Operator-installed receipt authority. The private key stays outside PostgreSQL.
//! Rotation is deliberately refused until a dual-signed server transition exists.
use ring::signature::Ed25519KeyPair;
use sqlx::{PgConnection, PgPool};
use std::{io::Read, path::Path};
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

/// Requires operator credentials. An exact repeat is harmless; never overwrites.
pub async fn install(pool: &PgPool, public_key: &str) -> anyhow::Result<Uuid> {
    super::unhex(public_key, 32)?;
    crate::database::check_schema(pool).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("insert into project_ref_authority(singleton,public_key) values(true,$1) on conflict do nothing")
        .bind(public_key).execute(&mut *tx).await?;
    let (id, installed) = binding(&mut tx).await?;
    anyhow::ensure!(
        installed == public_key,
        "receipt authority already installed; key replacement requires a supported signed rotation"
    );
    tx.commit().await?;
    Ok(id)
}

pub async fn binding(db: &mut PgConnection) -> Result<(Uuid, String), sqlx::Error> {
    sqlx::query_as("select d.id,r.public_key from device_authority d join project_ref_authority r using(singleton)")
        .fetch_one(db).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::KeyPair;

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
    }
}
