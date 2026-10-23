# Schema 47 release - 23 October 2026

## Deployed release

Application `3fa06fd79020f728f91b34c40945d77615376f53` is live on the VPS workspace, private API and
admin site at PostgreSQL schema 47 (previous release: `f62d62c`, schema 41, 14 October 2026).
Activation ran at about 05:35 UTC (11:35 in Asia/Dhaka) and the site was closed (503) for roughly
twenty to thirty seconds. The public marketing site is deployed separately by Vercel on every push to
`main`, so it was already running this web build before the VPS release; this release brought the
workspace host, the admin host and the API up to the same code.

This release adds migrations 0042 to 0047 (all additive; 0044 to 0047 were amended before this release
ran, which is allowed because no shared database had applied them):

| Migration | What it adds |
|---|---|
| 0042 | agent read path: agent identity class, read scope, call ledger |
| 0043 | per-project agent policy, kill switch, `sessions_valid_after` |
| 0044 | agent write path: proposals, receipts, agent drafts |
| 0045 | review revision binding (approvals name the revision seen), sequence column |
| 0046 | native-app sign-in hand-over codes (`app_handover_code`) |
| 0047 | operator-issued sign-in links (`operator_sign_in_link`; the runtime login cannot insert) |

Everything that is new for users is dormant or additive on this installation:

- **Agent gateway: off.** `AGENT_GATEWAY` is not set, so the agent data plane is not mounted
  (`/projects/{id}/agent/files` answers 404). No agent key exists. The AI-proposals inbox and the
  agent badge appear only when an agent has proposed something.
- **Retention jobs: off.** The hosted compose file now passes `RETENTION_JOBS=off` (the API's own
  default is on). The jobs delete expired sign-in rows and page metrics older than 30 days; they stay
  off until the owner has looked at what a first pass would remove (`RETENTION_JOBS=on` in `.env`).
- **Approvals bound to the revision seen.** A review now names the draft revision; reviews written
  before this release keep the old timestamp rule. The web app is the only caller and sends the
  revision.
- **Native-app sign-in** (`/app-signin`, `/app-signin/finish`, `POST /auth/app-handover*`) is live but
  only the desktop and Android apps use it, and none is distributed. The routes need the web service
  key or a signed-in browser session; the confirmation is a real form post.
- **Operator-issued sign-in links** exist only as an `engine-cli` command that needs the owner
  database credential; no HTTP route issues them.
- **Installable web app:** manifest, offline page and a service worker that stores only the offline
  page, two icons and public script, style and font files (never pages, documents or account data).
- **nginx:** the `app.dynodoc.online` server block was replaced with the repository version (the live
  file was byte-identical to the previous release's file first): a CSP entry for the service worker and
  offline page, `X-Frame-Options: DENY` for the two sign-in pages and a POST-only rate limit of 60 per
  minute on `/app-signin`. It was tested with `nginx -t`, reloaded, checked, and would have been
  reverted automatically on any failure.

Not changed: the PostgreSQL and converter containers (same ids and start times), unrelated
applications on the host, the live `.env`, the receipt-signing key (still not installed: see
[RECEIPT-AUTHORITY-DECISION.md](../RECEIPT-AUTHORITY-DECISION.md)), every optional writer. The
per-client rate limit of the sign-in hand-over is off because `ADDRESS_HEADER` and `XFF_DEPTH` are not
set (the per-account and per-code limits apply).

## Evidence

- Source: 1,594 committed files, archive SHA-256
  `000321191d8014985955b9cd8b03ea8c4f7f9480aee7230d78cd4bc2551a6756`, every file matched
  `SOURCE_MANIFEST.sha256` on the server. The GitHub Actions of the private repository could not run
  (account billing) for this revision or any since 20 October; the substitutes were local gates on the
  merged tree (fmt, clippy `-D warnings`, rustdoc `-D warnings`, OpenAPI drift, full workspace tests
  with only the known Windows-only SSE timing failure, frontend prettier/eslint/svelte-check/vitest/
  build, isolated Chromium suites) and green CI of the mirrored crates in the public engine repository
  (engine-ci, platform matrix and container build, run 39898048050 and its siblings).
- Rehearsal on a production copy (schema 41 dump of 325,990 bytes into a throwaway PostgreSQL 16 on an
  internal network): migrations 0.7 s, `grant-runtime` for `dynodoc_runtime_20261011`, per-document
  verification and `check-runtime` passed, event and snapshot digests identical before and after
  (2 documents, 38 events, 1 snapshot, 9 identities), candidate API started on the restricted login
  with the PRODUCTION secrets and passed the new start-up validation, candidate web (workspace and
  admin variants) started with the production web secrets, agent routes absent, hand-over routes
  refuse a caller without the service key, retention reported off. Two defects in the rehearsal script
  itself (a shadowed function name; a check that sent an empty body to a route that parses the body
  first) were found and fixed in the script, not in the product.
- Activation: verified backup
  `/opt/dynodoc-demo/backups/dynodoc-before-schema47-20261023T053529Z.dump` (325,990 bytes, SHA-256
  `18d2f8980531b62c4fdfe7547b301b8844883f27d326617240998cce6545d896`); migration, grants and
  verification passed; fingerprints before and after identical apart from the schema number; the API
  environment has no migration URL, shared-writer flag, signing key or agent gateway; 28 live checks
  passed (including the new files, `/app-signin` and the absent `/connectors/addins`); the PostgreSQL
  and converter containers and every unrelated container were unchanged.
- External sweep after activation (from a developer PC): `/healthz`, `/login`, `/service-worker.js`,
  `/offline.html`, `/site.webmanifest` answer 200 on the app and admin hosts; `/projects` redirects a
  signed-out visitor to `/login`; `/app-signin` carries `Cache-Control: private, no-store`,
  `Cross-Origin-Opener-Policy: same-origin` and `X-Frame-Options: DENY`; `/connectors/addins` is 404;
  a scripted `POST /auth/app-signin` is refused (403).

## Not verified

No signed-in session was exercised on the live site by the release process (no account credential is
held). The owner should sign in once and open a document. Real-device behaviour of the installable web
app, the Safari/iOS behaviour and the desktop and Android apps are unchanged by this release and
remain as described in the ledger.

## Rollback

As for earlier schema releases: older images must not be pointed at schema 47. Restore the verified
dump above into a new database with the `before-schema47-3fa06fd` images, accepting data written
since; a forward fix is preferred. Private artifacts are in `/opt/dynodoc-releases/20261023-schema47/`.
