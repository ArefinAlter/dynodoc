//! The MCP server of the Dynodoc AI-agent connector: read-only by default (Phase 0), with
//! `propose` and `submit` when the operator asks for them (Phase 1).
//!
//! This is the `engine-mcp` of `docs/AI-AGENT-CONNECTORS.md` sections 3.1, 11.2 and 11.3, for
//! `docs/AI-AGENT-PROTOCOL-SPEC.md` (`dynodoc.agent/1`): it presents the tools `files`,
//! `outline`, `read`, `find`, `diff` and `status` to an MCP client (the [`Profile::read_only`]
//! profile) and, in the [`Profile::full`] profile, `propose` and `submit` and a `status` that
//! also reports a proposal, and turns each call into a request to the agent routes of
//! `engine-api` (spec 12.1). The routes render the answer (`engine-agent`); this crate adds
//! nothing to it and decides nothing about access. The one thing it adds is a line after the
//! answer of a write tool that says the change is a proposal for a person to review: an agent
//! only ever proposes, and nothing it does is live until a person merges it ([`notes`]).
//!
//! | Module | |
//! |---|---|
//! | [`rpc`] | JSON-RPC 2.0 lines: decode, encode, bounded reads |
//! | [`server`] | [`Server`]: `initialize`, `server/discover`, `ping`, `tools/list`, `tools/call` |
//! | [`tools`] | the tool definitions, [`Profile`], `instructions` |
//! | [`schema`] | argument validation from the published schemas |
//! | [`request`] | tool call to HTTP request, pure |
//! | [`backend`] | [`HttpBackend`]: the request, the answer, `401` renewal, resend of a write, limits |
//! | [`notes`] | the line that says what a write tool's answer means |
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
//!    `access`. The read-only profile has no proposals, so it offers `what=access` only and
//!    omits `proposal` and `file`. The full profile adds `proposal` (a whole id) and
//!    `what` of `proposal` or `access`; `what=proposals` and `file` stay out because no
//!    route lists a file's proposals. With `what` left out, a given `proposal` is asked for,
//!    else the connection's access (so `status` with no arguments answers in both profiles).
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
//! 7. **`instructions`** of the read-only profile leave out the spec's sentence about
//!    `propose`: no such tool is listed. They are 197 characters, under the limit of 500. The
//!    full profile's say that `propose` never edits the team version, that a person must
//!    review and merge, to use `dry_run` first and to tell the user the proposal id.
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
//! The write tools (`propose`, `submit`; Phase 1) add these:
//!
//! 11. **`request_id` is optional.** Spec 7.7 requires it ("New UUID per call"). The backend
//!     adds a random UUID when the model gave none, once per logical call: a renewal of the
//!     session and the one resend of a lost request send the same body, and the next call gets
//!     another id. An id the model passes is used as given, which is how it retries a call
//!     whose answer it lost. When the outcome of a `propose` stays unknown (a timeout, a
//!     broken connection), the error line carries `request_id=<id>` so that the retry can
//!     reuse it. The service stores a receipt under it and answers a repeat with `replayed=1`.
//! 12. **Whole proposal ids.** `proposal` is `format: uuid` in `propose`, `submit` and `status`.
//!     The spec's examples print eight characters; the service chooses the proposal id itself,
//!     prints the whole one in every header, and its routes take no prefix.
//! 13. **Ops the service does not compile are not offered.** `fill`, `add_slide`,
//!     `delete_slide` and `macro` leave the `op` enum, and `from`, `to`, `dir`, `into` and `name`
//!     leave the op object (the service answers `unsupported_op` or `bad_request` for them;
//!     `fmt=md` is refused too, so `fmt` was never added). A call using them fails here, naming
//!     the field and the op, before a request is made.
//! 14. **The service's limits are in the schema.** `text`, `with` and the `text` of a block at
//!     most 100,000 characters, `find` at most 2,000 (the spec sets none; the service does),
//!     `blocks` at most 50, `values` at most 2,000 rows of at most 100 cells, `ops` 1 to 200.
//!     Without them the validator's silent cap of 4,096 characters would refuse a long
//!     paragraph. More than 200 ops, or a body over 1 MiB, never leaves the process.
//! 15. **`submit` is one tool, two routes** (`submit` or `withdraw` by `action`); `note` goes
//!     in the body. `propose` posts to `.../files/{file}/propose` (the service's name; the
//!     spec says `.../proposals`) without `file` in the body, which the path carries.
//! 16. **One resend.** A write (`propose`, `submit`) whose request failed in transit, whether
//!     it reached the service or not, is sent once more, unchanged, after a quarter of a
//!     second. That is safe because a `propose` repeats its `request_id` and a `submit` is
//!     idempotent by state. A read is never resent, an answer from the service never is, and
//!     neither is a `429` (the error carries `retry_after`).
//! 17. **A note after the answer.** The answer of `propose`, `submit` and a proposal's
//!     `status` ends with one line `note "..."` chosen from the header's `state` word:
//!     `proposal only ...; nothing is live`, `waiting for a person to submit; nothing is
//!     live`, and so on, plus the stale refs to read again. The service's own text comes first
//!     and unchanged ([`notes`]).
//! 18. **Error fields.** `op` and `id` (the failing op of a `propose`, as the service's library
//!     writes them after the other fields) and this crate's `request_id` join the typed fields
//!     of an error line; values stay restricted tokens or numbers.
//! 19. **Write scopes.** A [`Token`] says whether it carries the write scopes. When it does
//!     not (the key was not granted them), a write tool answers `forbidden policy=write_scope`
//!     without a request. The service decides in any case.
//! 20. **The validator goes deeper.** `ops` items are objects in an array, so the validator now
//!     checks `properties`, `required` and `additionalProperties: false` at any depth and
//!     `minItems`, and a failing item is named as `field=<name> op=<index>`.
//!
//! # Verification
//!
//! Unit tests in each module; `tests/server.rs` (protocol and stdout hygiene, both profiles,
//! hidden tools), `tests/backend.rs` (an in-process axum server: success, `304`, `401`
//! renewal, `403`, `404`, `429`, truncation and cursors, oversized bodies, redirects, timeouts,
//! token secrecy; for the write tools the request body and headers, request ids across
//! renewals and a dropped connection, dry runs, stale bases, every refusal, submit and
//! `awaiting_person`, status, missing write scopes and secrecy) and `tests/contract.rs` and
//! `tests/contract_write.rs` (the schemas against the spec text, the pinned size of both
//! profiles, the request mapping against the transcripts of spec 16, the default profile
//! against a golden copy, and the error lines and notes against `engine-agent`).
//!
//! # Not verified
//!
//! No real MCP client, model or `engine-api` was run against this crate. The 2026-07-28
//! details are unverified (see [`server`]). Claude Desktop and Claude Code behaviour is
//! taken from docs/AI-AGENT-VENDOR-INTEGRATIONS.md, read on 14 October 2026. Whether a model
//! uses the write tools well, and whether it keeps to the note's wording, is not measured.
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
