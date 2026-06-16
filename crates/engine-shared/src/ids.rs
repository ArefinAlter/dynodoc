//! Identity newtypes for the dynodoc data model.
//!
//! ULID strings for log/tree identities (time-ordered, replica-unique); UUIDs for
//! db-generated row identities. Each is `#[sqlx(transparent)]`, so it encodes and
//! decodes exactly as its inner type — usable directly in queries and `FromRow`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable node identity (ULID). Never changes for a node's lifetime, never reused.
/// The load-bearing data-model decision (docs/04 §D.1).
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, sqlx::Type,
)]
#[sqlx(transparent)]
pub struct NodeId(pub String);

/// Event identity (ULID). Time-ordered, unique across replicas.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct EventId(pub String);

/// Document identity (UUID, db-generated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct DocumentId(pub Uuid);

/// Snapshot identity (UUID, db-generated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct SnapshotId(pub Uuid);

/// Identity (user) identity (UUID, db-generated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct IdentityId(pub Uuid);

/// Magic-link token identity (UUID, db-generated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct MagicLinkId(pub Uuid);

/// Refresh-token identity (UUID, db-generated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct RefreshTokenId(pub Uuid);
