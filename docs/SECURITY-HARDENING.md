# Security hardening and remaining gates

10 October 2026. This record tracks the owner's historical security findings against
current code. It is a source/release checklist, not a claim that the whole platform
or its audit history is secure against a database administrator. It takes priority
over historical PoC auth descriptions. Continue the engine evolution plan after
the outstanding security boundaries below are addressed.

## First hardening increment

Implemented locally; final verification and source references are recorded in
[the handoff](ENGINE-EVOLUTION-HANDOFF.md). These changes are not yet deployed.
Production remains application `ecbc643`, schema 26, shared periodic writes off.

| Finding | Current source result | Outstanding work |
|---|---|---|
| Legacy snapshots mutable; replay not checked | **Open, highest priority.** Shared checkpoints already have object/reference validation, but legacy snapshot reads do not establish replay equivalence. | Add immutable snapshot guards with explicit audited erasure compatibility; remove no-op upserts first; verify/backfill legacy checkpoints from canonical history and fail closed on mismatch. Cover public versions, deployed pins, restore and audit reads. Do not replay all history on every ordinary read. |
| API uses migration/owner database credentials | **Open, highest priority.** | Separate migration owner and constrained runtime credentials, restrict privileges including TRUNCATE/trigger/schema changes, test administration/erasure, rehearse upgrade and compatible rollback. Database owner remains a trusted administrator until independent commitments exist. |
| Missing service key exposes sign-in tokens | **Fixed in source.** Both issuance routes reject absent, blank, short, missing or incorrect service credentials, regardless of APP_ENV. Startup rejects missing/short keys and zero PASETO keys before DB access. | Deploy API/web together after rehearsal. DEV_AUTH is still an explicit local-only web bypass; never enable it publicly. |
| Logout only clears a cookie | **Fixed in source.** Migration 0027 binds new tokens to a login ID. Logout revokes its refresh lineage and access tokens. Rotation/logout serialize on the identity row. Web clears cookies only after successful revocation. | Deploy; retain compatibility warning below. Browser and API regressions cover copied credentials, separate logins, rotated ancestors, races and stream data after revocation. |
| Event hash omits envelope metadata | **Open by design of v1; misleading row comment corrected.** | Introduce versioned envelopes binding document/project, parents, sequence, event ID, actor and content root. Add device/server signatures and independent pinned/witnessed heads per evolution part 6. Preserve old hashes and label their trust limits. |
| CSP/HSTS incomplete | **Open.** Marketing host has HSTS; app host lacked it in the read-only audit. Connector frame-ancestor policy alone is not a script CSP. | Nonce/hash CSP, scoped Office embedding, HTTPS HSTS and native-host/browser regression matrix. Avoid breaking Office panes with blanket frame denial. |
| File policy can weaken project rules | **Open, highest priority after snapshot/runtime boundaries.** | Enforce project policy as a floor and project authority for exceptions. Include file creators who are only project editors, existing overrides, move/inheritance, direct writes and merges; update effective-policy UI. Simply OR-ing protection flags is insufficient if file ownership still bypasses them. |
| Arbitrary metadata/settings changes lack audit | **Open.** | Define field-specific authority and settings schema, audit changes transactionally, preserve editor flows and protect project rules. |
| Event routes load full history | **Fixed for REST pagination and SSE catch-up in source.** REST uses SQL LIMIT with lookahead; SSE pins a head and reads 100-row pages, then live events. Subscriber admission is atomic. Resume headers and overflow/future positions are handled. | These are row bounds, not a byte/tenant/time budget. Bound search, expensive operations, per-event bytes and total concurrent work; add statement deadlines and rate limits. Other historical readers are not changed by this increment. |
| Public source archive includes stray worktree files | **Fixed in source.** Package the selected committed Git tree, record SOURCE_REVISION, retain secret/build exclusions. Untracked, staged-only and modified working files cannot enter the archive. | Rebuild the archive from the actual deployment commit when releasing. The audited live archive did not contain ops/status.json; tracked dynodoc_interactive.html is an intentional source file, not evidence of a secret leak. |
| Expression and graph recursion unbounded | **Fixed at expression admission and relevance-cycle traversal in source.** Expressions allow at most 64 KiB, nesting 32 and 256 term entries (including groups/unary terms); the resulting AST also bounds recursive visitors/drop. Dependency DFS uses explicit frames. UTF-8 operator-boundary panic also corrected. | These checks are not a global JSON/format parser budget or a large-document performance guarantee. Previously accepted over-limit expressions now yield a validation error. |
| OpenAPI check absent from application CI | **Fixed in source.** CI checks generated YAML, including schema-only changes. Standalone CI already did this. | Keep regenerated frontend API types and both YAML copies in sync. |
| Docs duplicates unchecked Apps Script helpers | **Fixed in source.** All three Google hosts receive Common.gs from google-common; packages and CI drift checks include it. | Existing manual Docs installations must add Common.gs when updating. Installed-host acceptance is still required. |
| JSON-LD less-than escape is ineffective | **Fixed in source.** Emits literal JSON Unicode escapes; regression verifies HTML cannot introduce a second script element and data round-trips. | No dynamic exploit was claimed for the former static data. |
| Same-host backups | **Owner-deferred.** | Do not describe same-VPS backups as independent recovery. Off-site backup destination/setup awaits changed owner direction. |

