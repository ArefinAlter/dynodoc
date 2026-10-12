# Database runtime boundary

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
