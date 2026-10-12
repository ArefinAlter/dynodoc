# Schema 38 release - 12/13 October 2026

## Deployed release

Application `49ada1e27e3aca9fb5405afbe75cd9442c613e2c` is live on the VPS workspace,
private API and admin site at PostgreSQL schema 38. Activation finished at
2026-10-12T19:18:51Z (13 October 01:18 in Asia/Dhaka). Standalone engine source is
`e5b9d94`. The public marketing site is deployed separately by Vercel from the same
`main` branch and already served the new pages before this rollout.

This release deploys, together: migrations 0030 to 0038 (typed shared checkpoints,
shared draft bases, shared named snapshots, account-bound signing keys, shared-object
budgets, private project transfers, shared project history with signed receipts,
activity read indexes, project-scoped device sessions), the rebuilt public site, the
repository-style Projects/profile/admin interfaces, the six add-in manifests renamed
for a future store listing, and the new read-only project activity APIs.

Not changed: the PostgreSQL and converter containers (IDs and start times verified),
unrelated applications on the host, the live `.env`, nginx configuration, the 3 MB
upload limit, and every optional writer. Shared periodic checkpoint writes, shared draft
bases and shared named snapshots stay off. **No receipt-signing key is installed**
(`DYNODOC_REF_SIGNING_KEY_FILE` is not set), so signed shared branch publication is
refused; everything else works. Installing the authority is a separate owner decision
(see "Next steps").

## Source and acceptance evidence

- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38294922447):
  formatting, Clippy, tests, OpenAPI consistency and committed-source packaging pass.
- [Application frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38294922456):
  build, unit tests, public browser tests and the authenticated browser workflows
  against a schema-38 stack with the production proxy header check pass.
- Standalone engine CI on `e5b9d94` passes.
- Local application Rust run: every test passes except the Windows-only
  `sse_delivers_a_new_event_within_100ms` timing assertion (385 tests); Linux CI is the
  acceptance run for that test. An isolated local authenticated browser run of the
  restyled interface passed 99 of 99 tests before migration 0038 existed.
- Axe passes on all 20 public pages; an independent adversarial review of the
  device-session code found no critical or high issue and its hardening items were applied.

The archive has 847 committed files; SHA-256
`38054db9f3f5edcaf9488a5f94b35590135700906d02ecffb005a94ace54bf86`. Every file matched
`SOURCE_MANIFEST.sha256`, and `/source` returns exactly the archive installed at
`/opt/dynodoc-demo/source.tar.gz`.

## Rehearsal on a production copy

Private artifacts are under `/opt/dynodoc-releases/20261013-schema38/`. A custom-format
dump of production (190,522 bytes, read through `pg_dump` only) was restored into a
throwaway PostgreSQL 16 container on an internal Docker network with the live restricted
login name and production's database-level ACL. With the candidate image: operator
migration 29 to 38 took 0.8 to 1.6 s; `grant-runtime` for the live role, snapshot
verification, per-document chain verification (two documents) and `check-runtime`
passed; the aggregate row counts and event/snapshot digests were identical before and
after (36 events, zero snapshots, nine identities). The candidate API started with the
restricted login, returned 401 for unauthenticated project/profile/admin routes and
mounted the device-session routes; the candidate web image served the public pages and
`security.txt`. All rehearsal containers and the network were removed.

Two rehearsal-harness issues were found and fixed before the real run, neither a
product defect: the dump carries GRANTs to the live runtime role (the role must exist
before restore), and database-level ACLs are not part of a dump (production revokes
PUBLIC temporary-object access; the copy must mirror that or `grant-runtime` refuses).

## Activation

1. Preflight: candidate source manifest, image build, rehearsal report and schema 29
   fingerprint asserted; current images tagged `before-schema38-20261013`.
2. Both nginx sites switched to maintenance responses; web, admin and API stopped.
3. Fresh verified backup: `/opt/dynodoc-demo/backups/dynodoc-before-schema38-20261012T191821Z.dump`,
   190,522 bytes, SHA-256 `a7683868fcc3fc9ddc0af266dec6ea0c3cc9a2dbd4ce58a0c8463b6dc83c64e8`
   (same-VPS backup; off-site recovery remains owner-deferred).
4. Operator migration, `grant-runtime dynodoc_runtime_20261011`, `verify-snapshots --all`,
   per-document `verify`, `check-runtime`; fingerprint equal to the preflight value with
   schema 38.
5. API/web/admin recreated from the pinned `release-49ada1e` images; the API environment
   was checked to contain no migration URL, no shared-writer flag and no signing key.
6. nginx restored last; 22 live checks passed (health, login, HSTS and CSP on pages,
   static files and errors, anonymous redirects, three Office panes with scoped
   embedding, the renamed manifest, the 64 px icon, `security.txt`, source archive).
   Visitors received maintenance responses for roughly one to two minutes.

No real user sign-in, email or document mutation was used.

## Rollback

Code rollback to a schema-29 image is not possible after this migration: those images
validate the complete migration list at start and must never be pointed at schema 38. The
tested recovery is to restore the verified pre-migration dump into a new database, point
the previous `before-schema38-20261013` images at it, and accept the data written after
the backup (the database held two documents and 36 events at the time). Migrations are
append-only and are not reversed in place, so a forward fix is the preferred response to
any defect. Keep
`/opt/dynodoc-releases/20261013-schema38/` and the backup.

## Next steps

- Decide receipt-authority key custody, install it with `engine-cli install-ref-authority`
  and mount it with `infra/demo/compose.schema38.override.yml`, then rehearse the shared
  writer path before enabling it.
- Per-project key grants, operating-system key storage and a review-only device scope.
- Packing, retention and same-file offline convergence remain open (a retention-plan
  command and a convergence report were started and set aside; see the status document).
- Marketplace publication, installed-editor acceptance and the legal operator fields
  are owner actions.
