//! Database operations for the auth flow.
//!
//! Each function is a single, focused query (or a short transaction) over the
//! identity / magic-link / refresh-token / access-list tables. Token lookups are by
//! **hash** — the plaintext token never touches the database. Consume and rotate are
//! written as conditional `UPDATE ... RETURNING` so the validity check and the
//! state change are one atomic statement (no check-then-act race / double-spend).

use chrono::{DateTime, Utc};
use engine_core::governance::Role;
use engine_shared::{DocumentId, Identity, IdentityId};
use sqlx::PgPool;
use uuid::Uuid;

use super::AuthError;

/// Find-or-create the identity for `email` (case-insensitive). A returning user keeps
/// their row. Provider names update only until the person saves their own profile.
pub async fn upsert_identity(
    pool: &PgPool,
    email: &str,
    display_name: Option<&str>,
) -> Result<Identity, AuthError> {
    let identity = sqlx::query_as::<_, Identity>(
        "insert into identity (email, display_name)
         values ($1, $2)
         on conflict (lower(email))
         do update set display_name = case when identity.profile_revision>0 then identity.display_name else coalesce(excluded.display_name, identity.display_name) end
         where identity.disabled_at is null
         returning *",
    )
    .bind(email)
    .bind(display_name)
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::TokenInvalid)?;
    Ok(identity)
}

/// Fetch an identity by id. A valid token for a missing identity is treated as invalid.
pub async fn get_identity(pool: &PgPool, identity_id: Uuid) -> Result<Identity, AuthError> {
    sqlx::query_as::<_, Identity>("select * from identity where id = $1 and disabled_at is null")
        .bind(identity_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AuthError::TokenInvalid)
}