## Session compatibility and rollout

New login tokens carry a random `session_id`; all refresh successors keep it. Each
authenticated request checks account generation and an active refresh successor.
An unexpired rotated ancestor can revoke its lineage, covering a stale cookie and
concurrent refresh. Retries are idempotent. An already-open SSE connection stops
before delivering its next protected event after revocation; an idle connection
can keep heartbeats until then. In-flight operations that passed authorization
before logout are not rolled back. Independently paired editor grants have their
own expiry/revocation and are not the web login being closed.

Pre-migration cookies lack a session ID. Logging out with one increments the account
generation, invalidating all existing logins and generation-bound connector grants
for that account. This is required to invalidate its unbound access token. A new
login is not invalidated by retrying that old logout. No existing hash, event or
snapshot is rewritten by migration 0027.

An old API ignores session IDs and would bypass the new per-login revocation check.
**Do not roll back to an unmodified schema-26 API with active credentials.** A rollback
needs a compatible build that retains revocation enforcement, or deliberate
credential invalidation before activation. Rehearse upgrade/rollback on synthetic
data and take a verified production backup before release. Archive packaging now
defaults to HEAD; deploy/build/package the same committed revision. A dirty server
worktree is not a supported release input.

## V1 audit statement

`content_hash` length-frames type, target, payload and actor; `chain_hash` is
SHA-256(content_hash || prev_chain_hash). Sequence contiguity is separately checked.
Document ID, event ID, sequence and creation time are not inside this v1 hash.
The historical comment in applied migration 0002 is inaccurate and is deliberately
not edited. Node Merkle roots are not full-state replay attestations. Internal
hash consistency cannot detect a privileged complete rewrite without an independent
commitment. Snapshot guards alone cannot establish that old stored states were
correct before those guards were installed.

## Resume order

1. Snapshot replay verification/immutable guards and runtime database privilege split,
   with fixtures for corruption, old snapshots, erasure and rollback.
2. Project policy floor/authority and audited metadata; CSP/HSTS compatible with all
   six editors; search/deadline/rate/concurrency budgets.
3. Complete bounded text/value/asset chunks and shared named-version/draft references.
4. Portable project-wide commits/parents/branches, durable local repositories and
   resumable missing-object exchange; constraint-aware offline convergence.
5. Full format fidelity, independent audit commitments and measured large-file/service
   limits. Installed Office/Google acceptance and marketplace preparation can proceed
   where host contracts are stable. Sideload packages remain unaccepted in real hosts.

Current personal drafts still belong to one file. None of this increment makes an
offline queue a portable commit graph, enables background capture or proves service
capacity for hundreds of thousands of people.
