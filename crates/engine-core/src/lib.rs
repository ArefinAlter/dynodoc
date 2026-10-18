//! dynodoc engine core.
//!
//! The irreducible foundation: append-only hash-chained event log (stage 14),
//! snapshotted fold / materialization (stage 15), semantic ops + governance
//! (stage 16), referential integrity + semantic merge (stage 21).
//!
//! `log`, `materializer`/`snapshot`, `ops`, `governance`, the referential-integrity
//! pass, the ODK expression parser, and the semantic three-way merge are implemented
//! (stages 14–16 and 21).

pub mod database;
pub mod expr;
pub mod governance;
pub mod integrity;
pub mod log;
pub mod materializer;
pub mod merge_policy;
pub mod ops;
pub mod projection;
pub mod repository;
pub mod richtext;
pub mod shared_checkpoint;
pub mod similarity;
pub mod snapshot;
pub mod snapshot_verification;
pub mod transparency;
pub mod workspace_merge;
