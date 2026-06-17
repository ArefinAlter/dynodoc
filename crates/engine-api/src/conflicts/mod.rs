//! Four-card conflict resolution (docs/21 item 4, PoC instantiation).
//!
//! The PoC has no CRDT branch source (stage 20 skipped). A *conflict* is instead two or
//! more **concurrent pending suggestions** that edit the same `(node_id, field)` to
//! *different* values — the PoC instantiation of the merge engine's `BothDifferent`
//! classification, generalized to N proposals. Conflicts are derived from current state
//! ([`derive`]), surfaced over HTTP ([`routes`]), and resolved by the author accepting a
//! proposal or entering a custom value ([`apply`]).

pub mod apply;
pub mod derive;
pub mod routes;

pub use routes::router;
