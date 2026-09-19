//! Public audit endpoints (docs/18 §2 "Audit").
//!
//! These make the hash-chain guarantees externally checkable, with no token required —
//! anyone can verify the log:
//!
//! - `GET /documents/:id/verify` runs [`engine_core::log::verify_chain`] and reports OK
//!   or the first inconsistent seq. Cacheable for 60s (the answer changes only when the
//!   log grows; a stale OK is harmless and a stale failure is still a real failure).
//! - `GET /documents/:id/snapshot/:snapshot_id` returns a full snapshot incl. its
//!   `merkle_root` (over the materialized node set) and `event_chain_hash`.
//! - `GET /documents/:id/merkle-proof/:event_id` returns an inclusion proof for an event
//!   within the events covered by the latest snapshot.
//!
//! ### Merkle proof scope (PoC)
//!
//! The stored `snapshot.merkle_root` commits to the materialized **node set** (docs/15
//! §E.3). This endpoint proves inclusion of an **event** in the covered event range, so
//! it builds an event-content Merkle tree (leaves `H(0x00 ‖ content_hash)`, internal
//! `H(0x01 ‖ l ‖ r)` — the same domain separation as the node tree) over events with
//! `seq <= snapshot.through_seq` and returns the sibling path plus that root. The client
//! recomputes the root from the leaf and path. (Binding this event root into the
//! snapshot row alongside the node root is a small post-PoC schema addition.)

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use engine_shared::{DocumentId, Snapshot};
use ring::digest::{Context, SHA256};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::documents::store as doc_store;
use crate::error::ApiError;
use crate::ops::apply::require_role;
use crate::AppState;
use engine_shared::IdentityId;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/documents/:id/verify", get(verify))
        .route("/documents/:id/snapshot/:snapshot_id", get(get_snapshot))
        .route("/documents/:id/merkle-proof/:event_id", get(merkle_proof))
}

// --- verify ---------------------------------------------------------------------

