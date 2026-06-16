//! Shared types for the dynodoc engine.
//!
//! Stage 12 scaffolding only: this crate currently exposes the identity newtypes
//! that the schema (stage 13) and event log (stage 14) build on. Event and node
//! types land in later stages.

use serde::{Deserialize, Serialize};

/// Stable node identity (ULID). Never changes for a node's lifetime, never reused.
/// See docs/04 §D.1 (the load-bearing data-model decision).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

/// Event identity (ULID). Time-ordered, unique across replicas.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub String);

/// Document identity (UUID, db-generated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentId(pub uuid::Uuid);

/// Identity (user) identity (UUID, db-generated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IdentityId(pub uuid::Uuid);

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
