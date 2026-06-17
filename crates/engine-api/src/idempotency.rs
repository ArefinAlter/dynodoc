//! Idempotency keys for write endpoints (docs/18 §6), Postgres-backed for the PoC.
//!
//! Every write endpoint accepts an optional `Idempotency-Key` header. The contract: a
//! request carrying a key that was already processed (by the same identity, on the same
//! path) within the [`TTL`] window returns the *cached* response without re-executing —
//! so a client that retries a POST never appends a duplicate event.
//!
//! The full design uses Redis; the PoC uses the `idempotency_key` table (0011). The
//! mechanism is a **reserve-then-complete** two-step, both steps atomic:
//!
//! 1. **Reserve.** `INSERT ... ON CONFLICT DO NOTHING RETURNING` a row with a sentinel
//!    "pending" status. If the insert lands, this request *owns* execution. If it
//!    conflicts, a prior request already holds the key.
//! 2. **Replay-or-wait.** On conflict, read the existing row: a completed row replays
//!    its cached status+body; a still-pending row means a concurrent duplicate is
//!    in-flight — answered with `409 Conflict` so the client retries (the same retry
//!    that idempotency protects).
//! 3. **Complete.** The owner runs the handler, then `UPDATE`s its reservation with the
//!    real status + body.
//!
//! No key header → no caching; the handler just runs.

use chrono::Duration;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::ApiError;

/// How long a stored response is replayable (docs/18: 24 hours).
pub const TTL: Duration = Duration::hours(24);

/// The sentinel status of a reservation that has not completed yet. Never a real HTTP
/// status, so a "pending" row is unambiguous.
const PENDING_STATUS: i32 = 0;

/// The outcome of reserving an idempotency key for a write.
pub enum Reservation {
    /// This request owns execution; run the handler then call [`complete`].
    Fresh,
    /// A prior request already produced this response — replay it verbatim.
    Replay { status: u16, body: Value },
    /// A duplicate request is still in flight; the caller should retry shortly.
    InFlight,
}

/// Attempt to reserve `(identity_id, key, path)`. Live, completed rows replay; live,
/// pending rows are in-flight; otherwise this request reserves the key and runs.
///
/// Reserving also opportunistically clears an expired row for the key first, so a key
/// reused after the TTL window behaves as fresh.
pub async fn reserve(
    pool: &PgPool,
    identity_id: Uuid,
    key: &str,
    path: &str,
) -> Result<Reservation, ApiError> {
    // Drop an expired reservation for this exact key so a post-TTL reuse is fresh.
    sqlx::query(
        "delete from idempotency_key
         where identity_id = $1 and idempotency_key = $2 and created_at < now() - $3::interval",
    )
    .bind(identity_id)
    .bind(key)
    .bind(format!("{} seconds", TTL.num_seconds()))
    .execute(pool)
    .await?;

    // Reserve atomically: the row lands only if no live row exists for the key.
    let reserved: Option<(i32,)> = sqlx::query_as(
        "insert into idempotency_key
           (identity_id, idempotency_key, request_path, response_status, response_body)
         values ($1, $2, $3, $4, '{}'::jsonb)
         on conflict (identity_id, idempotency_key) do nothing
         returning response_status",
    )
    .bind(identity_id)
    .bind(key)
    .bind(path)
    .bind(PENDING_STATUS)
    .fetch_optional(pool)
    .await?;

    if reserved.is_some() {
        return Ok(Reservation::Fresh);
    }

    // The key already exists and is live. Read what it holds.
    let existing: Option<(String, i32, Value)> = sqlx::query_as(
        "select request_path, response_status, response_body
         from idempotency_key
         where identity_id = $1 and idempotency_key = $2",
    )
    .bind(identity_id)
    .bind(key)
    .fetch_optional(pool)
    .await?;

    match existing {
        // Same key, different endpoint: a client bug. Refuse rather than serve the
        // wrong body.
        Some((stored_path, _, _)) if stored_path != path => Err(ApiError::Conflict {
            reason: "idempotency key reused on a different endpoint".into(),
        }),
        Some((_, status, _)) if status == PENDING_STATUS => Ok(Reservation::InFlight),
        Some((_, status, body)) => Ok(Reservation::Replay {
            status: status as u16,
            body,
        }),
        // Raced with a concurrent expiry delete: treat as fresh next call.
        None => Ok(Reservation::InFlight),
    }
}

/// Persist the real response for a reservation owned by this request. Called once the
/// handler has produced its `(status, body)`.
pub async fn complete(
    pool: &PgPool,
    identity_id: Uuid,
    key: &str,
    status: u16,
    body: &Value,
) -> Result<(), ApiError> {
    sqlx::query(
        "update idempotency_key
         set response_status = $3, response_body = $4
         where identity_id = $1 and idempotency_key = $2",
    )
    .bind(identity_id)
    .bind(key)
    .bind(status as i32)
    .bind(body)
    .execute(pool)
    .await?;
    Ok(())
}

/// Release a reservation whose handler failed, so a retry can run rather than being
/// stuck behind a dead "pending" row. Best-effort: a failure to clean up is logged but
/// not surfaced (the original handler error is what matters).
pub async fn release(pool: &PgPool, identity_id: Uuid, key: &str) {
    if let Err(err) = sqlx::query(
        "delete from idempotency_key
         where identity_id = $1 and idempotency_key = $2 and response_status = $3",
    )
    .bind(identity_id)
    .bind(key)
    .bind(PENDING_STATUS)
    .execute(pool)
    .await
    {
        tracing::warn!(error = %err, "failed to release idempotency reservation");
    }
}
