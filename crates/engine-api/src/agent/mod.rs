//! AI-agent connector: the data plane of `dynodoc.agent/1`, its read half (Phase 0) and its
//! review-gated write half (Phase 1) (docs/AI-AGENT-CONNECTORS.md sections 11.2 and 11.3,
//! docs/AI-AGENT-PROTOCOL-SPEC.md section 12).
//!
//! The pure read path and the operation compiler live in the `engine-agent` crate; this
//! module is the HTTP shell: the routes ([`routes`]), the bounded state cache ([`cache`]),
//! content negotiation and headers ([`http`]), the error envelope ([`error`]), the call
//! ledger ([`ledger`]) and the OpenAPI description ([`schema`]). The write half is
//! [`write`] (`propose`, and the routes) and [`proposal`] (`submit`, `withdraw`, status),
//! over the agent identity of [`identity`]; read [`write`] first: it states what an agent can
//! and cannot do and why nothing it does reaches the team version. The routes are mounted only when the operator
//! turns the gateway on (`AGENT_GATEWAY=on`); otherwise they do not exist and every
//! request, whatever its credential, gets the router's ordinary `404`.
//!
//! Beside them, always mounted, is the management plane ([`manage`]): the routes with
//! which an account session (never an agent or device token) reads and changes the
//! project and file policy ([`policy`]), pulls the kill switch and reads the usage ledger.
//! A project can be prepared before the gateway is switched on.
//!
//! What is not here: OAuth, API keys, the MCP write tools, sealed attribution (layer C), the
//! review screen's agent badge (the app), the evals, `diff proposal=` and any macro.
//!
//! # Decisions and deviations from the specification
//!
//! Where the spec says "proposed" the detail was settled here; where it is silent or
//! contradicts itself or the code, the stricter reading won. Each item is a choice a reader
//! of the spec could not have predicted.
//!
//! **Credentials and permission (spec 14)**
//!
//! 1. The agent scopes are `content:read`, `content:propose` and `content:submit` (migrations
//!    0042 and 0044): independent, none implies another. Writing also needs the project's
//!    `write_mode`, which is `none` until an owner changes it ([`write`]).
//! 2. An ordinary account login, a connector key or any non-device token is `401` on these
//!    routes (14.1 says "refused"; `401` matches the management routes' rule for a
//!    credential of the wrong class). A device token without the scope, for another
//!    project, or whose grant is gone is `403`. A project non-member is `403`, a file the
//!    caller cannot open is `404`; each layer has one answer for every reason.
//! 3. The key class is part of authority, read from `device_key_class` on every request. A
//!    person's key cannot hold, be granted or be issued `content:read`; an agent key can
//!    hold and be issued nothing else. A key with no class row is a person's key. A class
//!    is written only when enrolment names one and never changes.
//! 4. The `all` project-access mode never covers `content:read`: it always needs a grant
//!    for the project (`crate::auth::device::key_reaches`).
//! 5. The effective permission is the gateway flag, the grant, the person's current
//!    project and file role and the project and file policy of migration 0043 (spec 5.1 to
//!    5.3, 14.3): **default deny**. A project with no policy row, or `enabled = false`,
//!    serves no agent request; any member can still create an agent key and a grant, but
//!    both are inert until a project owner or manager enables agents. Every role, viewer
//!    included, may read (spec 14.3) unless the policy's `min_role` floor says otherwise.
//!    The policy is read on every request ([`policy::check`], [`policy::FILE_VISIBLE`]) and
//!    a disabled project, a role below the floor and a grant-less caller get the same
//!    `403`; a file with `agent_access = none` or of a kind the policy leaves out is the
//!    same `404` as a file that does not exist. The refused request is recorded in the
//!    ledger as `policy_denied` (it is internal; the caller cannot tell). Budgets are the
//!    `429` `budget_exceeded` of an already admitted caller.
//!
//! **Routes (spec 12)**
//!
//! 6. Route 13 (proposal `status`), routes 10 to 12 and their names are in [`write`]. `diff`
//!    still accepts `proposal=` and always answers `404`, after the file check; use
//!    `status` for a proposal.
//! 7. `GET .../diff` requires `from` (or a cursor carrying it); the spec leaves "neither"
//!    undefined. `to` defaults to the head.
//! 8. `{file}` is a document UUID or its first 8 hex characters when they name exactly one
//!    file the caller can open in the project (6.2). Trashed files, questionnaires and
//!    files of another project are not found.
//! 9. List-valued query parameters (`refs`, `ids`) are comma-separated. `max_bytes` is
//!    accepted wherever the library accepts it, although the 12.1 query lists omit it.
//!    Unknown query parameters and unknown body fields are `400`.
//! 10. `POST .../blocks:read` reads and writes nothing but the ledger, so it does not
//!     require `Idempotency-Key` (12.2 asks for one on every POST but `propose`), and it
//!     carries an `ETag` but never answers `304`. Its body is limited to 256 KiB.
//! 11. `Accept` selects the body: absent, `*/*`, `application/*` and `application/json` give
//!     the JSON mirror (12.2), `text/vnd.dynodoc.dct` (version 1 only) the DCT body. Errors
//!     are always the JSON envelope (12.4), even when DCT was asked for.
//! 12. ETag is `"<rev>:<chain8>"` of the revision served (for `diff`, the later one), on
//!     every file route and never on `files` or `access`. It validates one URL; the same
//!     `If-None-Match` on a different query is a different resource. A `304` is answered
//!     after the credential, grant, role and file checks, so a revoked caller never learns
//!     "unchanged", and before any state is read.
//! 13. A cursor pins its revision; when `rev` is also given they must agree. `outline` and
//!     `find` have no `rev` parameter: they read the head, or the revision of their cursor.
//! 14. `files` lists at most 10,000 files per project and refuses a larger project with
//!     `limit_exceeded field=files` rather than truncating.
//! 15. `access` describes this credential in this project: the grant now active, the
//!     person's role, the project's `write_mode` and `submit_requires_person`, the grant's
//!     budgets with what they have used (JSON only; the DCT header does not carry them; the
//!     proposal budgets appear when writing is on) and what this credential can do now
//!     (`calc=none`; `propose` and `submit` true only when its grant has the scope and the
//!     policy lets the agent). The discovery document advertises the server's capabilities
//!     (`propose` and `submit` true), `scopes` (the read scope, as in Phase 0) and
//!     `write_scopes`, and no MCP URL and no profiles because none exists.
//!
//! **State, verification and the ledger**
//!
//! 16. The cache holds at most [`cache::MAX_ENTRIES`] states, [`cache::MAX_BYTES`] of
//!     serialised state in total and [`cache::MAX_ENTRY_BYTES`] per state. A hit skips the
//!     replay, not the authorisation; the key includes the stored chain hash of the
//!     revision, read on every request. A cached state was verified when it was loaded; it
//!     is not replayed again.
//! 17. Verification is that of `GET /documents/{id}/state`: the nearest verified checkpoint
//!     plus a hash-checked tail. A document that does not replay is `replay_failed` (HTTP
//!     500, `details.agent_code=internal`). Events inside the checkpoint are covered by
//!     the checkpoint's own verification, not re-hashed per request.
//! 18. Every admitted read is a ledger row, not a sample, with outcome `ok` or the
//!     `agent_code` of the failure (`policy_denied` or `budget_exceeded` when the project
//!     policy stopped it; those two are written at most once a minute per key, project
//!     and outcome, because a refusal can be repeated at the admission rate and a ledger
//!     row cannot be deleted). Requests refused before admission (no membership,
//!     admission budget, bad credential) write nothing. A ledger failure is logged and does
//!     not fail the read. The runtime login cannot delete; pruning by retention is not
//!     built.
//! 19. Admission is the existing process-local [`crate::workloads`] limits (2 active and
//!     120 a minute per identity, 4 active and 600 a minute per project, 8 expensive reads
//!     in the process), answered `429` with `Retry-After`. No other rate-limit header is
//!     defined by the spec or sent.
//! 20. The global permissive CORS layer of [`crate::app`] also covers these routes. The
//!     credential is a bearer token, never a cookie; spec C11 asks for no CORS on the
//!     agent routes once a remote MCP binding exists (Phase 2).

pub mod cache;
pub mod error;
pub mod http;
pub mod identity;
pub mod ledger;
pub mod manage;
pub mod policy;
pub mod proposal;
pub mod routes;
pub mod schema;
pub mod state;
pub mod write;

pub use cache::StateCache;
pub use manage::{router as manage_router, AgentManageApi};
pub use routes::router;
pub use schema::AgentApi;
