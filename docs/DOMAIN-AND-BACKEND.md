# Domain and backend verification

Verified on 19 September 2026 against the live VPS. This is a pilot-operation
check, not a certification of production capacity or research compliance.

## Current routing

| Address | Service |
|---|---|
| `https://dynodoc.online` | Vercel homepage; `www` redirects here. |
| `https://app.dynodoc.online/workspace` | Researcher workspace on the VPS. |
| `https://app.dynodoc.online/auth/google/callback` | Google OAuth callback. |
| `https://app.dynodoc.online/auth/verify` | Email sign-in verification. |
| Old `dynodoc.103.191.50.32.sslip.io` host | Redirect to the corresponding path on `app.dynodoc.online`. |

The owner added the registrar A record `app → 103.191.50.32`. A certificate was
issued for `app.dynodoc.online`, expiring 18 December 2026. Certbot's timer is
active; the Dynodoc-specific renewal hook now reloads Nginx for both the new and
legacy certificate lineages.
A staging renewal dry-run for the new hostname passed, including the deploy hook's
Nginx configuration check and reload.

The live private `ORIGIN` is `https://app.dynodoc.online`. Only the Dynodoc API and
web containers were recreated for this origin change. The PostgreSQL container,
data volume, converter and unrelated VPS applications were preserved.

The new host uses `infra/demo/nginx-app.conf`; the legacy host uses
`infra/demo/nginx-legacy-redirect.conf`. Both preserve HTTP certificate-challenge
paths. The new workspace access log excludes URL queries, avoiding routine
logging of OAuth codes and email tokens in that log.

## Checks performed

| Area | Evidence |
|---|---|
| Homepage routing | A real browser followed the Vercel homepage's workspace button to `https://app.dynodoc.online/login`. |
| PostgreSQL | Version 16.15, healthy; all ten repository migrations applied successfully. |
| Persistence | Named volume `dynodoc-demo_database` mounted at `/var/lib/postgresql/data`; `fsync`, `full_page_writes` and `synchronous_commit` are on. |
| Network | Only web port `127.0.0.1:3088` is published. API, PostgreSQL and converter have no public host port. |
| Saved edits | A synthetic paragraph edit was saved and read back through the authenticated HTTPS gateway. |
| Original uploads | Small valid DOCX, XLSX and CSV uploads were downloaded byte-for-byte; content was parsed and verified again after the API/web containers were recreated. |
| Legacy DOC | A synthetic binary Word file was converted through the public authenticated conversion route, and the resulting DOCX text was verified. |
| Versions | The saved checkpoint remained present after recreation; the document's event hash chain verified successfully. |
| Access | Unauthenticated document access returns 401. Verification sessions were created internally for the synthetic account; this does not test Google or Resend login. |
| Backup | A fresh database dump was restored into a newly created temporary database; table counts, the named version and original-file hashes matched. Only that temporary database was removed afterward. |
| Resources | Approximately 385 GB free disk at the time of inspection; this is not a load-test result. |

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

The current upload limit is 10 MiB per original file. The web and Nginx request
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

At inspection, Google client ID/secret and Resend key/sender were still absent.
`DEV_AUTH` remains disabled. The owner still needs to:

- Set `PUBLIC_WORKSPACE_URL=https://app.dynodoc.online` in Vercel and redeploy.
  Existing links through the old hostname work through the redirect meanwhile.
- Configure Google branding/domain settings and the new exact callback URL.
- Verify a Resend sending domain, add its DNS records and set the provider
  credentials privately on the VPS.
- Complete real Google login and email delivery tests with a pilot researcher.

See the [updated Google and email setup guide](GOOGLE-AND-EMAIL-SETUP.md) for the
exact fields and commands. No new database provisioning in Vercel is required.
