//! The `/documents` REST surface: reads (list/get/node/events) and create.
//!
//! Reads serve materialized current state via the snapshot engine; the write path
//! lives in [`crate::ops`]. Public reads take no token; events + create require auth.

pub mod routes;
pub mod store;

pub use routes::router;
