# Domain and backend verification

Verified on 19 September 2026 against the live VPS. This is a pilot-operation
check, not a certification of production capacity or research compliance.

## Current routing

| Address | Service |
|---|---|
| `https://dynodoc.online` | Vercel homepage; `www` redirects here. |
| `https://app.dynodoc.online/workspace` | Researcher workspace on the VPS. |
| `https://app.dynodoc.online/auth/google/callback` | Workspace Google OAuth callback. |
| `https://admin.dynodoc.online` | Allowlisted operations dashboard on the VPS. |
| `https://admin.dynodoc.online/auth/google/callback` | Administrator Google OAuth callback. |
| `https://app.dynodoc.online/auth/verify` | Email sign-in verification. |
| Old `dynodoc.103.191.50.32.sslip.io` host | Redirect to the corresponding path on `app.dynodoc.online`. |

The registrar A records `app` and `admin` both point to `103.191.50.32`.
Certificates for both hosts expire on 18 December 2026. Certbot's timer is
active; the Dynodoc-specific renewal hook now reloads Nginx for the workspace, admin and
legacy certificate lineages.
Staging renewal dry-runs for the workspace and admin hosts passed, including the deploy hook's
Nginx configuration check and reload.

The live private `ORIGIN` is `https://app.dynodoc.online`. The Dynodoc API and
web containers were recreated for this release, and the separate admin container was added. The PostgreSQL container,
data volume, converter and unrelated VPS applications were preserved.

The new host uses `infra/demo/nginx-app.conf`; the legacy host uses
`infra/demo/nginx-legacy-redirect.conf`. Both preserve HTTP certificate-challenge
paths. The new workspace access log excludes URL queries, avoiding routine
logging of OAuth codes and email tokens in that log.

## Checks performed

| Area | Evidence |
|---|---|
| Homepage routing | A real browser followed the Vercel homepage's workspace button to `https://app.dynodoc.online/login`. |
| PostgreSQL | Version 16.15, healthy; all eleven repository migrations applied successfully. |
| Persistence | Named volume `dynodoc-demo_database` mounted at `/var/lib/postgresql/data`; `fsync`, `full_page_writes` and `synchronous_commit` are on. |
| Network | Workspace `127.0.0.1:3088` and admin `127.0.0.1:3089` are loopback-only. API, PostgreSQL and converter have no public host port. |
| Saved edits | A synthetic paragraph edit was saved and read back through the authenticated HTTPS gateway. |
| Original uploads | Small valid DOCX, XLSX and CSV uploads were downloaded byte-for-byte; content was parsed and verified again after the API/web containers were recreated. |
| Legacy DOC | A synthetic binary Word file was converted through the public authenticated conversion route, and the resulting DOCX text was verified. |
| Versions | The saved checkpoint remained present after recreation; the document's event hash chain verified successfully. |
| Access | Unauthenticated document access returns 401. Verification sessions were created internally for the synthetic account; this does not test Google or Resend login. |
| Backup | A fresh database dump was restored into a newly created temporary database; table counts, the named version and original-file hashes matched. Only that temporary database was removed afterward. |
| Resources | Approximately 383 GB free disk at the time of inspection; this is not a load-test result. |

The test uses synthetic content owned by `deployment-check@dynodoc.invalid`.
Verification documents are archived, with their append-only history retained.
No participant materials were used or modified.

## Where data lives

Research content, events, drafts, versions, memberships and account records live
in PostgreSQL. Original file bytes live in `document_upload.content` in the same
database; they are included in the database backups. Vercel is the public
homepage host, not the persistence layer. Web/API container replacement does not
replace the named database volume. Do not remove the volume or run a live
migration reset.

The current upload limit is 3 MB (3,000,000 bytes) per original file. The web and Nginx request
limits are 16 MiB to allow JSON/base64 overhead. The spreadsheet importer has
additional row, column and format-fidelity limits; see
[research compatibility](research-compatibility.md).

## Backups and remaining setup

`/etc/cron.d/dynodoc-demo-backup` schedules a daily backup at 02:17 **server local
time**. The VPS time zone is `Europe/London`, so its UTC offset changes with
British Summer Time. Cron is active. The script validates dump structure and
writes checksums; dated database backups are retained locally for roughly 14
days. The scheduled first overnight run was not observed during this session;
the script and a real restore were exercised directly.

Backups are currently on the same VPS. Off-server backup, automatic failure
alerts, recovery objectives and study-specific acceptance/load testing remain
open. The verified paths support the small pilot; they do not establish a
production service-level guarantee.

Google and Resend credentials are now installed privately. The sender is
`Dynodoc <signin@dynodoc.online>`. Both public `/auth/options` endpoints return
Google/email enabled and development disabled. Google authorization redirects use
S256 PKCE and their correct host-specific callbacks. The owner confirmed that
Google is published for external production sign-in. Actual provider login and mail
delivery still require the owner's browser acceptance; the send-only Resend key
cannot inspect domain verification through the domains API.

The owner administrator allowlist was verified. Unauthenticated admin requests
redirect to sign-in; the synthetic non-admin account received 403 on both hosts.
Anonymous document requests returned 401. The minute-by-minute host collector
reports all five Dynodoc services running, private PostgreSQL healthy, a fresh
local backup with valid checksum, and disk space. It exposes no Docker socket.

After this release, the existing synthetic document, stored CSV/DOCX/XLSX bytes,
named version, legacy DOC conversion and event-chain check all passed again.
All unrelated container IDs were unchanged across deployment. The PostgreSQL and
converter containers were preserved.

The owner still needs to:

- Keep `PUBLIC_WORKSPACE_URL=https://app.dynodoc.online` in Vercel. The Git
  integration has now deployed this release automatically; the new SVG and sitemap
  were verified on the public domain.
- Confirm actual Google login and email-link delivery; check Resend domain
  verification and DNS in the provider dashboard if delivery fails.
- Add the Search Console domain TXT verification and submit the sitemap after
  the homepage release. See [search/admin setup](SEARCH-AND-ADMIN-SETUP.md).
- Complete study-specific acceptance and off-server backup arrangements.

See [Google and email setup](GOOGLE-AND-EMAIL-SETUP.md) for the exact settings.
No new database provisioning in Vercel is required.