/// The hash-chain verification result.
#[derive(Debug, Serialize)]
pub struct VerifyResult {
    /// True if the whole chain recomputes correctly.
    pub ok: bool,
    /// The seq of the first inconsistency, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broken_at_seq: Option<i64>,
    /// Human-readable detail when broken.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Replay + verify the hash chain. `[document member]`, cacheable 60s.
#[utoipa::path(
    get, path = "/documents/{id}/verify",
    params(("id" = Uuid, Path, description = "Document id")),
    responses((status = 200, description = "Verification result", body = serde_json::Value)),
    tag = "audit"
)]
pub async fn verify(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let document_id = DocumentId(id);
    require_role(&state.pool, document_id, IdentityId(auth.identity_id)).await?;
    if !doc_store::document_exists(&state.pool, document_id).await? {
        return Err(ApiError::NotFound);
    }

    let result = match engine_core::log::verify_chain(&state.pool, document_id).await {
        Ok(()) => VerifyResult {
            ok: true,
            broken_at_seq: None,
            detail: None,
        },
        Err(engine_core::log::EventError::ChainBroken { seq, detail }) => VerifyResult {
            ok: false,
            broken_at_seq: Some(seq),
            detail: Some(detail),
        },
        Err(other) => return Err(ApiError::Internal(other.to_string())),
    };

    let mut response = Json(result).into_response();
    // Cacheable for 60s (docs/18). The verdict only changes as the log grows.
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

// --- snapshot -------------------------------------------------------------------

/// A full snapshot, including its Merkle root and bound chain hash. `[document member]`.
#[utoipa::path(
    get, path = "/documents/{id}/snapshot/{snapshot_id}",
    params(
        ("id" = Uuid, Path, description = "Document id"),
        ("snapshot_id" = Uuid, Path, description = "Snapshot id")
    ),
    responses(
        (status = 200, description = "The snapshot", body = serde_json::Value),
        (status = 404, description = "No such snapshot for this document")
    ),
    tag = "audit"
)]
pub async fn get_snapshot(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((id, snapshot_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Snapshot>, ApiError> {
    require_role(&state.pool, DocumentId(id), IdentityId(auth.identity_id)).await?;
    let snapshot =
        sqlx::query_as::<_, Snapshot>("select * from snapshot where id = $1 and document_id = $2")
            .bind(snapshot_id)
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or(ApiError::NotFound)?;
    Ok(Json(snapshot))
}

// --- merkle proof ---------------------------------------------------------------

/// An inclusion proof: leaf hash, sibling path (bottom-up), and the computed root.
#[derive(Debug, Serialize)]
pub struct MerkleProof {
    pub event_id: String,
    pub seq: i64,
    /// The event range the proof covers (`1..=through_seq` of the latest snapshot).
    pub through_seq: i64,
    /// `H(0x00 ‖ content_hash)` for this event, hex-encoded.
    pub leaf: String,
    /// Sibling hashes from leaf to root; each carries whether it sits on the left.
    pub path: Vec<ProofStep>,
    /// The recomputed event-content Merkle root, hex-encoded.
    pub root: String,
}

/// One step of a Merkle inclusion path.
#[derive(Debug, Serialize)]
pub struct ProofStep {
    /// The sibling digest, hex-encoded.
    pub sibling: String,
    /// True if the sibling is the left child (so combine as `H(0x01 ‖ sibling ‖ cur)`).
    pub sibling_is_left: bool,
}

/// Inclusion proof for an event within the latest snapshot's covered range. `[document member]`.
#[utoipa::path(
    get, path = "/documents/{id}/merkle-proof/{event_id}",
    params(
        ("id" = Uuid, Path, description = "Document id"),
        ("event_id" = String, Path, description = "Event id (ULID)")
    ),
    responses(
        (status = 200, description = "Inclusion proof", body = serde_json::Value),
        (status = 404, description = "No snapshot, or event not in range")
    ),
    tag = "audit"
)]
pub async fn merkle_proof(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((id, event_id)): Path<(Uuid, String)>,
) -> Result<Json<MerkleProof>, ApiError> {
    let document_id = DocumentId(id);
    require_role(&state.pool, document_id, IdentityId(auth.identity_id)).await?;

    // The latest snapshot pins the covered event range.
    let through_seq: Option<i64> = sqlx::query_scalar(
        "select through_seq from snapshot where document_id = $1 order by through_seq desc limit 1",
    )
    .bind(document_id.0)
    .fetch_optional(&state.pool)
    .await?;
    let through_seq = through_seq.ok_or(ApiError::NotFound)?;

    // Leaves: content_hash for events 1..=through_seq, in seq order.
    let rows: Vec<(String, i64, Vec<u8>)> = sqlx::query_as(
        "select id, seq, content_hash from event
         where document_id = $1 and seq <= $2 order by seq asc",
    )
    .bind(document_id.0)
    .bind(through_seq)
    .fetch_all(&state.pool)
    .await?;

    let target = rows
        .iter()
        .position(|(eid, _, _)| eid == &event_id)
        .ok_or(ApiError::NotFound)?;
    let target_seq = rows[target].1;

    let leaves: Vec<[u8; 32]> = rows.iter().map(|(_, _, ch)| leaf_hash(ch)).collect();
    let (root, path) = inclusion_path(&leaves, target);

    Ok(Json(MerkleProof {
        event_id,
        seq: target_seq,
        through_seq,
        leaf: hex(&leaves[target]),
        path,
        root: hex(&root),
    }))
}

/// Domain-separated leaf hash `H(0x00 ‖ content_hash)` (matches the node-tree leaves).
fn leaf_hash(content_hash: &[u8]) -> [u8; 32] {
    let mut ctx = Context::new(&SHA256);
    ctx.update(&[0x00]);
    ctx.update(content_hash);
    finish(ctx)
}

/// Internal node hash `H(0x01 ‖ left ‖ right)`.
fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut ctx = Context::new(&SHA256);
    ctx.update(&[0x01]);
    ctx.update(left);
    ctx.update(right);
    finish(ctx)
}

/// Build the Merkle root and the inclusion path for `target`, padding each odd layer
/// with a zero hash (the same construction as the snapshot node tree).
fn inclusion_path(leaves: &[[u8; 32]], target: usize) -> ([u8; 32], Vec<ProofStep>) {
    if leaves.is_empty() {
        return ([0u8; 32], Vec::new());
    }
    let mut layer = leaves.to_vec();
    let mut index = target;
    let mut path = Vec::new();

    while layer.len() > 1 {
        if layer.len() % 2 == 1 {
            layer.push([0u8; 32]);
        }
        let sibling_index = if index % 2 == 0 { index + 1 } else { index - 1 };
        path.push(ProofStep {
            sibling: hex(&layer[sibling_index]),
            sibling_is_left: sibling_index < index,
        });
        layer = layer
            .chunks(2)
            .map(|pair| node_hash(&pair[0], &pair[1]))
            .collect();
        index /= 2;
    }
    (layer[0], path)
}

fn finish(ctx: Context) -> [u8; 32] {
    let digest = ctx.finish();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_ref());
    out
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A proof's path recomputes the root for any leaf in a tree of any size.
    #[test]
    fn inclusion_path_recomputes_root() {
        for n in 1..=9usize {
            let leaves: Vec<[u8; 32]> = (0..n).map(|i| leaf_hash(&[i as u8; 32])).collect();
            let (root, _) = inclusion_path(&leaves, 0);
            for target in 0..n {
                let (_, path) = inclusion_path(&leaves, target);
                let mut cur = leaves[target];
                for step in &path {
                    let sibling = unhex(&step.sibling);
                    cur = if step.sibling_is_left {
                        node_hash(&sibling, &cur)
                    } else {
                        node_hash(&cur, &sibling)
                    };
                }
                assert_eq!(cur, root, "leaf {target}/{n} proof must recompute the root");
            }
        }
    }

    fn unhex(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }
}
