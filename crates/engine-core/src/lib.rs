//! dynodoc engine core.
//!
//! The irreducible foundation: append-only hash-chained event log (stage 14),
//! snapshotted fold / materialization (stage 15), semantic ops + governance
//! (stage 16), referential integrity + semantic merge (stage 21).
//!
//! `log`, `materializer`/`snapshot`, `ops`, `governance`, and the referential-
//! integrity pass are implemented (stages 14–16); the semantic three-way merge that
//! also builds on `integrity` lands in stage 21.

/// Append-only, hash-chained event log (the system of record). See docs/14.
pub mod log;

/// Snapshotted fold / state materialization. See docs/15.
pub mod materializer;

/// Snapshot storage, Merkle commitments, and snapshot-backed current-state reads.
pub mod snapshot;

/// Semantic operation handlers: per-variant validation against current state. See docs/16.
pub mod ops;

/// Capability matrix, governance gate, and proposal lifecycle. See docs/16.
pub mod governance;

/// Referential integrity pass (deploy gate, stage 16) + semantic three-way merge
/// (stage 21). See docs/16 part 4 and docs/21.
pub mod integrity;
