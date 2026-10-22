# Database runtime boundary

Schema 45 binds reviews to the revision a reviewer read. `change_request_review` stays in the
SELECT/INSERT history class and gains `draft_revision` and an identity column `seq` (the order of
one reviewer's decisions, in commit order); the runtime login needs USAGE, not UPDATE, on
`change_request_review_seq_seq`, so **re-run `grant-runtime` after migrating** (`check-runtime`
fails until then). `workspace_draft` gains `content_revision`; two triggers keep the revision
counter monotone and refuse a deciding review for any revision but the draft's current one.

Schema 44 adds two tables for the review-gated write path of AI agents. `agent_identity`
joins the SELECT/INSERT/UPDATE class (no DELETE): the sidecar of an agent identity (the
`identity` row under which an agent's drafts and audit rows are recorded), created once and
moved only by one `revoked_at` transition; a trigger refuses every other change and every
delete, for every login. `agent_proposal_receipt` joins the SELECT/INSERT history class (no
UPDATE, no DELETE): one append-only row per `propose` call that stored edits and per `submit`
or `withdraw` that changed state (counts, the stale refs, the agent's own reasons, the request
digest; no document text). It refuses UPDATE by trigger and DELETE except as the cascade of an
audited whole-document erasure or of the erasure of the person the agent acted for
(`agent_proposal_receipt_erasure_guard`, which checks the same-transaction receipt and the
trigger depth). Triggers also keep the agent identity out of `document_access`, `space_member`,
`change_request_review`, `connector_grant` and `device_signing_key`
(`refuse_agent_identity`), keep the reserved `agents.invalid` address domain for agent
identities alone (`agent_identity_domain_guard`, a deferred constraint trigger on `identity`),
refuse to enable or erase an agent identity as an account, and revoke a person's agent
identities when the person is disabled or erased (`revoke_removed_account_agents`, beside
`revoke_removed_account_devices` of schema 33). `erase_agent_children` is a second
`SECURITY DEFINER` trigger entry point, like `erased_identity_guard`: it deletes the drafts of
the erased person's agent identities in the erasure transaction, after that guard has accepted
the erasure receipt; `document_erasure_child_guard` is replaced (one clause added) so that it
lets those `draft_edit` rows go. The runtime login has no EXECUTE on any of these functions.
The migration also widens the scope checks with `content:propose` and `content:submit` (each
needs `content:read` in the same array; a CHECK on both scope arrays says so),
`agent_call.tool` with `propose` and `submit` (added `NOT VALID`: the new set only adds
values, so the exclusive lock does not wait for a scan of the ledger), adds `client_kind` to `device_key_class` and
the write fields to `project_agent_policy`. The startup check counts both new tables;
re-run `grant-runtime`, then `check-runtime`, after migrating. See
[AI agent connectors](AI-AGENT-CONNECTORS.md).

Schema 43 adds two tables to the SELECT/INSERT/UPDATE class (no DELETE for the runtime login):
`project_agent_policy`, the current agent policy of a project (no row means agents are
disabled; `enabled` defaults to false), and `file_agent_policy`, the per-file override. They
hold the current value only, edited in place; the history of every change is its
`operation_audit` row, written in the same transaction (the audit table is append-only). A
trigger lets the policy revision move by exactly one per update, never change the project and
never move `sessions_valid_after` (the kill-switch cut-off) back or clear it; another keeps the
file override's document fixed. A CHECK keeps `kinds` to one to three distinct known values. Both tables are removed only by the cascade of their project or
document (whole-document erasure removes the file override; the audit row stays). The
migration also adds a covering index on `agent_call(key_id, project_id, at)` for the per-grant
budgets (built inside the migration, not concurrently; the API refuses a newer schema, so a
rollback restores the dump). The startup check counts both tables; re-run `grant-runtime`, then `check-runtime`,
after migrating. See [AI agent connectors](AI-AGENT-CONNECTORS.md).

Schema 42 adds two tables to the SELECT/INSERT history class (no UPDATE, no DELETE for the
runtime login): `device_key_class`, one immutable row per signing key enrolled with a class
(`person` or `agent`; no row means a person's key), and `agent_call`, the append-only
metadata ledger of AI-agent reads (credential, project, file, tool, revision, request digest,
byte counts, outcome; never document text). Both refuse UPDATE by trigger for every login;
`device_key_class` refuses DELETE too, and an `agent_call` row can only be deleted by the
cascade of an audited whole-document erasure (`agent_call_erasure_guard`, which checks the
same-transaction erasure receipt and the trigger depth, because the document row is already
gone when the cascade runs). The startup check counts both tables, so a missing table or a
wrong privilege fails closed. The migration also widens the scope checks of `device_session`
and `device_key_project_grant` with `content:read`. Re-run `grant-runtime`, then
`check-runtime`, after migrating. See [AI agent connectors](AI-AGENT-CONNECTORS.md).

Schema 41 adds two SELECT-only tables, `project_ref_authority_epoch` (receipt
authority epochs with their stored signed statements) and
`project_ref_authority_revocation` (immutable revocation markers). They join the
read-only allowlist beside `project_ref_authority`; only the operator login, through
`engine-cli`, appends to them, and both refuse UPDATE/DELETE/TRUNCATE by trigger for
every login. Three non-security-definer trigger functions (genesis epoch, epoch
consistency, issuer guard on `project_ref_event`) read these tables with the invoking
login's SELECT privilege, so the runtime login cannot store a receipt signed by a
non-active or revoked epoch. The startup check counts both tables; re-run
`grant-runtime`, then `check-runtime`, after migrating. See
[authority rotation](REF-AUTHORITY-ROTATION.md).

Schema 40 adds two tables to the SELECT/INSERT/UPDATE class (no DELETE):
`device_key_access`, one editable mode row per signing key (`all` or `granted`; no
row means `granted`), and `device_key_project_grant`, per-project grants whose rows
are created and revoked but never edited or deleted. A trigger allows exactly one
change to a grant row (`revoked_at` from null to a time), so a changed scope set is
a revocation plus a new row; `device_signing_key` itself stays immutable. The scope
check on `device_session` now admits `history:review`. The startup check counts
both new tables, so a missing grant or a wrong privilege fails closed. Re-run
`grant-runtime`, then `check-runtime`, after migrating. See
[device key grants](DEVICE-KEY-GRANTS-API.md).

Schema 39 adds six project issue tables, each with an explicit allowlist (see
[project issues](PROJECT-ISSUES-API.md)). `project_issue_comment` joins the
SELECT/INSERT history list and also refuses UPDATE through a trigger. A new
SELECT/INSERT/UPDATE class holds `project_issue` and `project_issue_counter`: the
runtime login edits and closes issues but never deletes them. `project_issue_label`
and `project_star` join the INSERT/DELETE class (created and removed, never edited).
`project_label` is mutable. The startup check compares all four classes and counts
the tables, so an unlisted table or a wrong privilege fails closed. Re-run
`grant-runtime`, then `check-runtime`, after migrating.

Schema 38 adds one mutable table, `device_session`: a short-lived replay ledger and
issuance-rate window for project device sign-in. It stores no bearer token, secret
or grant, is pruned by the runtime login, and joins the mutable allowlist. The
permanent issuance record is an `operation_audit` row. Re-run `grant-runtime`, then
`check-runtime`. See [project-scoped device sessions](PROJECT-SCOPED-DEVICE-SESSIONS.md).

Schema 36 adds SELECT/INSERT on published commits, root reachability indexes,
project history reviews and signed ref events. Published metadata counters and
`project_ref_authority` are SELECT-only; only the operator installs its public key.
Review sequence USAGE is explicitly granted. Immutable guards and audited document
erasure preserve the boundary. Re-provision grants with the matching binary; see
[Shared project history](SHARED-PROJECT-HISTORY.md).

Schema 35 extends the explicit runtime inventory with SELECT/INSERT on
`project_staged_object` and `project_staged_commit`, and SELECT-only on
`project_staging_storage`. Immutable staging history has audited document/account
erasure entrypoints; quota/erasure functions are not runtime-callable. See
[Private project transfers](PRIVATE-PROJECT-TRANSFERS.md). Re-provision the matching
runtime grants after migration with a schema-compatible operator binary.

11 October 2026, schema 0029. This is a source and rollout contract; current checks
and deployment state are in [the handoff](ENGINE-EVOLUTION-HANDOFF.md).

## Process and privilege contract

`engine-api` never applies migrations. Startup checks the complete migration
version/checksum list and the effective privileges of its actual login. Missing,
modified or newer migration records fail closed. It refuses owner/superuser roles,
role memberships, database/schema/object ownership, DDL/temporary-object privileges,
trigger-bypass settings and callable application SECURITY DEFINER functions. It
does not accept a privileged connection followed by SET ROLE or SESSION
AUTHORIZATION: it also compares the session identity with the connection's login.
Supplying `MIGRATION_DATABASE_URL` to the API is itself a configuration error, even in local
development. API connections pin their search path to `pg_catalog, public, pg_temp`.

`engine-cli migrate` uses **only** `MIGRATION_DATABASE_URL`, in a separate operator
process. `engine-cli grant-runtime ROLE` uses that same operator connection to grant
an explicit allowlist to an already-created, dedicated nonprivileged login. It
does not create roles, handle passwords or silently repair role ownership/membership.
Grants and their verification share one transaction. `engine-cli check-runtime`
uses `DATABASE_URL` and only reads catalogs and migration records.

Embedders constructing `engine_api::AppState`/the router directly must call
`engine_core::database::check_runtime` themselves before serving requests; the
synchronous library constructor does not perform I/O. The supplied API binary
enforces this gate before starting background workers or accepting traffic.

The runtime login can SELECT/INSERT canonical events, snapshots and verification
receipts, named versions, draft edits, shared objects/checkpoints, provenance bundles,
reviews and audit/erasure receipts. It cannot UPDATE/DELETE/TRUNCATE those tables,
disable their triggers, modify migration metadata, change schemas or elevate roles.
Mutable operational tables retain the DML needed for authentication, permissions,
drafts, derived indexes and administration. Migration metadata and the convenience
view are SELECT-only. Two identity sequences permit usage but not `setval`.
Seven read-only access helpers are executable; new tables/functions need an explicit
allowlist and schema update. Effective column and PUBLIC grants are checked as well.

The API retention loop (`crates/engine-api/src/retention.rs`) depends on the grants
that already exist: SELECT, UPDATE (for `FOR UPDATE SKIP LOCKED`) and DELETE on the mutable
tables `magic_link`, `refresh_token`, `auth_request_window`, `device_enrollment_challenge`,
`web_metric` and `idempotency_key`, and nothing on history. It needs no migration and no
grant change; moving one of these six tables out of the mutable class would stop the
loop (a failed pass is logged and counted, never fatal), and its test asserts the
privileges.

Migration 0029 revokes PUBLIC privileges on application relations/functions and
database CREATE/TEMP, plus public-schema CREATE. It also removes automatic PUBLIC
function execution from the migration owner's future defaults. Use a dedicated
application database and consistent migration owner; this is not an unrestricted
shared-schema deployment contract.

The three existing erasure trigger entrypoints execute as their migration owner,
with a pinned safe search path and no PUBLIC execution. Their existing trash and
same-transaction receipt checks still apply. Only those triggers delete immutable
children during approved whole-document/account erasure. The runtime cannot install
them on a table, call them as ordinary functions or directly delete history. No
session flag disables the append-only guards.

This follows PostgreSQL's documented [ownership and privilege boundaries](https://www.postgresql.org/docs/16/ddl-priv.html)
and [safe SECURITY DEFINER function requirements](https://www.postgresql.org/docs/16/sql-createfunction.html#SQL-CREATEFUNCTION-SECURITY).
Future-function defaults use [global and schema-specific revocation](https://www.postgresql.org/docs/16/sql-alterdefaultprivileges.html),
because a schema-level REVOKE alone cannot subtract PostgreSQL's global default.

## Local setup

The numbered task names below are for the application repository. In the standalone
engine, copy `.env.example` and `migration.env.example` to `.env` and `.env.migrate`
only if absent; use `task run` instead of `task dev`. Both repositories provide the
same migrate, grant and check tasks.

1. `task setup` preserves existing files and creates `.env` and `.env.migrate` if
   missing. The latter is ignored by Git and is loaded only by operator tasks.
   Keep runtime credentials and auth keys in `.env`; never put the migration URL
   there. Existing development databases/credentials are not changed automatically.
2. `task up`, then connect to the intended local database as its owner. Create a
   separate login, using `psql` interactively so its password is not a CLI argument:

   ```sql
   CREATE ROLE dynodoc_runtime LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
   \password dynodoc_runtime
   ```

3. Set the owner's URL in `.env.migrate` as `MIGRATION_DATABASE_URL`; set the new
   login's URL in `.env` as `DATABASE_URL`. URL-encode password characters as needed.
4. `task migrate`, then `task db:grant-runtime`. On an existing database complete
   `engine-cli verify-snapshots --all` and independent audits before serving data.
5. `task db:check`, then `task dev`. The API performs the same startup gate. `task dev`
   does not migrate or inherit `.env.migrate` from a dependency.

The standalone engine uses the same CLI contracts; its development port is
5548 instead of the application's default 5432. For a custom role name use the CLI
directly. CLI binaries read environment variables, not dotenv files themselves.
SQLx tests still require isolated test-owner credentials with CREATE DATABASE and
CREATE ROLE; never give those credentials to a running API. Test fixtures create
actual restricted logins and dispose of their grants/roles after passing tests.

## Hosted upgrade and rollback

Deploy the API and web as a compatible release. The VPS upgrade from 26 through
27 (revocable sessions), 28 (verified snapshots) and 29 (database privileges) passed
on 11 October 2026; see [the release and tested rollback](operations/2026-10-11-security-runtime-release.md).
The sequence below remains the requirement for other existing installations.

1. Take and validate a backup and rehearse on an isolated copy. Stop old API writers
   and public traffic. Preserve current data/receipt counts and verification results.
2. Create a distinct nonprivileged runtime login/password using the database operator.
   Set `RUNTIME_DATABASE_URL` in the deployment configuration. Keep ownership and the
   PostgreSQL owner's credential out of the API/web container environments.
3. Build the new API image, which includes `engine-cli`. The Compose maintenance
   service is opt-in: `docker compose --profile maintenance run --rm database-admin migrate`.
   Then run `database-admin grant-runtime dynodoc_runtime`. It never starts with
   ordinary `compose up`; there is no migration sidecar attached to the API.
4. Run `database-admin verify-snapshots --all`, independent `verify DOCUMENT_UUID`
   checks and `check-runtime`. Investigate any mismatch; never bless corrupt states
   or change migration checksums to get the API to boot.
5. Start the API using only the runtime URL. Verify authentication/logout, canonical
   writes, public/history reads, requests/merges and administration/erasure on the
   rehearsal before activating production. Test role-denied operations separately.

Pre-0029 API images are **not compatible rollback**: they migrate at boot and bypass
the credential gate; earlier images also lack snapshot/session protections. Retain
a tested build implementing this schema/role contract or keep traffic stopped for
an explicit recovery/forward-fix procedure. Reopening a schema-29 database with the
new runtime role, restored production-copy upgrade and compatible-image rollback
are verified in the linked release. Repeat these gates for future schema changes.
Do not downgrade by deleting migration records.

The current source includes schema 30–34, while production remains 29. A schema-29
binary fails the exact catalog check after this upgrade, even with all optional
shared writers disabled. Before a future rollout, rehearse a compatible schema-34
rollback build and the existing history/public-pin/session/erasure workflows.
Migration 33 adds device key records/revocations and a read-only installation UUID;
34 adds read-only storage accounting maintained by restricted trigger functions.
Run `grant-runtime` again after migration to apply this expanded allowlist, then
`check-runtime`. See [device enrollment](DEVICE-SIGNING-KEYS.md) and
[storage budgets](SHARED-STORAGE-BUDGETS.md). This source pass has not performed
production-copy rehearsal or VPS deployment.

## Trust and remaining work

This constrains a compromised runtime credential's ability to rewrite existing
history or bypass triggers. It does not make the database owner untrusted, impose
per-user SQL row policies, authenticate arbitrary new appends, or prevent abuse of
the permitted audited whole-resource-erasure path. API authorization remains
essential. Signed/witnessed heads and independent recovery remain separate work.
Startup checks cannot stop an operator from changing privileges after startup;
database administration and credential rotation remain trusted operations.

The schema-29 credential separation changed no content algorithm, event hash,
snapshot fingerprint or document API shape. The startup catalog checks are bounded
by the schema inventory; they add no per-event
queries. Shared-object accounting now bounds encoded retained bytes/object counts;
whole-service/local-store budgets, full-state memory, project-wide portable branches,
offline convergence and installed native-host acceptance remain on the staged plan.
