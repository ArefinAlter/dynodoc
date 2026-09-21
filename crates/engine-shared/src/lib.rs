//! Shared types for the dynodoc engine.
//!
//! - [`ids`]: identity newtypes (ULID / UUID) used across the log, tree, and auth.
//! - [`enums`]: domain vocabulary mirroring the schema's `CHECK`-constrained columns.
//! - [`payload`]: the typed, internally-tagged semantic event vocabulary.
//! - [`rows`]: `sqlx::FromRow` structs, one per table in `migrations/`.
//!
//! Event-construction and fold logic live in `engine-core` (stage 14+); this crate
//! is the shared data-model vocabulary the rest of the workspace builds on.

pub mod enums;
pub mod ids;
pub mod payload;
pub mod richtext;
pub mod rows;

pub use enums::{DocumentStatus, EventType, NodeType, ParseEnumError};
pub use ids::{DocumentId, EventId, IdentityId, MagicLinkId, NodeId, RefreshTokenId, SnapshotId};
pub use payload::EventPayload;
pub use rows::{
    Document, DocumentAccess, Event, Identity, MagicLink, Node, RefreshToken, Snapshot,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newtypes_roundtrip_json() {
        let n = NodeId("01J0".to_string());
        let s = serde_json::to_string(&n).unwrap();
        assert_eq!(serde_json::from_str::<NodeId>(&s).unwrap(), n);
    }
}
