//! dynodoc engine core.
//!
//! The irreducible foundation: append-only hash-chained event log (stage 14),
//! snapshotted fold / materialization (stage 15), semantic ops + governance
//! (stage 16), referential integrity + semantic merge (stage 21).
//!
//! Stage 12 scaffolding only — modules are placeholders so the workspace builds.

/// Append-only, hash-chained event log (the system of record). See docs/14.
pub mod log {}

/// Snapshotted fold / state materialization. See docs/15.
pub mod materializer {}

/// Semantic operation handlers + governance gate. See docs/16.
pub mod ops {}

/// Referential integrity pass + semantic three-way merge. See docs/21.
pub mod integrity {}
