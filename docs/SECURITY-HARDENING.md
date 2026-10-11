# Security hardening and remaining gates

Updated 11 October 2026. This record tracks the owner's historical security findings against
current code. It is a source/release checklist, not a claim that the whole platform
or its audit history is secure against a database administrator. It takes priority
over historical PoC auth descriptions. Continue the engine evolution plan after
the outstanding security boundaries below are addressed.

## First hardening increment

Implemented, verified in both repositories and deployed as application `858801c`,
schema 29. Shared periodic writes remain off. The isolated restore/upgrade/rollback,
restricted runtime, live browser/proxy and connector checks pass; see
[release evidence](operations/2026-10-11-security-runtime-release.md) and
[the current handoff](ENGINE-EVOLUTION-HANDOFF.md). Native-host acceptance,
byte/memory bounds and independent commitments are separate open gates.

| Finding | Current source result | Outstanding work |
|---|---|---|
| Legacy snapshots mutable; replay not checked | **Deployed and verified.** Migration 0028 adds immutable snapshot/receipt guards, replay-based backfill and a checked reader/writer shared by serving paths. API/CLI verification includes non-node state. Existing-row/corruption cases pass in synthetic acceptance; production currently has zero legacy snapshots. | Retain the [upgrade contract](VERIFIED-SNAPSHOTS.md) for other databases: old rows require explicit replay backfill. Receipts are internal attestations, not external signatures. |
| API uses migration/owner database credentials | **Deployed and verified.** Schema 0029 separates operator migrations/provisioning from the restricted API login. Runtime privilege denials, upgraded-copy workflows and compatible rollback pass. | Follow [the database contract](DATABASE-PRIVILEGES.md) for future upgrades. Database owners and permitted audited erasure remain trusted boundaries until independent commitments exist. |
| Missing service key exposes sign-in tokens | **Deployed.** Both issuance routes reject absent, blank, short, missing or incorrect service credentials regardless of APP_ENV; startup rejects missing/short keys and zero PASETO keys. | DEV_AUTH is explicitly local-only and is disabled on the live web/admin. Real provider login/email delivery acceptance remains separate. |
| Logout only clears a cookie | **Deployed.** Migration 0027 binds new tokens to a login ID; logout revokes its refresh lineage and access tokens. Rotation/logout serialize on the identity row. Web clears cookies only after successful revocation. | Retain the legacy-cookie compatibility warning below. API/browser tests and isolated candidate/rollback workflows pass; independently paired editor grants retain their own revocation lifecycle. |
| Event hash omits envelope metadata | **Open by design of v1; misleading row comment corrected.** | Introduce versioned envelopes binding document/project, parents, sequence, event ID, actor and content root. Add device/server signatures and independent pinned/witnessed heads per evolution part 6. Preserve old hashes and label their trust limits. |
| CSP/HSTS incomplete | **Deployed and browser/proxy verified.** Nonce/hash script CSP, HTTPS HSTS, restrictive ordinary-page framing and scoped Office pane/command-frame policies are live. | [Exact policy and limits](BROWSER-AND-WORKLOAD-SECURITY.md). Mock SDK/bridge tests and hosted delivery are not installed-Office/Google acceptance. |
| File policy can weaken project rules | **Deployed and verified.** Effective floors and project authority govern file rules, direct writes, reviewed merges and moves; old weak overrides cannot weaken the project. | Follow the [policy/metadata contract](PROJECT-POLICY-AND-METADATA.md). Existing directly shared file access remains additive; this is not per-user SQL isolation. |
| Arbitrary metadata/settings changes lack audit | **Deployed and verified.** Supported bounded settings patches; governing management for settings/archive; protected-team rename; atomic before/after digest audit with no-op/rollback tests. | Old changes are not retroactively audited; operational audit is not independently signed. |
| Event routes load full history; expensive reads unbounded | **Deployed for REST/SSE row pagination and process-local admission.** REST uses SQL LIMIT/lookahead; SSE pins a head and reads 100-row pages. Response/SSE slots, identity/project expensive-read budgets, SQL deadlines and container/proxy limits are live. | These do not bound per-event or total document bytes, preempt all synchronous CPU work, index search or provide fleet-wide tenant fairness. See [workload limits](BROWSER-AND-WORKLOAD-SECURITY.md). Other historical readers still hydrate full state. |
| Public source archive includes stray worktree files | **Deployed and byte-verified.** `/source` serves 632 files from the exact committed deployment tree with SOURCE_REVISION and hashes. Untracked/staged-only/modified working files cannot enter it. | Package each future deployed revision. Live checks exclude ops/status.json and env files; tracked dynodoc_interactive.html is intentional source. |
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

### Acceptance checklist: snapshot source and next database-role increment

- Completed in source: inventory and checked legacy snapshot producers/readers,
  including core materialization, named versions, public links, deployed pins,
  restore and audit exports. No-op snapshot updates in `workspace.rs` and
  `product.rs` are removed. Migration 0028 adds guards without editing applied
  migrations or automatically attesting old states.
- Completed in source: compare full reconstructed state and document/revision/chain
  bindings before issuing receipts. Ordinary reads validate a stored receipt and
  checked suffix; explicit audit/backfill streams events and compares one snapshot
  at a time. Full document state remains resident; request/tenant resource budgets
  are still open. A hash of stored bytes alone does not prove replay equivalence.
- Passed locally in both repos: pre-0028 upgrade, corruption and serving-path
  regressions, concurrent creation, restore atomicity and approved erasure. CLI
  backfill also verified the synthetic browser-workflow database. Shared checkpoint
  validation remains a separate contract. These checks do not replace rehearsal
  against a verified copy of existing production data before rollout.
- Implemented in source: separate migration-owner/runtime commands and an explicit
  runtime allowlist. Focused actual-login tests deny history UPDATE/DELETE/TRUNCATE,
  trigger disabling, DDL and escalation; audited erasure still works. Startup checks
  versions/checksums and rejects unsafe credentials without migrating. Both repos
  pass full local verification and CI; evidence is recorded in the handoff. Never
  broaden grants to hide a workflow failure. The owner URL must stay out of the
  API environment.
- Passed: restored production-copy upgrade and compatible rollback with synthetic
  sessions/workflows, followed by the verified live upgrade. The linked release
  records the backfill sequence, two deployment-checker corrections and cleanup.
  The database owner can still rewrite privileged state; independent commitments
  remain an explicitly open audit boundary. Repeat acceptance for future upgrades.

### Subsequent order

The snapshot/role upgrade, compatible rollback, browser policies and first
single-instance workload budgets pass and are deployed. Continue with:

1. Complete bounded text/value/asset chunks and shared named-version/draft references.
2. Portable project-wide commits/parents/branches, durable local repositories and
   resumable missing-object exchange; constraint-aware offline convergence.
3. Full format fidelity, independent audit commitments and measured large-file/service
   limits. Installed Office/Google acceptance and marketplace preparation can proceed
   where host contracts are stable. Sideload packages remain unaccepted in real hosts.

Current personal drafts still belong to one file. None of this increment makes an
offline queue a portable commit graph, enables background capture or proves service
capacity for hundreds of thousands of people.
