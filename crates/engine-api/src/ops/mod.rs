//! The write path: HTTP endpoints ([`routes`]) over the shared apply funnel
//! ([`apply`]).
//!
//! Every content/governance write flows through `apply`, which runs the stage-16
//! governance gate, materializes the `node` table (so the event-log FK holds),
//! appends to the hash-chained log, snapshots, and fans the new event(s) out to SSE
//! subscribers. The endpoints just decode a request body into an `EventPayload` and
//! handle idempotency.

pub mod apply;
pub mod routes;

pub use routes::router;
