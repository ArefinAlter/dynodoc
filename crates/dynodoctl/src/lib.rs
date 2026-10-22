//! `dynodoctl`: the operator tool for the Dynodoc self-hosted single-node bundle (`infra/selfhosted`).
//!
//! It drives `docker compose` through a [`runner::Runner`], so every command is tested against a
//! scripted fake. Commands live in their own modules; `cli` parses arguments and `main` maps
//! results to the exit codes documented in `docs/SELF-HOSTED-OPERATIONS.md`.

pub mod backup;
pub mod compose;
pub mod configure;
pub mod ctx;
pub mod db;
pub mod envfile;
pub mod error;
pub mod generator;
pub mod install;
pub mod manifest;
pub mod net;
pub mod origin;
pub mod preflight;
pub mod redact;
pub mod rehearse;
pub mod report;
pub mod restore;
pub mod runner;
pub mod status;
pub mod tarball;
#[cfg(test)]
pub mod testkit;
pub mod util;
pub mod x509;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
