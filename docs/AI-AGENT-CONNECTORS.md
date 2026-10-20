# AI agent connectors: master design

> **Status: design, not implemented.** 14 October 2026. This document specifies how AI
> agents (Claude first, then ChatGPT, then others) connect to Dynodoc to read documents,
> spreadsheets and presentations and to propose edits that people review. Nothing here
> is built. It changes no code, migration or deployed behaviour. Numbers for effort and
> token savings are estimates or design targets and say so.
>
> Companion documents:
> [protocol specification](AI-AGENT-PROTOCOL-SPEC.md) (tool contract, grammar, errors),
> [token efficiency and benchmarks](AI-AGENT-TOKEN-EFFICIENCY.md),
> [security and privacy](AI-AGENT-SECURITY.md),
> [vendor integrations](AI-AGENT-VENDOR-INTEGRATIONS.md).
> Prior proposal this supersedes where they differ:
> [agent provenance proposal](proposals/AGENT-PROVENANCE-PROPOSAL.md).

## 0. Owner decisions this design follows

Recorded 14 October 2026 (from the owner's brief):

1. Target customers are teams of any size, in enterprise and academia, working on many
   documents.
2. `hyperformula` (GPL) was replaced on 14 October 2026 by an in-house, dependency-free formula
   engine that runs in the browser. Server-side spreadsheet features that need formula
   evaluation or rewriting still need a server-side port of it.
3. Agent connectors: **Claude first**, then ChatGPT (apps, connectors, actions), then
   the others.
4. Agent edits flow through the existing review model (drafts and change requests, never
   silent writes to the team version), are attributed as agents and are hash-chained.
5. Minimising the token cost of programmatic edits is a primary goal.
6. The application stays private and proprietary; the engine, API and CLI stay MIT.
   Copyright holder: Verne Ltd.
7. This and two other ideas (self-hosted edition, end-to-end encryption) are documented
   now, not built.

Standing constraints from the repository: events are append-only; migrations are
immutable once applied; node ids are never reassigned; production is schema 40 with
shared writers off and no receipt-signing authority installed
([PROJECT-STATUS.md](PROJECT-STATUS.md),
[RECEIPT-AUTHORITY-DECISION.md](RECEIPT-AUTHORITY-DECISION.md)).

## 1. Purpose, goals, non-goals, personas, metrics

### 1.1 Purpose

People already ask AI assistants to read and revise their documents. Today the choices
are poor: paste text into a chat (no identity, no review, no provenance), or give an
assistant an account token (every edit looks like the person, nothing is gated, and the
current REST surface returns a whole file per call). Dynodoc's differentiator is that
edits are typed, attributed, hash-chained and reviewable. This design lets an assistant
use that machinery without weakening it, at a token cost proportional to the task.

### 1.2 Goals

| # | Goal |
|---|---|
| G1 | A user connects an AI client to a Dynodoc project in minutes and chooses exactly which projects and scopes it gets. |
| G2 | Every agent write is a proposal: it lands in a private draft owned by an agent identity, becomes a change request only when submitted, and reaches the team version only when a person with merge rights merges it under the project's rules. |
| G3 | Agent authorship is visible in the UI and tamper-evident in the event chain. |
| G4 | The fixed and per-call token cost is a small multiple of the information needed, with a measured benchmark behind every claim. |
| G5 | One server contract serves Claude, ChatGPT and other MCP clients and plain HTTP clients. |
| G6 | An administrator can allow, restrict, budget, audit and switch off agent access at project and instance level, and the switch takes effect on the next request. |
| G7 | The design works on an intranet or self-hosted installation, with documented limits for cloud-hosted clients. |

### 1.3 Non-goals

- No agent merges, approves, changes membership, edits policy or erases content.
- No model inside Dynodoc. Dynodoc does not run, host, fine-tune or evaluate models.
- No transcript or prompt capture: the server cannot see the conversation.
- No claim that agent output is correct, or that the host client behaves safely.
- No in-editor assistant and no "ask the history" summarisation (these need the scope
  decision called out in the earlier proposal).
- No marketplace listing of the Office and Google add-ins; those are separate.
- No end-to-end encryption interplay in v1; see section 9.4.

### 1.4 Personas and use cases

| Persona | Scenario | Needs |
|---|---|---|
| Researcher in Claude Desktop | "Harmonise the terminology in this 60-page protocol, show me what changed." | Cheap outline and search, a bulk replace, one review screen. |
| Programme manager | "Update the budget sheet with these Q3 actuals." | Range reads, cell writes, formula safety, diff of numbers. |
| Policy team lead | Lets team members use any assistant but keeps merge rights and a record of who proposed what. | Per-project policy, agent badge, audit export. |
| Compliance officer | Needs to know which provider saw which file and prove reviewed edits. | Egress policy, call log, attribution in the chain. |
| Platform admin | One-click disable, per-client allow-list, budgets. | Kill switch, policy, usage dashboard. |
| Integrator | A script or custom assistant calls Dynodoc. | OpenAPI, API key, stable errors, idempotency. |

### 1.5 Success metrics (targets to validate, not claims)

| Metric | Target | How measured |
|---|---|---|
| Silent writes to the team version by an agent credential | 0 | Gate test in CI and production counters. |
| Median bytes returned for orientation in a 200-block document | at most 1 KB at depth 0 | Benchmark harness ([token efficiency](AI-AGENT-TOKEN-EFFICIENCY.md)). |
| Task token cost versus a thin text wrapper on the benchmark suite | measured reduction with confidence interval; no public claim before the run | Harness, pre-registered analysis. |
| Proposals merged without reviewer edits | tracked, no target before pilot | Review data. |
| Time from "install" to first proposal for a Claude Desktop user | under 10 minutes | Usability test with five users. |
| Revocation latency | next request | Integration tests. |
| p95 latency of `outline` and `read` on a 10,000-node file, warm cache | under 300 ms (design target) | Load test; unproven today. |

## 2. Current state versus target

| Capability | Tag | Evidence or gap |
|---|---|---|
| Append-only hash-chained events; `content_hash` frames actor | EXISTS | `migrations/0002_events.sql`; `crates/engine-core/src/log.rs`. |
| Stable node ids; compact rich-text patches with base digests | EXISTS | `richtext.rs`, `engine-shared/src/richtext.rs`. |
| Personal drafts, change requests, approvals, merge with policy floors | EXISTS | `workspace.rs`, `reviews.rs`, `access.rs`; [PROJECT-POLICY-AND-METADATA.md](PROJECT-POLICY-AND-METADATA.md). |
| Project-scoped machine credentials with account-controlled per-project grants | EXISTS | [PROJECT-SCOPED-DEVICE-SESSIONS.md](PROJECT-SCOPED-DEVICE-SESSIONS.md), [DEVICE-KEY-GRANTS-API.md](DEVICE-KEY-GRANTS-API.md); migrations 0038, 0040. |
| File-scoped editor credentials that can only propose | EXISTS | `connector_grant` (migration 0023), `provenance.rs`. |
| Immutable operation audit | EXISTS | `operation_audit` (migration 0015), `product::audit`. |
| Idempotent write patterns | EXISTS | `idempotency.rs`; `request_id` in `history.rs`; bundle receipts in `provenance.rs`. |
| Local signing key, in-memory renewal, restricted file permissions | EXISTS | `crates/engine-cli/src/{device,signing,private_file}.rs`. |
| Process-local admission and rate budgets | EXISTS | `crates/engine-api/src/workloads.rs`. |
| Agent identity class and delegation | MISSING | `identity` has no kind; no principal link. |
| Agent scopes on credentials; agent policy; DLP | MISSING | Scopes are `project:read`, `history:*`. |
| OAuth 2.1 authorisation server and resource metadata | MISSING | Magic link plus trusted web server (`crates/engine-api/src/auth/`). |
| Compact outline, range, profile, find, diff endpoints | MISSING | See [protocol spec section 3](AI-AGENT-PROTOCOL-SPEC.md). |
| Op compiler (intent ops to engine ops) | MISSING | Parts exist (`make_patch`, `diff`); `between()` only in TypeScript. |
| MCP server (stdio and Streamable HTTP) | MISSING | None. |
| Server-side formula evaluation and reference rebasing | MISSING | Browser-only in-house engine (`src/lib/workspace/formula/`); no server-side port. |
| Agent attribution in the event chain | MISSING | Merge appends events as the merger (`append_to_team`). |
| Review UI that identifies agent authorship | MISSING | `REQUEST_JSON` exposes author id, name, email only. |
| Vendor listings (Anthropic directory, ChatGPT app directory) | MISSING | Legal pages are drafts "not yet effective" ([LEGAL-PAGES.md](LEGAL-PAGES.md)), which directories require. |

## 3. Architecture

### 3.1 Components

```
  User machine                               Vendor cloud                 Dynodoc deployment
 +---------------------------+            +-----------------------+     +-----------------------------------+
 | Claude Desktop / Code     |            | claude.ai connectors  |     | nginx (TLS, headers, limits)      |
 | Cursor, Codex, Gemini CLI |            | ChatGPT (plugins)     |     |   /mcp      -> engine-mcp (HTTP)  |
 |   |  stdio                |            | Copilot Studio        |     |   /oauth/*  -> engine-api         |
 |   v                       |            +----------+------------+     |   /api/...  -> engine-api         |
 | engine-mcp (local)        |                       | HTTPS + OAuth    |   app host  -> SvelteKit (consent, |
 |   device key (Ed25519)    |                       v                  |               grants, policy, UI) |
 +-------------+-------------+            +-----------------------+     +--------+--------------------------+
               | HTTPS, device-session    | engine-mcp (remote)   |              |
               | token (1 h, in memory)   +----------+------------+              v
               |                                     | agent token         +-----------+
               +-------------------------------------+-------------------> | engine-api|---> PostgreSQL
                                                                           | (policy,  |     (restricted
                                                                           |  gates)   |      runtime role)
                                                                           +-----------+
```

| Component | Responsibility | Licence and repo |
|---|---|---|
| `engine-agent` (new crate) | Outline, read, find, diff, op compiler, DCT codec, ref and hash functions, profile statistics. Pure functions over `engine-core` types where possible. | MIT, engine repo |
| `engine-api` additions | Agent routes, `AgentAuth` extractor, agent tokens, OAuth authorisation server, grants, policy, budgets, call ledger. The only authorisation point. | MIT |
| `engine-mcp` (new binary crate) | MCP protocol, transport, tool definitions, profiles. Holds no database credentials; forwards the caller's token. | MIT |
| App additions (SvelteKit) | OAuth consent screen, grants and keys UI, project agent policy tab, proposal inbox, agent badges, kill switch, usage view. | Proprietary, private repo |
| Packaging | Claude Code plugin (skills plus MCP config), optional Desktop bundle, OpenAPI document, vendor listings. | Mixed |

The MCP server being a separate process with no database credentials is deliberate: it
cannot become a second authorisation point or a path around the runtime role
([DATABASE-PRIVILEGES.md](DATABASE-PRIVILEGES.md)).

### 3.2 Trust boundaries

| Boundary | Between | What crosses | Control |
|---|---|---|---|
| TB1 | AI client and model provider, and Dynodoc | Tool arguments, tool results (document content) | Grant scope, policy, DLP, budgets; Dynodoc cannot control the provider. |
| TB2 | `engine-mcp` and `engine-api` | The caller's token; DCT or JSON | Token audience check; no token passthrough; API re-authorises every call. |
| TB3 | `engine-api` and PostgreSQL | SQL under the restricted runtime login | Existing privilege boundary. |
| TB4 | Browser consent and authorisation server | OAuth authorisation request, user decision | Exact redirect match, PKCE, state, consent text from server-held client metadata. |
| TB5 | Document content and the model | Untrusted text | DCT escaping, no write to team version, review. |

### 3.3 Data flows

**Read.** Client calls `outline` → `engine-mcp` → `GET …/agent/files/{file}/outline`
→ `engine-api` authorises (token, grant, role, policy, DLP) → state cache or checked
checkpoint reader → DCT → client. No write; one `agent_call` row (sampled metadata).

**Propose.** Client calls `propose` → compile ops (validated against the draft's state
with `governance::authorize`) → one transaction: idempotency lookup, `workspace_draft`
create or lock, `draft_edit` rows with the agent identity, receipt, audit → diff
summary back. Nothing is appended to `event`.

**Review and merge (unchanged for people).** The principal or reviewers open the change
request in the web workspace; approvals and merge follow the file's effective policy.
At merge, attribution is recorded (section 6).

**Kill switch.** Principal revokes grants, or a project manager disables agents, or the
operator turns the gateway off. Every request re-reads grants and policy, so the next
call fails; in-flight transactions complete or roll back atomically.

### 3.4 Where the existing mechanisms plug in

| Existing mechanism | Reuse |
|---|---|
| `workspace_draft`, `draft_edit` | Proposal storage; `source.kind = "agent"` (8 KiB cap). |
| `reviews::submit`, `merge`, review rules | Unchanged; agent identity never approves. |
| `workspace_merge::diff` and `merge` | Diff output and conflict detection. |
| `richtext::make_patch`, `compact_event` | Text edit compilation. |
| `governance::authorize` incl. sheet protection | Per-op validation. |
| Device sessions and `key_reaches` | Local credential path; grant model template for OAuth grants. |
| `connector_grant` pairing | Template for API-key style credentials (hash only, bearer, revocable). |
| `operation_audit` | Grant, policy, session and proposal audit rows. |
| `Workloads` | Admission control; extended with per-grant budgets. |
| Idempotency ledger and `request_id` | Retried writes. |

## 4. Identity, credentials and grants

### 4.1 Agent identity (proposed)

An agent identity is an `identity` row (so `event.actor_id` and `draft_edit.actor_id`
foreign keys hold) with a sidecar:

```sql
-- proposed migration 0041_agent_identity.sql (tentative number)
create table agent_identity (
  identity_id  uuid primary key references identity(id),
  principal_id uuid not null references identity(id),
  client_kind  text not null check (client_kind in
               ('claude','chatgpt','gemini','copilot','cursor','codex','device','generic')),
  client_ref   text not null,        -- OAuth client_id or device key id; a claim, never authority
  display_name text not null,        -- e.g. "Claude for Olivia"
  created_at   timestamptz not null default now(),
  unique (principal_id, client_kind, client_ref)
);
-- identity.email stays NOT NULL: agent rows use agent+<uuid>@agents.invalid (reserved TLD)
-- and sign-in/mail code must refuse addresses in that domain.

create table device_key_class (      -- one immutable row per enrolled key, beside device_key_access
  key_id     uuid primary key references device_signing_key(id),
  class      text not null check (class in ('person','agent')),
  label      text,                   -- e.g. "Claude Desktop on laptop"; no secrets
  created_at timestamptz not null default now()
);
```

Rules:

- One agent identity per (principal, client product). The person's name is part of the
  display name, so a reviewer sees who delegated.
- It is not a project member and holds no `document_access` row. Its authority is the
  principal's current role intersected with grants and policy (protocol spec 14.3).
  This avoids a second population of members that could be promoted by mistake.
- It is excluded from approvals by construction and by an explicit check (I4).
- On principal disablement or erasure the agent identity and its credentials are
  revoked in the same trigger that already revokes device keys
  (`revoke_removed_account_devices`, migration 0033).

### 4.2 Three ways to obtain a credential

| Path | Phase | For | Mechanism |
|---|---|---|---|
| A. Local agent key | 0 to 1 | stdio MCP on the user's machine | Enrol an Ed25519 key with class `agent` (`device_key_class`). Grants per project and scope exactly as device keys. Session tokens are issued by the existing challenge protocol and kept in memory (1 hour). |
| B. OAuth authorisation | 2 | claude.ai, ChatGPT, Copilot Studio, any remote MCP client | The principal consents in the browser; the client receives a short access token and a rotating refresh token. Grants are rows per (authorisation, project, scopes). |
| C. Agent API key | 2 | Scripts, HTTP-only integrators | Opaque 256-bit key shown once, hash stored, project-scoped, expiry at most 90 days, revocable. Modelled on `connector_grant`. |

Path A needs no OAuth server and ships first. It is also the only path available for a
fully private install that cloud clients cannot reach.

### 4.3 Path A in detail

1. Person runs `engine-cli device keygen --out agent.der` (existing) and enrols it in
   Profile → Signing keys with the new choice "Use for an AI agent" (proposed UI). The
   enrolment records `device_key_class = 'agent'` and a label.
2. The key is `granted` mode by default (schema 40); the person adds the project and
   picks scopes `content:read`, optionally `content:propose`, `content:submit`.
3. `engine-mcp --device-key agent.der --remote remote.json --project <uuid>` obtains a
   session with the existing challenge flow (`device_sessions.rs`); the scopes widen the
   `device_session.scopes` check (migration 0040 pattern).
4. The token is accepted only by `AgentAuth` extractors on agent routes. A person's
   device token for history sync does not reach them unless it has the new scopes.

Limits inherited, stated honestly: the key file is unencrypted; a stolen token works for
up to an hour; no per-grant expiry
([PROJECT-SCOPED-DEVICE-SESSIONS.md](PROJECT-SCOPED-DEVICE-SESSIONS.md), "Security
caveats and not done").

### 4.4 Path B in detail (OAuth 2.1)

Roles: `engine-mcp` is the resource server; `engine-api` hosts the authorisation server
(or an embedded one); the web app renders consent.

| Endpoint | Purpose |
|---|---|
| `GET /.well-known/oauth-protected-resource` (on the MCP host) | RFC 9728 metadata: `resource`, `authorization_servers`, `scopes_supported`. |
| `GET /.well-known/oauth-authorization-server` | RFC 8414 metadata including `client_id_metadata_document_supported: true`, `code_challenge_methods_supported: ["S256"]`, `token_endpoint_auth_methods_supported: ["none"]`, `authorization_response_iss_parameter_supported: true`. |
| `GET /oauth/authorize` | Validates client and redirect, then redirects to the app consent page. |
| `POST /oauth/token` | `authorization_code` and `refresh_token` grants; `application/x-www-form-urlencoded`. |
| `POST /oauth/revoke` | RFC 7009 revocation. |
| `POST /oauth/register` | Dynamic client registration (fallback; deprecated in the current MCP spec, still required by some clients). |

Client identification, in the order vendors use: pre-registered clients (admin approved),
Client ID Metadata Documents (CIMD, preferred by the current MCP spec; Claude uses
Anthropic's published identity when the server advertises CIMD, and ChatGPT prioritises
CIMD), then DCR. See [vendor integrations](AI-AGENT-VENDOR-INTEGRATIONS.md) for what
each client requires.

Token design:

- Access token: Paseto v4.local, 15 minutes (proposed; Claude refreshes proactively up
  to five minutes before expiry), claims `typ`, `principal`, `agent`, `client`, `grant`
  (authorisation id), `aud` = canonical resource URI, `jti`, `iat`, `exp`. Distinct
  implicit assertion `dynodoc.agent-session.v1/implicit`, so account and device
  validators fail at the authentication tag, as for device sessions. Project reach is
  **not** in the token: it is read from grant rows on every request.
- Refresh token: opaque, hash stored, rotates on use, reuse of a retired token revokes
  the lineage, absolute lifetime 90 days (proposed), sliding 14 days.
- Audience binding (RFC 8707 `resource`): required and checked; tokens are never
  forwarded to another service (token passthrough is forbidden by the MCP spec).
- Scopes: `content:read`, `content:propose`, `content:submit`. `offline_access` is
  accepted so refresh tokens can be issued (some clients lose access without it).

Consent screen (app): shows client name from the metadata document with its origin,
requested scopes in plain language, a project picker limited to projects where the
person is a member, an explicit "can propose changes; cannot merge" statement, the
policy that applies per project (including whether writes are allowed), and which AI
provider will receive document text ("content you open will be sent to the assistant
you are using; Dynodoc cannot control how that provider handles it").

### 4.5 Grants

```sql
-- proposed (0041/0042)
create table agent_authorization (            -- the person's consent for one client
  id uuid primary key default gen_random_uuid(),
  principal_id uuid not null references identity(id),
  agent_identity uuid not null references agent_identity(identity_id),
  client_id text not null, client_name text not null,
  scopes text[] not null check (scopes <@ array['content:read','content:propose','content:submit']),
  created_at timestamptz not null default now(), revoked_at timestamptz
);
create table agent_project_grant (            -- revoke-only, like device_key_project_grant
  id uuid primary key default gen_random_uuid(),
  authorization_id uuid references agent_authorization(id),
  key_id uuid references device_signing_key(id),            -- exactly one of the two
  project_id uuid not null references project_settings(space_id),
  scopes text[] not null,
  created_at timestamptz not null default now(), created_by uuid not null references identity(id),
  revoked_at timestamptz,
  check ((authorization_id is null) <> (key_id is null))
);
create unique index on agent_project_grant(coalesce(authorization_id,key_id), project_id)
  where revoked_at is null;
```

Same invariants as schema 40: rows are never edited except one `revoked_at` transition;
at most 50 active grants per credential; changes take the account row lock so an
in-flight request either finishes first or sees the change; every change writes
`operation_audit`.

Further tables (sketch, Phase 2): `agent_oauth_client(client_id, name, redirect_uris,
trust, first_seen, approved_by)` for CIMD and DCR clients;
`agent_refresh_token(id, authorization_id, hash, parent, used_at, expires_at)` for
rotation and reuse detection; `agent_api_key(id, project_id, hash, scopes, expires_at,
last_used_at, revoked_at)`; `project_agent_policy`, `file_agent_policy` (section 5);
`agent_call` (protocol spec 9.6). All follow the existing conventions: immutable or
revoke-only rows, runtime role without DELETE, audit rows for state changes.


## 5. Policy model

### 5.1 Layers and precedence

A request is allowed only if every layer allows it. Lower layers may narrow, never
widen, the layer above (the rule project floors already follow in
[PROJECT-POLICY-AND-METADATA.md](PROJECT-POLICY-AND-METADATA.md)).

| Order | Layer | Owner | Examples |
|---|---|---|---|
| 1 | Instance (operator) | Verne or the self-hosting operator | Gateway on or off; client block list; global budgets; DLP provider. Environment and admin console. |
| 2 | Organisation or workspace | Organisation admin | Allowed client kinds; mandatory DLP; maximum `write_mode`; egress attestation requirement. **Needs a tenant object that does not exist today** (open question 3). |
| 3 | Project | Project owner or manager | `enabled`, `write_mode`, kinds, clients, scopes ceiling, budgets, submit rule, retention. |
| 4 | File | Owner or manager of the file | `agent_access`: `inherit`, `none`, `read`, `propose`. |
| 5 | Grant | The principal | Projects and scopes the client may use. |
| 6 | Session narrowing | The principal, per conversation (Phase 4) | A file allow-list carried in the token for one task. |

Default for a project that has never set a policy: **agents disabled** until a project
owner enables them. Rationale: schema 40 made new keys "granted" rather than "all"
because the safe state should be the default; the same principle applies.

### 5.2 Project policy fields (proposed)

| Field | Type and values | Default | Meaning |
|---|---|---|---|
| `enabled` | boolean | false | Master switch for the project. |
| `write_mode` | `none`, `draft`, `change_request` | `draft` once enabled | `none`: read only. `draft`: proposals stay private drafts until a person submits. `change_request`: an agent may submit. |
| `submit_requires_person` | boolean | true | Even with `change_request`, a person must press submit. |
| `kinds` | subset of `document`, `spreadsheet`, `presentation` | all | File kinds agents may touch. |
| `clients.allow` | list of client kinds or client ids, or `*` | `[]` | Which AI clients may connect. Empty means none. |
| `clients.require_approval` | boolean | true | A manager approves each new client before its grants take effect. |
| `scopes_max` | subset of `content:*` | `content:read` | Ceiling for grants in this project. |
| `budgets.calls_per_minute` | integer | 60 | Per grant. |
| `budgets.read_bytes_per_day` | integer | 20,000,000 | Per grant; counted in response bytes. |
| `budgets.proposals_per_day` | integer | 20 | Per grant. |
| `budgets.open_proposals` | integer | 5 | Per grant; open drafts and requests. |
| `budgets.ops_per_proposal` | integer | 500 | Engine ops; hard cap 10,000. |
| `dlp.mode` | `off`, `redact`, `block` | `off` | Phase 4. See section 5.4. |
| `egress.require_attestation` | boolean | false | A manager must record that the chosen provider's terms were reviewed. |
| `retention.call_days` | integer | 90 | Retention of read-call rows. |
| `revision` | integer | 1 | Optimistic concurrency, as `project_settings.revision`. |

```json
{
  "enabled": true,
  "write_mode": "change_request",
  "submit_requires_person": true,
  "kinds": ["document", "spreadsheet"],
  "clients": { "allow": ["claude", "chatgpt"], "require_approval": true },
  "scopes_max": ["content:read", "content:propose", "content:submit"],
  "budgets": { "calls_per_minute": 60, "read_bytes_per_day": 20000000,
               "proposals_per_day": 20, "open_proposals": 5, "ops_per_proposal": 500 },
  "dlp": { "mode": "redact", "rules": ["iban", "national_id_bd", "email"] },
  "egress": { "require_attestation": true },
  "retention": { "call_days": 90 },
  "revision": 7
}
```

Storage: `project_agent_policy(space_id pk references project_settings, doc jsonb,
revision, updated_by, updated_at)`; changes write `operation_audit` rows with
before and after digests, using the same domain-bound fingerprint approach as
`document.metadata_changed` (`workspace.rs`), so readable policy values need not be
retained after erasure. Management routes require project owner or manager and an
account session (a device or agent token is `401`, as for key grants).

### 5.3 File policy

`file_agent_policy(document_id pk, access text)` with `inherit` (default), `none`,
`read`, `propose`. A project manager can mark a contract `none` while the rest of the
project is open. The effective value is the minimum of project and file.

### 5.4 DLP hooks (Phase 4)

DLP runs on the read path, before content leaves the API:

| Hook | Behaviour | Notes |
|---|---|---|
| Block rules | A block matching a rule is withheld (`@ref X redacted`) | Cheap, deterministic. |
| Redact rules | Matched spans replaced by stable placeholders (`[iban:1]`) | Block becomes read-only for text ops (a rewrite would destroy hidden text). |
| File labels | `agent_access: none` on the file | The coarse control; works without rule maintenance. |
| External webhook | Server posts candidate text to a customer endpoint with a deadline; `fail_closed` option | Customer owns the classifier; latency is added to reads. |
| Output size | Per-call `max_bytes` | Limits bulk extraction. |

DLP is advisory against a determined user who has the document open anyway. It targets
accidental disclosure to a provider. Limits are in the
[security document](AI-AGENT-SECURITY.md).

## 6. Review-gated writes and attribution

### 6.1 Lifecycle

```
          propose (agent)           submit (agent if policy allows, else principal)
 none ───────────────────► DRAFT ─────────────────────────────► CHANGE REQUEST (open)
                            │  ▲ principal may edit/adopt in the web editor           │
                            │  │                                                     ├─ approvals (people, per rules)
                  withdraw  ▼  │                                                     ├─ changes_requested ─► agent re-proposes
                        WITHDRAWN                                                    ├─ declined ─► (agent informed via status)
                                                                                     └─ MERGE (person with merge right)
                                                                                           └─► events appended; attribution recorded
```

| Actor | May | May not |
|---|---|---|
| Agent identity | Create and extend own drafts; submit or withdraw own proposals (per policy); read status | Approve, decline, merge, edit anyone else's draft, change rules, comment on the team version |
| Principal | See and edit the agent's drafts; submit; withdraw; approve others' requests (if reviewer) | Be the sole approver of their own agent's request (I5) when approvals are required |
| Reviewer, editor, manager | Existing review and merge rights | n/a |

Today `drafts()` and `get_draft` return only `created_by = viewer`, and `edit_draft`,
`sync_draft` and `submit` require `created_by = caller`
(`crates/engine-api/src/workspace.rs`, `reviews.rs`). For agent drafts these checks
must also accept the agent's principal. This is a bounded change to six queries and is
a prerequisite for Phase 1 (listed in section 11).

### 6.2 What a reviewer sees (app change)

- A badge "Claude for Olivia" with the client product, on the request, each changed
  block and the activity feed.
- Per-change `why` text, labelled as the agent's statement.
- A line stating whether formatting was inferred (section 8.3 of the protocol spec).
- A count of ops compiled from macros ("replace all: 80 blocks").
- A warning when the base revision is behind the team version.
- Filters: "agent-authored", by client, by principal.

### 6.3 Attribution layers

Defined in the [protocol spec, section 9.5](AI-AGENT-PROTOCOL-SPEC.md). Summary and
decision framing:

| Layer | Provides | Cost | Phase |
|---|---|---|---|
| A. Draft level | `created_by` and `draft_edit.actor_id` are the agent identity; call ledger; audit rows | New tables only | 1 |
| B. Merge audit | `document.change_request_merged` already names the contributor | None beyond A | 1 |
| C. Sealed | Content events carry the agent as `actor_id`; one sealing event commits to the draft's edits and calls and names the merger and approvals | New event type, CHECK change, reader updates, capability gate | before 2 |

**Why C is the right target.** Today a person looking at the chain sees the merger as
the author of every merged event. That is correct for a person merging a colleague's
work, and it is exactly wrong for reconstructing what an AI wrote. `content_hash`
frames the actor, so carrying the agent in `actor_id` makes authorship tamper-evident
without any new hashing scheme. The sealing event adds what the chain cannot know
otherwise: the merger, the approvals counted, and commitments to the agent's calls.

**Why not do C first.** It changes the canonical event vocabulary. The MIT crates fail
to materialise a log containing an unknown variant
(`MaterializeError::InvalidPayload`), and [PRODUCT-SPEC.md](PRODUCT-SPEC.md) requires
existing events and readers to keep working. So C needs a decision record (015), a
per-document capability, updated verifiers and exporters, and a written compatibility
statement. A and B are useful on their own and carry no such risk.

### 6.4 Sealing event (proposed shape)

```json
{
  "type": "ChangeAttributed",
  "draft_id": "b1f1c9f2-5e1a-4e0f-8d3e-4f9a7c5d2e11",
  "author": { "kind": "agent", "identity": "<agent uuid>", "principal": "<person uuid>",
              "client_kind": "claude" },
  "merger": "<person uuid>",
  "first_seq": 413, "last_seq": 492,
  "calls_root": "<hex sha-256 Merkle root over agent_call.request_digest, ordered>",
  "edits_root": "<hex sha-256 Merkle root over sha256(canonical(draft_edit.payload)), ordered>",
  "approvals": ["<uuid>"], "override": false
}
```

- It is appended in the same transaction as the merged content events by
  `append_to_team`, which gains an `author` parameter for agent-authored requests.
- It carries ids and hashes only: no document text, prompt or model output. Audited
  erasure of a person or document therefore cannot require rewriting it.
- `model` and tool version are not in the event. The server sees only client
  self-descriptions (`clientInfo`, `X-Dynodoc-Client`), which are claims; they are
  stored in `agent_call` as claims and never described as facts.

### 6.5 Audit representation

All rows use `operation_audit(actor_id, action, resource_id, detail)`.

| `action` | `resource_id` | `detail` (no content) |
|---|---|---|
| `agent.authorization.created` / `.revoked` | authorisation id | client id, client name, scopes |
| `agent.grant.set` / `.revoked` | credential id | project, scopes, previous scopes |
| `agent.session_issued` | key or authorisation id | project(s), scopes, expiry (one row per issuance) |
| `agent.policy.changed` | project id | fields changed, before and after digests |
| `agent.client.approved` / `.blocked` | project id | client kind or id |
| `agent.proposal.created` | proposal id | file, base rev, ops in, ops engine, request digest |
| `agent.proposal.submitted` / `.withdrawn` | proposal id | by whom |
| `agent.killswitch` | project or instance | level, actor |
| `agent.denied` | project id | reason code, rate-limited to one row per credential per minute |

The `agent_call` ledger holds per-call metadata (protocol spec 9.6). Together with
`operation_audit` it answers: who authorised which client for which projects, which
calls it made and how large they were, what it proposed, who merged it.

### 6.6 Effects on existing surfaces

| Surface | Change |
|---|---|
| `/structure` provenance | `agent` field already returns `actor_id`; with layer C it resolves to the agent identity, and the response adds `author_kind`. |
| Project activity feed | Event label gains "by Claude for Olivia"; request rows show the agent badge. |
| `/documents/{id}/verify` | Unchanged for chain integrity; adds an optional attribution check recomputing `edits_root` where draft rows are retained. |
| Exports | Provenance JSON adds `author_kind`, `principal`. C2PA or PROV exports (future) map `agent` to a software agent acting on behalf of the principal. |
| Notifications | Reviewers are notified when an agent submits; the principal is notified of agent drafts awaiting submit. |

## 7. Interfaces required

### 7.1 Agent data plane (protocol spec section 12)

New read endpoints (all `GET` under `/projects/{id}/agent/files/{file}/…`): `outline`,
`blocks` (node range, section, batch with ETags via `POST …/blocks:read`), `cells` (A1
range), `sheets/{sheet}/profile`, `slides`, `find`, `diff`; plus `files`, proposals
(`POST`, `GET`, submit, withdraw) and `access`. Fifteen routes in total, with the
discovery document.

### 7.2 Management plane (account session only)

| Route | Purpose |
|---|---|
| `GET/PUT/DELETE /agent/authorizations…` | List, narrow, revoke a client's consent. |
| `PUT/DELETE /devices/{key}/projects/{project}` (existing, scopes widened) | Key grants including `content:*`. |
| `PATCH /devices/{key}` with `class` | Enrol or relabel an agent key. |
| `GET/PUT /projects/{id}/agent-policy` | Project policy with `revision`. |
| `PUT /documents/{id}/agent-policy` | File `agent_access`. |
| `GET /projects/{id}/agent-usage` | Calls, bytes, proposals, denials by client and day. |
| `POST /projects/{id}/agent-policy/kill` | Immediate disable (also revokes sessions). |
| `POST /agent/keys`, `DELETE /agent/keys/{id}` | API keys (Path C). |
| Admin: `POST /admin/agent-gateway` | Operator on or off, client block list. |

A device, agent or connector token on any management route is `401`, and another
person's resource is `404`, following [DEVICE-KEY-GRANTS-API.md](DEVICE-KEY-GRANTS-API.md).

### 7.3 Changes to existing code (bounded list)

1. `AuthContext` and `ProjectAuth`: add the agent token type and `AgentAuth<S>`.
2. Draft and request visibility and ownership checks accept the principal for agent
   drafts (six queries in `workspace.rs`, `reviews.rs`, `project` pages).
3. `request["mine"]` includes the principal for agent-authored requests (I5).
4. `append_to_team`: optional `author` and a sealing event (layer C).
5. `reviews::create` and `provenance::prepare` accept `source.kind = "agent"` and host
   `agent`, if bundles are used for receipts.
6. `device_session.scopes` and grant CHECK constraints widen (migration, new numbers).
7. OpenAPI: typed bodies for the agent routes; generated from the same schemas as the
   protocol spec.
8. `identity` guard: refuse sign-in, magic link and mail for agent identities.
9. Erasure paths: cascade to `agent_identity`, `agent_authorization`, caches.

## 8. Delivery surfaces and priority

Order follows the owner's decision: Claude first, then ChatGPT, then the rest. Details,
access dates and unverified items are in
[AI-AGENT-VENDOR-INTEGRATIONS.md](AI-AGENT-VENDOR-INTEGRATIONS.md).

| Priority | Surface | Transport and auth | Phase | Notes |
|---|---|---|---|---|
| 1a | Claude Code (plugin: skills plus MCP config) | stdio + device key | 0 to 1 | Plugins bundle skills, MCP servers, commands; skills carry the usage guide at zero fixed cost. |
| 1b | Claude Desktop | stdio + device key (config file) | 0 to 1 | Remote connectors are added through the Connectors UI. |
| 1c | Claude Agent SDK | stdio or HTTP + token in headers | 1 | The SDK does not run an OAuth flow; caller passes a token. |
| 1d | claude.ai custom connector (Team: owner adds) | Streamable HTTP + OAuth (CIMD or DCR) | 2 | Callback `https://claude.ai/api/mcp/auth_callback`; calls come from published Anthropic ranges. |
| 1e | Anthropic connectors directory | same + review | 2 | Needs privacy policy, test account, annotations, docs. |
| 2a | ChatGPT custom MCP server (plugins) | Streamable HTTP + OAuth 2.1 (CIMD preferred) | 3 | Web only for MCP apps; no API keys or client credentials. |
| 2b | ChatGPT public directory | review | 3 | Verified organisation, challenge file, test cases. |
| 2c | Custom GPT Actions | OpenAPI | n/a | Being retired for Enterprise workspaces from 11 December 2026; do not invest beyond the OpenAPI we generate anyway. |
| 3 | Gemini CLI, Cursor, Codex, VS Code Copilot, Devin Desktop, Cline, Zed, JetBrains | stdio or HTTP | 1 and 2 | Mostly configuration snippets and a README. |
| 3 | Copilot Studio, Microsoft 365 Copilot, Gemini Enterprise | Streamable HTTP + OAuth, public endpoint | 4 | Enterprise admin gates; public HTTPS required. |
| 3 | Generic REST/OpenAPI | HTTP + API key or OAuth | 2 | Same routes, JSON mirror. |

Intranet and self-hosted installations: stdio clients work anywhere the machine can
reach the API. Cloud-hosted clients call from the vendor's network, so a private server
needs a public HTTPS front end, an IP allowlist for the vendor's published range, or the
vendor's tunnel product (Anthropic documents MCP tunnels as a research preview for
Enterprise by request; OpenAI documents a Secure MCP Tunnel; maturity of both to be
re-checked at build time).

## 9. Security and privacy summary

The full threat model, controls and legal notes are in
[AI-AGENT-SECURITY.md](AI-AGENT-SECURITY.md). The decisions that shape the design:

### 9.1 Controls at a glance

| Threat | Primary control | Secondary |
|---|---|---|
| Prompt injection in document text | No tool can write the team version; every write is a reviewed proposal | DCT escaping; `trust=data`; per-op `why` untrusted; budgets |
| Data exfiltration through the model | Grant bounds what is readable; DLP; per-file labels | Egress policy and attestation; call ledger; no outbound fetch from Dynodoc |
| Confused deputy | Audience-bound tokens; no passthrough; MCP server holds no DB credentials | API re-authorises every call |
| Excessive agency | Eight tools; one write tool; drafts only; scopes ceiling | Budgets; open-proposal cap |
| Token theft | Short access tokens, rotating refresh, reuse detection, revocation | Per-project grants limit blast radius |
| Malicious or mimicking OAuth client | Consent shows verified origin and client name; admin client approval; exact redirect match | CIMD fetch SSRF controls; DCR rate limits |
| Cross-tenant access | Same project and file role checks as the web app | Uniform 403/404; cache keyed by document and chain |
| Reviewer fatigue | Budgets; submit-requires-person; summaries and diffs | Metrics on merge-unchanged ratio |

### 9.2 What Dynodoc cannot see or control

- The conversation, the prompt, the model, the provider's retention or training terms.
  These are the customer's contract with their AI vendor, not Dynodoc's.
- Whether the client behaves safely with other tools in the same session.
- Whether a person pasted document text into a chat directly.

The consent screen and the legal pages must say so plainly.

### 9.3 Privacy position

- Dynodoc stores call metadata, not content. Prompts and completions never reach it.
- The AI client a user connects is **not** a Dynodoc subprocessor; it is a tool the
  customer chose, acting on the user's instruction. The privacy policy and subprocessor
  pages should state this and list the data categories sent to a connected client. They
  are currently drafts "not yet effective" ([LEGAL-PAGES.md](LEGAL-PAGES.md)); this is
  a prerequisite for every directory listing.
- Retention of `agent_call` rows is configurable (default 90 days); proposal receipts
  persist with the proposal.
- Erasure: sealing events and receipts carry ids and hashes only.

### 9.4 End-to-end encryption interaction

If a project or file is client-side encrypted (a separate documented idea), the
server cannot materialise or diff content, so server-side agent reads do not work for
that content. The only agent integration compatible with E2EE is a **local** one, where
the MCP server runs on the user's machine, holds the keys and talks to the server
only in ciphertext. That would be a different tool set (decrypt locally, propose
encrypted deltas). Not designed here; flagged so the two designs are not built in
conflict. `agent_access: none` is the default for any file flagged encrypted.

## 10. Operations

| Topic | Design |
|---|---|
| Topology | `engine-mcp` (HTTP) and `engine-api` behind the existing nginx; a dedicated host name for the MCP resource (for example `mcp.<domain>`) so protected-resource metadata and the canonical `resource` URI are stable; consent UI on the app host. Local `engine-mcp` is a user-run binary. |
| Configuration | `AGENT_GATEWAY` (on, off), `AGENT_PUBLIC_URL`, token lifetimes, allowed redirect origins, CIMD fetch allow/deny and egress proxy, DLP provider, retention. Secrets (Paseto key already exists) stay in the API environment. The MCP process needs none. |
| Deployment | Same release routine as schema 38 to 40 ([operations records](operations/2026-10-13-schema40-release.md)): backup, production-copy rehearsal, additive migration, compatible rollback. New tables are additive, so a rollback to the previous API leaves them unused. |
| Upgrade and rollback | Layer C is the one irreversible step per document (events with a new type). It needs a capability flag and a statement that older readers cannot materialise those documents. |
| Observability | Per tool: calls, latency, bytes, estimated tokens, error codes, cache hit ratio, `have` hit ratio; per grant: budget use, denials. Logs carry paths and codes, not bodies, tokens or document text (existing rule). Trace context stored on call rows. |
| Capacity | The state cache and per-grant budgets bound memory and rate. Today full state is still materialised per uncached read; the evolution plan's chunked storage remains the long-term fix ([ENGINE-EVOLUTION-PLAN.md](ENGINE-EVOLUTION-PLAN.md)). No capacity claim is made. |
| Failure modes | Policy or grant store unavailable: deny. Cache cold: slower. Idempotency ledger unavailable: refuse writes. AS unavailable: existing sessions continue until expiry; new consent fails. DLP webhook down: per-policy `fail_closed` or skip with a warning. |
| Incident response | Revoke grants for a client kind or a principal in one statement; set `AGENT_GATEWAY=off`; rotate the Paseto key (ends all sessions; documented as heavy); audit rows identify affected projects. |
| Support | A "what my assistant can see" page per principal (grants, last calls by day, bytes); a project page of agent proposals. |

## 11. Delivery plan

### 11.1 Phases

Effort is in person-weeks. Assumptions: one senior engineer fluent in this Rust
codebase for engine and API work, plus 0.5 of a SvelteKit engineer for the private app
repository, plus review time from a security reviewer. **Excluded** from every number:
legal and privacy-policy adoption, waiting time for vendor reviews, the formula-engine
replacement, a tenant or organisation object, the self-hosted and end-to-end-encryption
editions, user research beyond the stated usability test, localisation, marketing.
Confidence is the author's judgement, not a measurement.

| Phase | Scope | Effort | Confidence | Depends on |
|---|---|---|---|---|
| -1 Measurement | Fixture corpus, harness skeleton, baseline arms B0 and B1, byte benchmark in CI | 2 to 3 | High | none |
| 0 Read-only stdio MCP | `engine-agent` codec and reads, state cache, agent routes, `device_key_class`, scopes, `engine-mcp` stdio, Claude Code plugin and skill, tests | 7 to 9 | Medium | -1 |
| 1 Propose | Op compiler, `between`, agent identity, drafts and receipts, attribution A and B, visibility changes, review badges (app), policy v0, evals | 11 to 13 | Low to medium | 0 |
| 2 Remote MCP and OAuth | Streamable HTTP, authorisation server (CIMD, DCR, PKCE, rotation), consent and grants UI, API keys, budgets, conformance, ops; claude.ai custom connector end to end | 10 to 12 | Medium | 1 |
| 2b Seal attribution | Decision 015, `ChangeAttributed`, capability gate, verifiers | 3 to 4 | Medium | decision |
| 3 ChatGPT | `chatgpt` profile (`search`, `fetch`), annotation audit, CIMD checks, challenge file, submission pack, developer-mode test on a Business workspace | 4 to 6 | Medium | 2, legal pages adopted |
| 4 Policy, DLP, enterprise | Layered policy, DLP rules and webhook, usage dashboards, audit export, session narrowing, kill-switch UI, Copilot Studio and Gemini Enterprise adapters | 8 to 10 | Low | tenant object (open question 3) |
| **Total** | | **45 to 57** | Low | |

### 11.2 Phase 0 detail (read-only, stdio)

| Task | Weeks | Notes |
|---|---|---|
| `engine-agent`: refs, hashes, DCT writer, outline, read, find, diff, escaping | 2 | Pure functions; golden tests. |
| Bounded state cache keyed by `(doc, rev, chain)` | 1 | Purge on erasure. |
| Agent routes 1 to 9 and 13 to 15, `AgentAuth`, scopes, migration 0041 | 3 | Mirrors `ProjectAuth<S>`. |
| `engine-mcp` stdio on `rmcp`, tool definitions, `instructions`, profiles | 1 | stdout hygiene test. |
| Claude Code plugin (manifest, skill with DCT legend, `.mcp.json`), Desktop config guide | 0.5 to 1 | |
| Contract, golden, authorisation, revocation tests; read-path security review | 1.5 | |

Acceptance: eight tools list as designed (six for a read-only grant); outline, read,
find and diff pass the golden corpus; no route writes anything except call metadata; a
revoked grant fails on the next request; byte benchmark published to the repository
with the harness; a person can follow the guide and read a document from Claude Desktop
and Claude Code. Rollout: `AGENT_GATEWAY=off` in production; enabled on a staging
stack and one dogfood project.

**Implementation status, MCP server and Claude Code plugin, 20 October 2026.** (This
paragraph supersedes "Not built: ... `engine-mcp`, the Claude Code plugin and skill" in the
18 October note below; the other items of that list are unchanged by it.) Built: the library
crate `crates/engine-mcp` (a hand-written, dependency-light MCP server over newline-delimited
JSON-RPC: `initialize`, `server/discover`, `ping`, `tools/list`, `tools/call`; the six
read-only tools `files`, `outline`, `read`, `find`, `diff` and `status`, each defined once and
validated from its published schema; an HTTP backend for protocol spec 12.1 routes 1 to 5, 7
to 9 and 14, with ETag and cursor carry-through, one session renewal on `401`, `dd/1 error`
lines for every failure, and byte, time and redirect limits), the `engine-cli mcp` verb
(Path A: it loads the agent key like `sync` does and keeps a one-hour device session in
memory), the Claude Code plugin under `integrations/claude-code/dynodoc/` (manifest,
`.mcp.json`, a short skill with the DCT legend) and the guide
[AI-AGENT-CLAUDE-SETUP.md](AI-AGENT-CLAUDE-SETUP.md). Evidence: 70 tests in `engine-mcp` and
15 new ones in `engine-cli` (plus one ignored, below), including the real process on pipes
with every log level on, and tests that pin the tool definitions to the specification text, the
requests to the spec 16.1 transcript and the error lines to the `engine-agent` error table.
The decisions (a verb in `engine-cli`, not a second binary; `rmcp` not used) and the
deviations from the specification are in the crate documentation (`cargo doc -p engine-mcp`).
Not built: any write tool, Streamable HTTP and OAuth, the `flat` and `chatgpt` profiles, the
`dynodoc_workflow` prompt, a marketplace entry, a `.mcpb` bundle, release packaging of
`engine-cli`, the "Use for an AI agent" choice on the signing-keys screen. **Not verified:**
no real MCP client (Claude Code, Claude Desktop) and no real `engine-api` was run against it
(the tests use stand-in servers); the 2026-07-28 details (the `server/discover` result, the
`cacheScope` values, the request metadata key) and every vendor behaviour in the plugin and
the guide are marked "unverified: check at build time". `engine-cli mcp` cannot start a
session until `content:read` is among the scopes `engine-core` signs for a device session (the
agent-routes work adds it); one ignored test, `content_read_is_a_scope_engine_core_signs`,
checks that after the merge. The configuration snippets of the vendor document (section
12.1) name `dynodoc-mcp`; the built command is `engine-cli mcp`.

**Implementation status, 18 October 2026.** Built, in the new library crate
`crates/engine-agent` (pure functions over `DocumentState`, no I/O): refs, hashes,
revision tokens, A1 and slide addressing; the DCT v1 writer with strict reference
parsers; escaping and injection hygiene; the byte budget and cursors; `files`, `outline`,
`read`, `find` and `diff` returning typed results, DCT text and the JSON mirror; the
error table (26 codes); golden transcripts, hostile-text, property and conformance tests
over a fixture corpus built with real engine operations; and a byte benchmark with one
committed run (`docs/benchmarks/agent-dct-bytes-20261018.json`, bytes only, no
tokeniser). Not built: the state cache, agent routes, `AgentAuth`, scopes and migration,
`engine-mcp`, the Claude Code plugin and skill, the TypeScript side of the shared
vectors, `fmt=md` cell annotations for ranges, and any measurement of model behaviour or
real token counts. Decisions and deviations from the protocol spec are listed in the
crate documentation (`cargo doc -p engine-agent`).

### 11.3 Phase 1 detail (propose)

| Task | Weeks |
|---|---|
| Op compiler: `set_text`, `replace`, `replace_all`, `insert`, `delete`, `move`, `set_type`; `between()` port with shared vectors | 4 to 5 |
| Sheet `set_cells`, `clear`; slide `set_shape`, `set_notes` | 1.5 |
| Agent identity, `agent_call`, receipts, idempotency, proposal routes, submit and withdraw | 2.5 |
| Visibility and ownership changes for agent drafts; I5 rule | 1 |
| App: badges, inbox, per-change `why`, behind-base warning, adopt-draft action | 2 |
| Project policy v0 and management routes | 1 |
| Property tests, gate test, eval suite v1 (20 tasks), reviewer usability test (5 users) | 2 |

Acceptance: the gate test proves no event is appended by `propose` or `submit`;
property tests pass; the evals show success rate not worse than the B1 baseline
(non-inferiority margin pre-registered) with the measured token reduction reported
with confidence intervals; reviewers can tell agent from person in every surface.
Rollout: per-project opt-in, agents default disabled, dogfood then pilot teams.

### 11.4 Phase 2 detail (remote MCP and OAuth)

| Task | Weeks |
|---|---|
| Streamable HTTP binding, `server/discover` and legacy handshake, Origin checks | 1.5 |
| Authorisation server: metadata, authorize, token, revoke, DCR, CIMD fetch with SSRF controls, PKCE, `resource`, `iss` | 4 to 5 |
| Consent page and authorisations management UI | 2 |
| Refresh rotation, reuse detection, revocation, introspection for the MCP server | 1 |
| API keys (Path C) | 0.5 |
| Per-grant budgets and rate limits | 1 |
| Conformance (Inspector, SDK suite, claude.ai manual matrix), pen test fixes | 1.5 |
| nginx, host name, monitoring, runbooks | 1 |

Acceptance: claude.ai custom connector completes OAuth and a propose-and-review cycle
on a Team plan test organisation; Claude Code connects over HTTP with OAuth; revocation
and budget tests pass under concurrency; external penetration test findings of high
severity closed.

### 11.5 Test strategy

Layers are defined in [protocol spec section 17](AI-AGENT-PROTOCOL-SPEC.md). Additions
at system level:

| Test | Purpose |
|---|---|
| Contract tests from one schema source | MCP tool definitions, HTTP OpenAPI, JSON mirror and DCT grammar cannot drift. |
| Replayable agent evals | Record complete tool transcripts from live model runs; replay them against the server in CI to catch output drift; run live evals on a schedule with pinned model versions. Success is checked by a deterministic task checker, not by a model. |
| Differential tests | Agent-compiled proposals applied by an independent reference implementation (the web editor's state-diff path) must yield equal state. |
| Security regression | Injection corpus embedded in documents; assertions are about server behaviour (no write path, no scope escalation, budgets), not about model behaviour. |
| Soak and concurrency | Concurrent proposals on one file; grant revocation during `propose`; two clients using one request id. |
| Upgrade rehearsal | Production-copy upgrade and rollback exactly as for schemas 38 to 40. |

### 11.6 Migration and compatibility

- Migrations are additive (tables, CHECK widening). Applied migrations are not edited.
- Layer C adds an event type; documents opt in with a capability flag recorded in an
  immutable table, set only after all deployed readers support it.
- Existing connectors, device sessions and history sync are unchanged.
- API compatibility: new routes only; `{error, code, details}` retained.

## 12. Benchmark methodology (summary)

The benchmark that must pass before any savings claim is specified in
[AI-AGENT-TOKEN-EFFICIENCY.md](AI-AGENT-TOKEN-EFFICIENCY.md). In short: pre-registered
tasks over fixed corpora; three arms (raw REST passthrough, thin text wrapper, this
protocol); at least two model families with pinned versions; provider-reported token
usage; at least 10 runs per cell; paired comparisons with bootstrap intervals; a
success checker independent of any model; harness and raw logs published. Until it
runs, the only numbers in these documents are payload byte counts on a synthetic
fixture and design targets.

## 13. Risks

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Reviewers rubber-stamp agent proposals | Medium | Bad edits merge | Diff-first UI; budgets; merge-unchanged metric; policy requiring approvals; I5 |
| R2 | A prompt-injected agent proposes a plausible malicious edit | Medium | Content integrity | Review gate, per-op `why`, no write to team version, agent identity visible |
| R3 | Provider data exposure beyond customer expectation | Medium | Privacy and contractual | Consent text, egress attestation, DLP, per-file `none`, documentation |
| R4 | Token stolen from a client machine | Medium | Read or propose within grant | Short lifetime, rotation, reuse detection, grant revocation, per-project grants |
| R5 | Vendor spec or policy churn (MCP revisions, directory rules) | High | Rework | Official SDK, HTTP binding as source of truth, profiles, quarterly vendor review |
| R6 | Directory rejection (privacy policy not adopted, annotations, test account) | Medium | Delay | Treat legal adoption as an explicit dependency; submit after Phase 2 hardening |
| R7 | Formula engine replacement delays spreadsheet features | Medium | Capped sheets scope | Gated ops; literal edits first |
| R8 | Agent identity design collides with member lists, mail, notifications | Medium | Bugs | Synthetic address guard; tests across notification and member queries |
| R9 | Op compiler corrupts formatting | Low to medium | Data quality | Property tests, `make_patch` round-trip guard, review shows formatting inferred |
| R10 | Cost of state materialisation on large files under agent load | Medium | Latency | Cache, budgets, admission limits; chunked storage later |
| R11 | Sealing event splits readers | Low | Verification failures | Capability gate, compatibility statement, staged enablement |
| R12 | Savings claims challenged | Medium | Credibility | Benchmark first; claims policy in the token document |

## 14. Open questions

Each has a recommended default and a decider.

| # | Question | Recommended default | Decider |
|---|---|---|---|
| 1 | When do we implement layer C (sealed attribution), and as agent-as-actor or merger-as-actor? | Decision record 015 before Phase 2; agent-class requests only, agent as `actor_id`, merger in the sealing event | Owner with engine lead |
| 2 | Synthetic `.invalid` address for agent identities, or nullable email? | Synthetic address plus guards | Engine lead |
| 3 | Is there an organisation or tenant object for layer 2 policy and org-wide kill switch? | No; Phase 4 starts with project and instance layers and revisits when the self-hosted design defines tenancy | Owner |
| 4 | Where do OAuth endpoints and the MCP resource live (host names)? | Dedicated `mcp.` host for the resource; authorisation server under the API host; consent UI in the app | Engine lead with operator |
| 5 | Minimal consent page in the MIT engine for headless installs? | Yes, a plain server-rendered page; the app replaces it | Engine lead |
| 6 | Token lifetimes? | Access 15 minutes; refresh sliding 14 days, absolute 90 | Security reviewer |
| 7 | Default `submit_requires_person`? | True when `required_approvals > 0`, else false | Project owner (policy), owner (default) |
| 8 | Require a manager to approve each new client before its grants work? | Yes for projects with `required_approvals > 0` | Owner |
| 9 | Offer long-lived API keys (Path C) at all? | Yes, project-scoped, at most 90 days, off by default per project | Owner |
| 10 | Which vendor directory first? | Anthropic connectors directory after Phase 2; ChatGPT after Phase 3 | Owner |
| 11 | Who adopts the privacy policy and subprocessor wording needed for listings? | Owner with counsel before Phase 2 exit | Owner |
| 12 | Allow agents to leave canonical document comments? | No in v1 | Owner |
| 13 | Retention of read-call rows | 90 days, configurable | Owner with counsel |
| 14 | Treat the principal as contributor for I5 only when approvals are required? | Always, to keep one rule | Engine lead |

## Appendix A. Glossary

| Term | Meaning |
|---|---|
| Agent identity | Identity row of class `agent`, acting for a principal |
| CIMD | OAuth Client ID Metadata Document: a client identified by an HTTPS URL that serves its metadata |
| DCR | OAuth Dynamic Client Registration (RFC 7591) |
| DCT | Dynodoc Compact Text, the model-facing format |
| Grant | Revoke-only record of project and scopes a credential may use |
| MCP | Model Context Protocol |
| Principal | The person an agent acts for |
| Proposal | An agent's draft or change request |
| Resource indicator | OAuth `resource` parameter (RFC 8707) binding a token to a server |

## Appendix B. References

Repository: [PRODUCT-SPEC.md](PRODUCT-SPEC.md), [ENGINE-EVOLUTION-PLAN.md](ENGINE-EVOLUTION-PLAN.md),
[PROVENANCE-SYNC.md](PROVENANCE-SYNC.md), [PROJECT-SCOPED-DEVICE-SESSIONS.md](PROJECT-SCOPED-DEVICE-SESSIONS.md),
[DEVICE-KEY-GRANTS-API.md](DEVICE-KEY-GRANTS-API.md), [PROJECT-POLICY-AND-METADATA.md](PROJECT-POLICY-AND-METADATA.md),
[SECURITY-HARDENING.md](SECURITY-HARDENING.md), [BROWSER-AND-WORKLOAD-SECURITY.md](BROWSER-AND-WORKLOAD-SECURITY.md),
[research/AGENTIC-DOCUMENTS-RESEARCH.md](research/AGENTIC-DOCUMENTS-RESEARCH.md),
[research/AI-GOVERNANCE-RESEARCH.md](research/AI-GOVERNANCE-RESEARCH.md).

External (accessed 14 October 2026; per-claim tags in the vendor document):

- MCP specification and versioning: https://modelcontextprotocol.io/specification/versioning
- MCP authorization (2026-07-28): https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/index
- MCP security best practices: https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices
- Anthropic custom connectors: https://claude.com/docs/connectors/custom/add-unlisted.md
- Anthropic directory review criteria: https://claude.com/docs/connectors/building/review-criteria.md
- OpenAI plugins and MCP: https://developers.openai.com/plugins/build/mcp-server ,
  https://developers.openai.com/plugins/build/auth
- OWASP Top 10 for LLM Applications 2025: https://genai.owasp.org/llm-top-10/
