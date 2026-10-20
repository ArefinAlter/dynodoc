//! The read-only MCP server of the Dynodoc AI-agent connector, Phase 0.
//!
//! This is the `engine-mcp` of `docs/AI-AGENT-CONNECTORS.md` sections 3.1 and 11.2, for the
//! read half of `docs/AI-AGENT-PROTOCOL-SPEC.md` (`dynodoc.agent/1`): it presents the tools
//! `files`, `outline`, `read`, `find`, `diff` and `status` to an MCP client and turns each
//! call into a request to the agent routes of `engine-api` (spec 12.1). The routes render
//! the answer (`engine-agent`); this crate adds nothing to it and decides nothing about
//! access.
//!
//! | Module | |
//! |---|---|
//! | [`rpc`] | JSON-RPC 2.0 lines: decode, encode, bounded reads |
//! | [`server`] | [`Server`]: `initialize`, `server/discover`, `ping`, `tools/list`, `tools/call` |
//! | [`tools`] | the tool definitions, [`Profile`], `instructions` |
//! | [`schema`] | argument validation from the published schemas |
//! | [`request`] | tool call to HTTP request, pure |
//! | [`backend`] | [`HttpBackend`]: the request, the answer, `401` renewal, limits |
//! | [`session`] | [`Session`] and [`Token`]: where bearer tokens come from |
//! | [`error`] | `dd/1 error` lines from API errors |
//!
//! # Where the binary lives, and why
//!
//! The library holds no key file code, no signing code and no database code: its
//! dependencies are `serde_json`, `tokio`, `tracing`, `uuid` and `reqwest`. A caller
//! supplies a [`Session`] and gets a [`Server`].
//!
//! The stdio entry point is the `engine-cli mcp` verb, not a second binary. Two options were
//! weighed:
//!
//! * **(a) A binary in this crate.** It would need the key file loader (`keyfile`,
//!   `passphrase`, `keystore`, `private_file`, `signing`: about 2,300 lines of `engine-cli`,
//!   with their platform code and tests) and the device-session client. Those would have to
//!   move into a shared library crate, changing the structure of a crate that is not part
//!   of this work and whose debug binary is close to the Windows main-thread stack limit.
//! * **(b) A verb in `engine-cli`.** The key and passphrase handling is reused unchanged.
//!   Moved code: none. Added to `engine-cli`: one module (`mcp.rs`, a [`Session`] over the
//!   device-session challenge protocol, and the wiring) and one enum variant. This is also
//!   the binary a person already needs for `device keygen` and `sync`, so one install
//!   serves the whole workflow.
//!
//! (b) won on the stated criteria: least existing code moved, and this library stays free of
//! key and database dependencies. Its cost is that the `mcp` process links what
//! `engine-cli` links (a larger binary; `sqlx` and `engine-api` are present in it but
//! unused, and the process holds no database credential and opens no database connection).
//! If a smaller dedicated binary is wanted later, only `engine-cli/src/mcp.rs` and the key
//! code it uses would move; this crate would not change.
//!
//! # Decisions and deviations from the specification
//!
//! Where the spec is silent, contradicts itself or its own examples, the example or the
//! stricter reading won. Each item is something a reader of the spec could not have
//! predicted.
//!
//! 1. **No `rmcp`.** Spec 13.1 recommends the official SDK. This crate hand-writes the five
//!    methods a read-only server needs instead (see [`server`]), adding no dependency beyond
//!    `serde_json` and crates the workspace already builds. `rmcp` is MIT or Apache-2.0 and
//!    maintained by the MCP project, so the choice is about size and surface, not licence;
//!    it was not built or measured here.
//! 2. **`status` is narrowed.** Spec 7.9 describes `what` as `proposal`, `proposals` or
//!    `access`. Phase 0 has no proposals, so the read-only profile offers `what=access` only
//!    and omits `proposal` and `file`.
//! 3. **`file` is not `format: uuid`.** Spec 7.3 to 7.6 type `file` as a UUID, but its
//!    transcripts (16.1) pass the 8-character prefix that `files` prints. The schema takes
//!    8 to 36 characters; the request builder admits a canonical UUID or 8 hex characters.
//! 4. **`cursor` on `find` and `diff`.** Their spec schemas have none, yet both can end with
//!    `more=<cursor>`; without the argument a truncated answer could not be continued.
//! 5. **`type` on enum properties, and a `title` on every tool** (the Anthropic directory
//!    requires one, spec 13.3). Added to the spec schemas; neither changes which calls are
//!    valid.
//! 6. **`find` needs `file`.** The routes search one file (spec 12.1 route 8). Spec 7.5 also
//!    allows a project; that is `bad_request field=file` until a route exists.
//! 7. **`instructions`** leaves out the spec's sentence about `propose`: no such tool is
//!    listed. It is 197 characters, under the limit of 500.
//! 8. **Wider protocol revisions.** `initialize` also agrees `2025-06-18` and `2025-03-26`,
//!    which the spec does not list: the tool surface is the same, and a client that asks for
//!    an older revision would otherwise be offered one it may not know.
//! 9. **ETag.** An `outline` `if_none_match` that looks like `"<rev>:<chain8>"` (the `etag`
//!    in `_meta["dynodoc/v1"]`) is sent as the `If-None-Match` header, and a `304` becomes
//!    the one-line `unchanged=1` result; any other value is the library's own `oh=` hash and
//!    goes in the query. `read` has no such argument (spec 5.2 lists "outline and read", but
//!    `read` has `have`).
//! 10. **Sequential service.** Requests are answered one at a time, in order, and
//!     `notifications/cancelled` is ignored. A client that issues parallel calls gets them
//!     one after the other.
//!
//! # Verification
//!
//! Unit tests in each module; `tests/server.rs` (protocol and stdout hygiene),
//! `tests/backend.rs` (an in-process axum server: success, `304`, `401` renewal, `403`,
//! `404`, `429`, truncation and cursors, oversized bodies, redirects, timeouts, token
//! secrecy) and `tests/contract.rs` (the schemas against the spec text, the request mapping
//! against the transcripts of spec 16, and the error lines against `engine-agent`).
//!
//! # Not verified
//!
//! No real MCP client, model or `engine-api` was run against this crate. The 2026-07-28
//! details are unverified (see [`server`]). Claude Desktop and Claude Code behaviour is
//! taken from docs/AI-AGENT-VENDOR-INTEGRATIONS.md, read on 14 October 2026.
#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]

pub mod backend;
pub mod error;
pub mod notes;
pub mod request;
pub mod rpc;
pub mod schema;
pub mod server;
pub mod session;
pub mod tools;

pub use backend::{
    parse_base, Backend, BackendConfig, CallContext, ConfigError, HttpBackend, ToolCall, ToolOutput,
};
pub use error::ToolError;
pub use server::Server;
pub use session::{Session, SessionError, Token};
pub use tools::{Profile, Tool};