/// Record a freshly issued magic link (only its hash is stored).
pub async fn issue_magic_link(
    pool: &PgPool,
    identity_id: IdentityId,
    token_hash: &[u8],
    expires_at: DateTime<Utc>,
) -> Result<(), AuthError> {
    sqlx::query("insert into magic_link (identity_id, token_hash, expires_at, session_generation) select id,$2,$3,session_generation from identity where id=$1 and disabled_at is null")
        .bind(identity_id.0)
        .bind(token_hash)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

/// Atomically consume a magic link: mark it used iff it is unconsumed and unexpired,
/// returning the identity it logs in. A second attempt (or an expired link) yields
/// [`AuthError::MagicLinkInvalid`].
pub async fn consume_magic_link(pool: &PgPool, token_hash: &[u8]) -> Result<Uuid, AuthError> {
    let identity_id: Option<Uuid> = sqlx::query_scalar(
        "update magic_link
         set consumed_at = now()
         where token_hash = $1 and consumed_at is null and expires_at > now()
           and exists(select 1 from identity i where i.id=magic_link.identity_id and i.disabled_at is null and i.session_generation=magic_link.session_generation)
         returning identity_id",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;
    identity_id.ok_or(AuthError::MagicLinkInvalid)
}

/// Record a freshly issued refresh token (only its hash is stored).
pub async fn issue_refresh(
    pool: &PgPool,
    identity_id: Uuid,
    token_hash: &[u8],
    expires_at: DateTime<Utc>,
) -> Result<(), AuthError> {
    let generation = session_generation(pool, identity_id).await?;
    issue_refresh_generation(pool, identity_id, token_hash, expires_at, generation).await
}
pub async fn issue_refresh_generation(
    pool: &PgPool,
    identity_id: Uuid,
    token_hash: &[u8],
    expires_at: DateTime<Utc>,
    generation: i64,
) -> Result<(), AuthError> {
    issue_bound_refresh(pool, identity_id, token_hash, expires_at, generation, None).await
}

pub async fn issue_bound_refresh(
    pool: &PgPool,
    identity_id: Uuid,
    token_hash: &[u8],
    expires_at: DateTime<Utc>,
    generation: i64,
    session_id: Option<Uuid>,
) -> Result<(), AuthError> {
    let result = sqlx::query("insert into refresh_token(identity_id,token_hash,expires_at,session_generation,session_id) select id,$2,$3,$4,$5 from identity where id=$1 and disabled_at is null and session_generation=$4").bind(identity_id).bind(token_hash).bind(expires_at).bind(generation).bind(session_id).execute(pool).await?;
    if result.rows_affected() != 1 {
        return Err(AuthError::TokenInvalid);
    }
    Ok(())
}

/// Rotate a refresh token: in one transaction, mark the presented token rotated (iff
/// valid — unrotated, unrevoked, unexpired) and issue a successor. A rotated, revoked,
/// expired, or unknown token yields [`AuthError::RefreshInvalid`] and changes nothing.
/// Single-use rotation makes a stolen-and-replayed refresh token detectable.
pub async fn rotate_refresh(
    pool: &PgPool,
    presented_hash: &[u8],
    new_hash: &[u8],
    new_expires: DateTime<Utc>,
) -> Result<Uuid, AuthError> {
    rotate_refresh_session(pool, presented_hash, new_hash, new_expires)
        .await
        .map(|(id, _)| id)
}
pub async fn rotate_refresh_session(
    pool: &PgPool,
    presented_hash: &[u8],
    new_hash: &[u8],
    new_expires: DateTime<Utc>,
) -> Result<(Uuid, i64), AuthError> {
    rotate_bound_refresh(pool, presented_hash, new_hash, new_expires)
        .await
        .map(|(id, generation, _)| (id, generation))
}

pub async fn rotate_bound_refresh(
    pool: &PgPool,
    presented_hash: &[u8],
    new_hash: &[u8],
    new_expires: DateTime<Utc>,
) -> Result<(Uuid, i64, Option<Uuid>), AuthError> {
    let mut tx = pool.begin().await?;

    // Rotation and logout lock the same identity before touching token rows. This
    // prevents a concurrent successor escaping a logout, in either lock order.
    sqlx::query("select i.id from identity i join refresh_token r on r.identity_id=i.id where r.token_hash=$1 for update of i")
        .bind(presented_hash).fetch_optional(&mut *tx).await?;
    let identity: Option<(Uuid,i64,Option<Uuid>)> = sqlx::query_as(
        "update refresh_token
         set rotated_at = now()
         where token_hash = $1
           and rotated_at is null and revoked_at is null and expires_at > now()
           and exists(select 1 from identity i where i.id=refresh_token.identity_id and i.disabled_at is null and i.session_generation=refresh_token.session_generation)
         returning identity_id,session_generation,session_id",
    )
    .bind(presented_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((identity_id, generation, session_id)) = identity else {
        // Nothing valid to rotate — roll back (the tx made no changes anyway).
        return Err(AuthError::RefreshInvalid);
    };

    sqlx::query(
        "insert into refresh_token (identity_id, token_hash, expires_at,session_generation,session_id) values ($1, $2, $3,$4,$5)",
    )
    .bind(identity_id)
    .bind(new_hash)
    .bind(new_expires)
    .bind(generation)
    .bind(session_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((identity_id, generation, session_id))
}

/// Revoke a login using any unexpired refresh secret in its rotation lineage.
/// Legacy tokens lack a login ID, so those logouts revoke the account generation.
/// Retries and unknown/expired secrets are idempotent and reveal no account state.
pub async fn revoke_login(pool: &PgPool, presented_hash: &[u8]) -> Result<(), AuthError> {
    let mut tx = pool.begin().await?;
    sqlx::query("select i.id from identity i join refresh_token r on r.identity_id=i.id where r.token_hash=$1 for update of i")
        .bind(presented_hash).fetch_optional(&mut *tx).await?;
    let login: Option<(Uuid, i64, Option<Uuid>)> = sqlx::query_as(
        "select r.identity_id,r.session_generation,r.session_id from refresh_token r join identity i on i.id=r.identity_id where r.token_hash=$1 and r.revoked_at is null and r.expires_at>now() and i.session_generation=r.session_generation")
        .bind(presented_hash).fetch_optional(&mut *tx).await?;
    if let Some((identity, generation, session)) = login {
        if let Some(session) = session {
            sqlx::query("update refresh_token set revoked_at=now() where identity_id=$1 and session_id=$2 and revoked_at is null")
                .bind(identity).bind(session).execute(&mut *tx).await?;
        } else {
            sqlx::query("update identity set session_generation=session_generation+1 where id=$1 and session_generation=$2")
                .bind(identity).bind(generation).execute(&mut *tx).await?;
            sqlx::query("update refresh_token set revoked_at=now() where identity_id=$1 and session_generation=$2 and revoked_at is null")
                .bind(identity).bind(generation).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// The role an identity holds on a document, if any (the access list). `None` means
/// the identity is not on the document's access list (no capability).
pub async fn role_for(
    pool: &PgPool,
    document_id: DocumentId,
    identity_id: Uuid,
) -> Result<Option<Role>, AuthError> {
    let role: Option<String> = sqlx::query_scalar("select effective_document_role($1,$2)")
        .bind(document_id.0)
        .bind(identity_id)
        .fetch_one(pool)
        .await?;
    // The CHECK constraint guarantees a parseable role; a surprise value means "no role".
    Ok(role.and_then(|r| r.parse().ok()))
}

/// Grant (or change) an identity's role on a document — the access-list write the
/// stage-18 sharing endpoint will call; exposed now so tests can seed access.
pub async fn grant_access(
    pool: &PgPool,
    document_id: DocumentId,
    identity_id: Uuid,
    role: Role,
) -> Result<(), AuthError> {
    sqlx::query(
        "insert into document_access (document_id, identity_id, role)
         values ($1, $2, $3)
         on conflict (document_id, identity_id) do update set role = excluded.role",
    )
    .bind(document_id.0)
    .bind(identity_id)
    .bind(role.as_str())
    .execute(pool)
    .await?;
    Ok(())
}

/// Consult revocation state on every authenticated request, not only at token mint.
pub async fn ensure_active(pool: &PgPool, id: Uuid) -> Result<(), AuthError> {
    let active: bool = sqlx::query_scalar(
        "select exists(select 1 from identity where id=$1 and disabled_at is null)",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    if active {
        Ok(())
    } else {
        Err(AuthError::TokenInvalid)
    }
}

/// Restoration never reactivates a bearer token issued before an account removal.
pub async fn ensure_session(
    pool: &PgPool,
    identity: Uuid,
    generation: i64,
) -> Result<(), AuthError> {
    ensure_bound_session(pool, identity, generation, None).await
}

pub async fn ensure_bound_session(
    pool: &PgPool,
    identity: Uuid,
    generation: i64,
    session: Option<Uuid>,
) -> Result<(), AuthError> {
    let active: bool = sqlx::query_scalar("select exists(select 1 from identity i where i.id=$1 and i.disabled_at is null and i.session_generation=$2 and ($3::uuid is null or exists(select 1 from refresh_token r where r.identity_id=i.id and r.session_generation=$2 and r.session_id=$3 and r.rotated_at is null and r.revoked_at is null and r.expires_at>now())))").bind(identity).bind(generation).bind(session).fetch_one(pool).await?;
    if active {
        Ok(())
    } else {
        Err(AuthError::TokenInvalid)
    }
}

pub async fn session_generation(pool: &PgPool, identity: Uuid) -> Result<i64, AuthError> {
    sqlx::query_scalar(
        "select session_generation from identity where id=$1 and disabled_at is null",
    )
    .bind(identity)
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::TokenInvalid)
}
