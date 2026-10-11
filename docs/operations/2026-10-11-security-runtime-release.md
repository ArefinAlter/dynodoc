# Security and runtime-boundary rollout - 11 October 2026

## Deployed release

Application `858801c3e589b903faccb8a6aa62367020aa6bbb` is live on the VPS workspace,
private API and admin site at schema 29. Activation and live verification completed
at 2026-10-11T06:54:36.148896+00:00. Engine implementation is `80cd148` (security changes in
`f718c11`); application security code is `812f97a`, with the hydration fixture fix
`e22a103` and Office command-frame compatibility fix `858801c`.

This activates the accumulated authentication/session, verified-snapshot,
runtime-database-role, project-policy/metadata and browser/workload protections.
Projects remains the front page, profile visibility stays within shared project
membership and original uploads retain the 3 MB pilot limit. Shared periodic
checkpoint writes remain disabled. Event operations and v1 hashes are unchanged.

## Source and acceptance evidence

- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38116813490):
  275 tests, lint/format/OpenAPI and committed-source packaging checks pass. Rust,
  migrations, lockfile and toolchain inputs are identical at the deployed revision.
- [Standalone engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38116978805):
  275 tests and its independent checks pass. The standalone Rust 1.96 lint convention
  is intentionally preserved separately from the application's Rust 1.82 baseline.
- [Final frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38118877490):
  lint/type/build/connector drift, 289 units, 19 public and 72 signed-in browser
  workflows pass, including the production Nginx header regression.
- Local browser CSP/command-frame checks pass. Full Windows runs intermittently
  missed the unchanged 100 ms SSE deadline and the spreadsheet accessibility
  fixture's 60-second budget; focused SSE and full Linux acceptance pass. Neither
  threshold was loosened. Standalone rustdoc passes with 20 existing warnings.

The final archive has 632 committed source files; SHA-256 590ce886d751405521131b284652bc3a252fa109ef7601784fa0799c4ade0cf3.
All candidate/deployed files match the manifest and `/source` returns exactly that
archive. Untracked server files and `.env` files are excluded. Subsequent
documentation commits do not change these verified runtime images or the archive.

## Upgrade and rollback rehearsal

Private artifacts are retained under `/opt/dynodoc-releases/20261011-security/`.
A validated 153,141-byte production backup was restored into an isolated, internal
Docker network. Separate operator CLI migration to 29, snapshot verification,
per-document chain verification and restricted-runtime validation preserved the
original two documents, 36 events and zero snapshots.

Both the candidate and compatible rollback API pass synthetic login, canonical
writes, snapshots/versions, project history recovery, request/merge, chain audit,
audited erasure and server-side logout revocation on the upgraded copy. Actual
runtime credentials are denied history deletion/truncation, snapshot updates,
trigger disabling and table creation. Candidate web sessions, Projects/Workspace/
Profile SSR, scoped panes, command-frame artifacts and source delivery pass with
both APIs. Staged Nginx configurations pass using the live site's certificate paths.
No production email or document mutation was used for acceptance.

## Activation and live checks

Fresh backup before migration: /opt/dynodoc-demo/backups/dynodoc-before-security-20261011T065213Z.dump, 153,141 bytes.
`pg_restore --list` passed; SHA-256 f47d8172c528e59a6cff0aab9f66a31336c99b71b5612ce4ae117f9cfcb9d124. These are same-VPS backups;
off-site recovery remains owner-deferred.

Public traffic briefly received maintenance responses while old API/web/admin
writers stopped. Migrations 27–29 were applied with separate operator credentials;
the API now uses restricted login `dynodoc_runtime_20261011`, validates startup
privileges and receives no migration/owner URL. Snapshot/chain checks and aggregate
row fingerprints pass unchanged. API/web/admin alone were recreated from pinned
`release-858801c` images. PostgreSQL and converter IDs, images and start times were
preserved. Unrelated containers were preserved. Nginx activation happened last.

The first activation probe ran before Nginx's replacement workers had taken over
and still received maintenance 503. A second checker assertion incorrectly treated
the Location header as case-sensitive. Traffic stayed closed while both harness
issues were corrected; a narrow proxy-only resume waited for public health and
normalized header names. No migration was repeated or database rollback performed.
Both original and corrected scripts/logs are retained in the private release record.

Live checks pass for internal API and direct/public web/admin health, anonymous
sign-in redirects, CSP/HSTS on pages/static/errors, scoped embedding for three Office
panes and the manifest command frame, 24 connector delivery/auth
checks, and exact source bytes. Synthetic encrypted cookies containing invalid API
credentials exercise Projects/Workspace/Profile SSR through live Nginx without
accessing an account; pages return 200 and protected API data still returns 401.
This verifies the large-header regression boundary without logging in as a real user.
A clean Chromium session also passes the Projects sign-in redirect, hydrated login
form and six installer links with no page errors or CSP violations.

All task-owned local test servers/database and labelled VPS rehearsal containers,
their disposable PostgreSQL volume and internal network have been removed.
Private backup/rehearsal evidence and compatible images remain. No installed-host,
marketplace, real-provider sign-in or real-email-delivery acceptance is implied.

## Compatible code rollback

Keep the candidate web/proxy and use the rehearsed schema-29-compatible API built
from application `336b718`. It preserves the preceding authentication, snapshot,
runtime-role and project-policy guarantees; it predates the new API workload limits.

```sh
cd /opt/dynodoc-demo
docker compose --project-name dynodoc-demo --env-file infra/demo/.env \
  -f infra/demo/compose.yml \
  -f /opt/dynodoc-releases/20261011-security/release.compose.yml \
  -f /opt/dynodoc-releases/20261011-security/rollback-api.compose.yml \
  up -d --no-deps --no-build --pull never api
```

This is a tested recovery procedure, not a rollback performed on production. Recheck
health, runtime role and revocation; record the mixed-source state if used and serve
the retained matching API source as required. Do not reactivate schema-26 API images,
delete migration records or restore an old database merely to roll back code.
Normal restarts use the first two configuration files above and omit the rollback
override. Preserve current schema, credentials and post-release data.

## Next concrete work

Resume evolution part 2 with versioned, bounded text/value/asset chunks and decode/
reference budgets, compatibility vectors and a large-value/reuse corpus. Then
measure packing/incremental publication and replace duplicated draft/version bases
with checked shared references before enabling the periodic writer.

Next dependencies are durable local repositories and portable project-wide commit
manifests/parents/atomic branch refs, resumable missing-object exchange, and offline
convergence using read/write/validation independence. Independent signed/witnessed
heads, full adapter fidelity and 100–500 MB/multi-tenant acceptance remain separate
gates. The new admission limits do not bound all resident document memory, index
search, establish fleet-wide fairness or prove capacity for hundreds of thousands
of users. Installed Word/Docs, Excel/Sheets and PowerPoint/Slides acceptance and
publisher setup remain open; current drafts and main selection are per file.
