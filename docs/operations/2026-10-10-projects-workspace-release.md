# Projects workspace rollout - 10 October 2026

## Deployed release

Application `ecbc643` (implementation `6f1ac21`) is live on schema 26, activated
at 21:22 Asia/Dhaka (15:22 UTC). Matching standalone engine source is `8eef6dd`,
with documentation through `ce56803`. Projects is now the default workspace;
profiles are private to shared Project members and originals have a 3 MB cap.
The previous release was application `536a448`, schema 25.

## Preparation and rehearsal

The restricted release directory is `/opt/dynodoc-releases/20261010-ecbc643/`.
It retains the previous source, container inventory, Nginx checksums, candidate
archive, build logs, a schema-compatible rollback context and reviewed scripts.
All 601 packaged source files passed manifest verification. Archive SHA-256:
`89911ace10e76b86bfc73dbdab1e5089beeeb8dd0c337ebc86722443aba68f73`.
The deployed source matches its own manifest; all 22 applied migration checksums
match the candidate. Initial aggregate inventory: 2 documents, 36 events, 0 uploads.

Previous API/web/admin/converter images are retained under
`dynodoc-demo-{service}:before-projects-20261010`. Candidate images use
`:release-ecbc643`. The rollback API is `:rollback-schema26-20261010`: previous
source plus migration 0026 and the tested provider-name/account-erasure changes
needed to preserve the new profile fields' lifecycle guarantees. The unmodified
schema-25 API cannot run on schema 26 because its embedded migration list ends at 25.

All candidate and rollback images built successfully outside the live source
directory. The isolated rehearsal used synthetic credentials and a disposable
PostgreSQL container on an internal network, without production data. It verified:

- Actual previous API starts at schema 25 and issues a usable synthetic session.
- Candidate upgrades to schema 26, retains that session, omits email from profiles,
  saves a profile, rejects stale revision with 409 and anonymous reads with 401.
- Compatible rollback starts on schema 26, preserves a manually chosen name/bio
  through provider sign-in and clears a second synthetic account's bio on erasure.
- Candidate starts again after rollback with the retained profile unchanged.
- Candidate converter rejects 3,000,001 declared bytes with 413 before reading the
  request body, and an invalid key with 403.

All labelled rehearsal containers/network and temporary credential files were
removed. No rollback was needed in production.

## Backup and activation

Fresh backup: `/opt/dynodoc-demo/backups/dynodoc-20261010T152220Z.dump`, 151,770 bytes.
`pg_restore --list` passed. SHA-256:
`c0bf48230e01a08a810a9ab10cf08e46e12c52c412c780d4c7956ede3e0068b9`.
A restricted copy is retained in the release directory with `backup.json`.

Only `api`, `web`, `admin` and `converter` were recreated using the verified image
override and `up -d --no-deps --no-build --pull never`. PostgreSQL and all 18
unrelated containers retain their IDs, image IDs and start times. Nginx config
checksums are unchanged; it was not reloaded. Source manifest verification passes.
Aggregate counts remain 2 documents, 36 events and 0 uploads; the aggregate event
fingerprint is identical. This is preservation evidence, not an independent audit.
Shared checkpoint writes remain disabled.

| Service | Release image ID |
| --- | --- |
| API | `94625a18340b7ecf6aca58df358c9e02b43e79dbaeb32ae6da7a9de3b501b5b1` |
| Web/admin | `daad1c3a168a97ccc27fc1a0a5a145d0d57f336bc883f88f80877b3470eaa98a` |
| Converter | `d13a11fa20d4f5b35cfaaecffec69b1ab2aacae20c7ec7cd28e10c394a253787` |

## Live verification

Internal API and direct/public HTTPS web/admin health checks return 200. The 17
workspace checks in `workspace-smoke.json` pass: synthetic page shells for Editors,
Projects, own/member profiles, Project and file routes return 200 directly and
through live Nginx with identical preload links. Largest recorded direct header
set is 6,577 bytes. Anonymous pages redirect to sign-in; profile API/gateway reads
return 401. Sign-in defaults to Projects, development auth stays off, deployed
OpenAPI includes profiles, and the live converter passes both rejection checks.

All 22 existing connector delivery/auth checks pass, including exact package/icon
bytes for all six editors. A clean Chromium browser follows Projects/profile to
sign-in, sees six editor downloads, confirms the Projects destination and has no
page errors. No real account sign-in, email, document mutation or native-host
acceptance was performed. The synthetic page-shell cookie contains invalid engine
tokens and grants no document access. Vercel reports success for `ecbc643`.

Authenticated application evidence remains the source CI: 233 Rust tests per
repository, 282 frontend unit tests, 16 public and 67 signed-in browser tests.
See [the source release record](../PROJECTS-WORKSPACE-RELEASE.md) for CI links.
Standalone documentation head `ce56803` also passed
[engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38062466747).
Later documentation-only commits do not change the deployed source archive/images.

## Compatible code rollback

The reviewed server override selects the rehearsed schema-26-compatible previous
API and retained previous web/admin/converter images:

```sh
cd /opt/dynodoc-demo
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml \
  -f /opt/dynodoc-releases/20261010-ecbc643/rollback.compose.yml \
  up -d --no-deps --no-build --pull never api web admin converter
```

This is a recovery procedure, not an action taken during activation. Recheck
health and restore the exact previous source archive/manifest if used, removing
only candidate-added source files under the application directory. The guarded
fallback in the retained `activate.py` shows that bounded source restoration.
Preserve schema 26 and post-release data. Do not use the unmodified old API, delete
the migration record or restore an older database just to roll back code.

## Next priorities

Complete installed-host acceptance for Word/Docs, Excel/Sheets and PowerPoint/Slides
using test accounts and files. Their packages are installable development sources;
this release does not certify native fidelity or marketplace publication.
Resume part 2 bounded values/assets, compression/incremental publication and shared
draft/version references, followed by part 3 portable commits/parents, project-wide
branches and resumable exchange. Current main/draft selection remains per file.
Offline convergence, independent audit and large-service capacity gates remain open.
