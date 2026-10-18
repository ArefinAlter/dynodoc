//! AI-agent connector, Phase 0: the read-only data plane of `dynodoc.agent/1`
//! (docs/AI-AGENT-CONNECTORS.md section 11.2, docs/AI-AGENT-PROTOCOL-SPEC.md section 12).
//!
//! The pure read path lives in the `engine-agent` crate; this module is the HTTP shell:
//! the routes ([`routes`]), the bounded state cache ([`cache`]), content negotiation and
//! headers ([`http`]), the error envelope ([`error`]), the call ledger ([`ledger`]) and
//! the OpenAPI description ([`schema`]). The routes are mounted only when the operator
//! turns the gateway on (`AGENT_GATEWAY=on`); otherwise they do not exist and every
//! request, whatever its credential, gets the router's ordinary `404`.
//!
//! What is not here: `propose`, `submit`, proposal `status`, project and file policy,
//! budgets, OAuth, API keys and the MCP server.

pub mod cache;
pub mod error;
pub mod http;
pub mod ledger;
pub mod routes;
pub mod schema;
pub mod state;

pub use cache::StateCache;
pub use routes::router;
pub use schema::AgentApi;
