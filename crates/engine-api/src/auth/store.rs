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
/// their row; a new `display_name` updates it, a missing one leaves it untouched.
pub async fn upsert_identity(
    pool: &PgPool,
    email: &str,
    display_name: Option<&str>,
) -> Result<Identity, AuthError> {
    let identity = sqlx::query_as::<_, Identity>(
        "insert into identity (email, display_name)
         values ($1, $2)
         on conflict (lower(email))
         do update set display_name = coalesce(excluded.display_name, identity.display_name)
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
    let result = sqlx::query("insert into refresh_token(identity_id,token_hash,expires_at,session_generation) select id,$2,$3,$4 from identity where id=$1 and disabled_at is null and session_generation=$4").bind(identity_id).bind(token_hash).bind(expires_at).bind(generation).execute(pool).await?;
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
    let mut tx = pool.begin().await?;

    let identity: Option<(Uuid,i64)> = sqlx::query_as(
        "update refresh_token
         set rotated_at = now()
         where token_hash = $1
           and rotated_at is null and revoked_at is null and expires_at > now()
           and exists(select 1 from identity i where i.id=refresh_token.identity_id and i.disabled_at is null and i.session_generation=refresh_token.session_generation)
         returning identity_id,session_generation",
    )
    .bind(presented_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((identity_id, generation)) = identity else {
        // Nothing valid to rotate — roll back (the tx made no changes anyway).
        return Err(AuthError::RefreshInvalid);
    };

    sqlx::query(
        "insert into refresh_token (identity_id, token_hash, expires_at,session_generation) values ($1, $2, $3,$4)",
    )
    .bind(identity_id)
    .bind(new_hash)
    .bind(new_expires)
    .bind(generation)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((identity_id, generation))
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
    let active: bool = sqlx::query_scalar("select exists(select 1 from identity where id=$1 and disabled_at is null and session_generation=$2)").bind(identity).bind(generation).fetch_one(pool).await?;
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
