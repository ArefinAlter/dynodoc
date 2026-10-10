# History and connector release — 10 October 2026

## Deployed release

Application source is `536a44874fa163939f7500b9ac388d64260acda0`
(final implementation `0478bf2`). The matching standalone engine source is
`a44b8ad`, with documentation through `7bfd16c`.
API/web/admin were activated on the VPS at 13:36 UTC. Production is now schema 25.
Delivery, anonymous authentication, synthetic page-shell proxy checks and a clean
browser check passed. Installed Microsoft/Google host acceptance is separate.

## Preparation evidence

- Previous live API/web/admin: `8c7461d`, schema 24, with the documented Nginx
  header-buffer repair. Both live site configs match the current templates.
- All 21 applied migration SHA-384 checksums match the candidate. Migration 0025
  adds derived checkpoint tables/guards; no existing migration or event is rewritten.
- The reviewed source archive contains 580 manifest-verified files, excluding
  private environment files, keys, dependencies and build output. SHA-256:
  `d0a594b285f5d1ff002008472f63f71c9434188242372b59cbad70755fa20f12`.
- Backup: `/opt/dynodoc-demo/backups/dynodoc-20261010T132727Z.dump`, 144,068 bytes.
  `pg_restore --list` and checksum validation passed. SHA-256:
  `81570adbbcaec948635774d26faea086cfeb901230cea7afb78baf716050a319`.
- Before-release aggregate inventory: 2 documents, 36 canonical events, 0 uploads.
  Only counts and an aggregate event-row fingerprint were recorded; no document
  contents or account credentials appear in this record.
- Previous images retained as `dynodoc-demo-{api,web,admin}:before-history-pairing-20261010`.
  Previous source, site configs, backup copy and container inventory are retained
  under `/opt/dynodoc-releases/20261010-536a448/` with restricted access.
- API/web/admin candidate builds passed. A separate previous-source API build
  includes the unchanged migration 0025 for rollback compatibility.
- An isolated PostgreSQL container with synthetic credentials and no published
  ports started the actual previous API at schema 24, then the candidate at schema
  25. The unmodified previous API correctly failed on schema 25 because its embedded
  migration list stops at 24. No production data was used in this rehearsal.
- The compatible rollback API also started successfully against that schema-25
  database. Both labelled rehearsal containers and their internal network were
  removed, along with their temporary synthetic credentials.

## Activation and preservation

Only `api`, `web` and `admin` were recreated, using `up -d --no-deps --no-build`.
The PostgreSQL and converter container IDs, image IDs and start times are unchanged;
the same three values are unchanged for all 18 unrelated running containers.
Nginx was not edited or reloaded. Both app/admin HTTPS health checks and the
internal API health check returned 200. Development authentication remains disabled.

After migration, the aggregate counts remain 2 documents, 36 events and 0 uploads;
the aggregate event-row fingerprint is unchanged. This is preservation evidence,
not a new independent chain audit. Periodic shared writes are still disabled.

Release images are retained as `dynodoc-demo-{api,web,admin}:release-536a448`:

| Service | Image ID |
| --- | --- |
| API | `857425374b1cad7c5487a762a2acd4964a68e0ef841a9cb615dbb2e9af84b037` |
| Web | `86b7826574f4dbdd106706d0d318e35bf378832a3421513d6af3de24168883b9` |
| Admin | `b841628d39c5f0f3401fa2520eb541547663d1d97afb9fcfbd829340d597377f` |

The separately hosted homepage's Vercel status was successful for `536a448`.
The VPS runtime/source archive remains at that build; subsequent documentation and
release-check commits do not require rebuilding application images.

## Delivery check

`python infra/demo/check-connectors.py https://app.dynodoc.online` is a repeatable,
read-only check of package bytes, pane headers, sign-in redirects and anonymous
authentication boundaries. Before activation it correctly failed on the old
server's missing `/connectors` page (404). After activation all 22 checks passed,
including byte equality for all three Office manifests, three Google ZIPs, three
icons and the command page. The gateway rejected both missing and invalid keys.
Approval redirects preserve the local return URL through normal sign-in.

A clean Chromium run found all six download links and followed approval to login
with no page errors. Synthetic encrypted web sessions containing invalid engine
tokens exercised `/workspace` and `/projects` page shells: direct and public HTTPS
both returned 200 with identical preload links (4,079 and 2,884 bytes respectively).
This checks the previously failing Nginx path without entering a production account.
The deployed OpenAPI exposes both connection completion and history recovery.

These checks do not sign in, send email, access documents or establish installed-host
compatibility. Existing source CI remains the authenticated workflow evidence:
[Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38054623731),
[frontend/browser](https://github.com/ArefinAlter/dynodoc/actions/runs/38054623648),
[standalone source](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38054667212).
The standalone documentation head `7bfd16c` also passed
[engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38055342326).

## Code rollback

The rehearsed rollback API is `dynodoc-demo-api:rollback-schema25-20261010`.
It uses previous source plus the unchanged migration 0025; the previous web/admin
images are retained unchanged. The server's reviewed override file selects them:

```sh
cd /opt/dynodoc-demo
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml \
  -f /opt/dynodoc-releases/20261010-536a448/rollback.compose.yml \
  up -d --no-deps --no-build --pull never api web admin
```

This command is a recovery procedure, not an action taken during rollout. Recheck
health and restore the matching previous source archive if used. Schema 25 and
post-release data remain in place. Do not use the unmodified old API, remove the
migration record or restore an old database merely to roll back application code.
Future event/codec changes or enabling new writers require another compatibility
review; this rehearsal is specific to the current release.

## Compatibility and remaining work

Periodic shared checkpoint writes remain disabled by default. Deploying the
schema and mixed readers does not opt the writer in. Full JSON named-version/draft
bases, full-state materialization, bounded large values and portable commits remain
the next engine work under the existing staged plan.

No store publication or actual native-host acceptance is implied by deployment.
Word/Docs, Excel/Sheets and PowerPoint/Slides still need the installed-host matrix,
including consent, browser launch, two users, capture/push/review/pull and revocation.
Real production email/Google login and native historical file fidelity also remain
user acceptance checks. No production invitation was sent for this release.
