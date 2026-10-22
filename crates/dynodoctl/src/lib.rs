//! `dynodoctl`: the operator tool for the Dynodoc self-hosted single-node bundle (`infra/selfhosted`).
//!
//! It drives `docker compose` through a [`runner::Runner`], so every command is tested against a
//! scripted fake. Commands live in their own modules; `cli` parses arguments and `main` maps
//! results to the exit codes documented in `docs/SELF-HOSTED-OPERATIONS.md`.

pub mod compose;
pub mod ctx;
pub mod envfile;
pub mod error;
pub mod runner;
pub mod util;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
