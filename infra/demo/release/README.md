# Release rehearsal and activation (production VPS)

These two scripts are what the 12 and 13 October 2026 production releases used (records in
`docs/operations/`). They run ON the VPS as root and assume the compose project
`dynodoc-demo` in `/opt/dynodoc-demo`.

| Script | Purpose |
|---|---|
| `rehearse.py` | Dumps production read-only, restores it into a throwaway PostgreSQL on an internal Docker network, runs the candidate image's operator migration, `grant-runtime` for the live runtime role, per-document verification and `check-runtime`, compares event and snapshot digests before and after, starts the candidate API and web image and probes them. Exit code 0 only if everything passed. Writes `rehearsal-report.json`. |
| `activate.py` | Preflight (manifest, build status, rehearsal report, schema fingerprint), nginx maintenance, stop writers, fresh verified backup, source copy, migration + grants + verification, start pinned images, restore nginx, live checks. Restores the previous release if anything fails before the migration starts; keeps traffic closed if it fails afterwards. |

## Procedure

1. CI green on a committed revision; `python3 infra/demo/package-source.py DEST REV`.
2. Upload to `RELDIR/source-REV.tar.gz`, extract to `RELDIR/candidate`, run
   `sha256sum -c SOURCE_MANIFEST.sha256`, build `dynodoc-demo-{api,web}:release-REV`.
3. `RELDIR=… TO_SCHEMA=N TAG=… API_IMAGE=… WEB_IMAGE=… python3 rehearse.py`
4. `REV=… RELDIR=… FROM_SCHEMA=N-1 TO_SCHEMA=N python3 activate.py` (detach it with `setsid nohup`
   and poll `RELDIR/activation-status`).

The rehearsal must mirror production's database-level ACL (a dump does not carry it) and create the
live runtime role before restoring; `rehearse.py` does both. Rollback is a restore of the verified
pre-migration dump; see the release records. Do not run these against anything that is not the
production VPS layout without reading them first.
