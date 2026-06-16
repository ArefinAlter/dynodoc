//! `sqlx::FromRow` structs — one per table, mirroring the schema in `migrations/`.
//!
//! These are faithful row bindings: every column, typed. The `type`/`status`
//! columns are carried as `String` (Postgres stores them as `CHECK`-constrained
//! text); parse them into [`crate::NodeType`] / [`crate::DocumentStatus`] /
//! [`crate::EventType`] for typed handling. The 32-byte hash columns are `Vec<u8>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;

use crate::ids::{
    DocumentId, EventId, IdentityId, MagicLinkId, NodeId, RefreshTokenId, SnapshotId,
};

/// A row of `document` — materialized current state of an instrument.
#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct Document {
    pub id: DocumentId,
    pub title: String,
    pub languages: Vec<String>,
    /// One of [`crate::DocumentStatus`] (`draft` | `deployed` | `archived`).
    pub status: String,
    pub settings: Value,
    pub deployed_snapshot_id: Option<SnapshotId>,
    pub created_by: Option<IdentityId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A row of `node` — one stable-identity node in the instrument tree.
#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub document_id: DocumentId,
    pub parent_id: Option<NodeId>,
    /// One of [`crate::NodeType`].
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    pub node_type: String,
    /// Dense order key (fractional index) for sibling ordering.
    pub pos: String,
    pub current_fields: Value,
    pub var_name: Option<String>,
    pub deleted: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A row of `event` — one immutable, hash-chained entry in the log of record.
#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub document_id: DocumentId,
    /// Monotonic per-document sequence (the fold order).
    pub seq: i64,
    /// One of [`crate::EventType`].
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    pub event_type: String,
    pub actor_id: IdentityId,
    pub target_node_id: Option<NodeId>,
    pub payload: Value,
    /// SHA-256 of the event's canonical content (32 bytes).
    pub content_hash: Vec<u8>,
    /// The previous event's `chain_hash` (32 zero bytes for a document's genesis).
    pub prev_chain_hash: Vec<u8>,
    /// SHA-256(prev_chain_hash || content_hash) (32 bytes).
    pub chain_hash: Vec<u8>,
    pub created_at: DateTime<Utc>,
}

/// A row of `snapshot` — a snapshotted fold with a Merkle root over its events.
#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: SnapshotId,
    pub document_id: DocumentId,
    pub through_seq: i64,
    pub state: Value,
    /// Merkle root over the covered events (32 bytes).
    pub merkle_root: Vec<u8>,
    /// The `event.chain_hash` at `through_seq`, binding the snapshot to the chain.
    pub event_chain_hash: Vec<u8>,
    pub created_at: DateTime<Utc>,
}

/// A row of `identity` — an actor (PoC: email + magic-link auth).
#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct Identity {
    pub id: IdentityId,
    pub email: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A row of `magic_link` — a one-time login token (stored hashed).
#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct MagicLink {
    pub id: MagicLinkId,
    pub identity_id: IdentityId,
    /// SHA-256 of the issued token (32 bytes); the token itself is never stored.
    pub token_hash: Vec<u8>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// A row of `document_access` — an identity's role on a document (the access list).
#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct DocumentAccess {
    pub document_id: DocumentId,
    pub identity_id: IdentityId,
    /// The PoC role (`author` | `reviewer` | `auditor`), stored as text; parsed into
    /// `engine_core::governance::Role` at the API boundary.
    pub role: String,
    pub created_at: DateTime<Utc>,
}

/// A row of `refresh_token` — a rotating session-continuation token (stored hashed).
#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct RefreshToken {
    pub id: RefreshTokenId,
    pub identity_id: IdentityId,
    /// SHA-256 of the issued opaque token (32 bytes); the token itself is never stored.
    pub token_hash: Vec<u8>,
    pub expires_at: DateTime<Utc>,
    /// Set when this token has been exchanged for a successor (rotation).
    pub rotated_at: Option<DateTime<Utc>>,
    /// Set when explicitly revoked.
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
