# Schema 39 release - 13 October 2026

## Deployed release

Application `1b469a20b5939fc4eb817d2495813f672148bbdd` is live on the VPS workspace,
private API and admin site at PostgreSQL schema 39. Activation finished at about
2026-10-13T03:02:27Z (09:02 in Asia/Dhaka). Standalone engine source is `676d143`.
The public marketing site is deployed separately by Vercel from the same `main` branch.

This release adds migration 0039 (project issues, labels, comments, stars), the Issues
tab, star button and labels manager, the operator and legal-page updates for Verne
Systems, the removal of the public application-source download (`/source` is a 302 to the
public engine repository), the proxy `PUT` method, and the company contact in
`security.txt`. The compose file no longer mounts a source archive. The local repository
retention plan and convergence report are command-line features that ship with the CLI
and need no server change.

Not changed: the PostgreSQL and converter containers (unchanged IDs and start times),
unrelated applications on the host, the live `.env`, nginx configuration, and every optional
writer. No receipt-signing key is installed; see
[RECEIPT-AUTHORITY-DECISION.md](../RECEIPT-AUTHORITY-DECISION.md).

## Evidence

- Application Rust CI and frontend CI pass on `1b469a2` (one browser job was rerun once
  after a pre-existing editor-hydration race in the rich-text test, and passed; the test now
  waits for the editor to become editable). Engine CI passes on `676d143`.
- Local isolated authenticated browser suite: 103 of 103 tests across 23 spec files on a
  schema-39 stack; Rust fmt and Clippy clean; all Rust tests pass except the Windows-only
  100 ms SSE timing assertion.
- Archive: 870 committed files, SHA-256
  `5eb11d296d29e2f72a0e81dfea3ff00b64dc1d2b6f9fb8b38e89f407423e848f`; every file matched
  `SOURCE_MANIFEST.sha256` on the server.
- Rehearsal on a production copy (schema 38 dump of 270,543 bytes into a throwaway
  PostgreSQL 16 on an internal network): migration 0.7 s, `grant-runtime` for the live role,
  per-document verification and `check-runtime` pass, event and snapshot digests identical,
  candidate API healthy on the restricted login, candidate web serving the public pages.
- Activation: fresh verified backup
  `/opt/dynodoc-demo/backups/dynodoc-before-schema39-20261013T030200Z.dump`
  (270,543 bytes, SHA-256 `8c3fa913146a3802a0d81ac3b2a8e7d165fbfbf21d4553e33c00cadc48ee44d8`);
  migration, grants and verification passed; fingerprints before and after identical
  (2 documents, 36 events, 0 snapshots, 9 identities); API environment contains no migration
  URL, shared-writer flag or signing key; 23 live checks passed; nginx restored last.
  Visitors received maintenance responses for roughly half a minute.
- Outside checks after activation: app and admin health 200, `/source` 302 to the engine
  repository, `security.txt` lists the company mailbox, `/privacy` shows the registered address.

## Rollback

Same policy as the schema 38 release: schema 38 images must not be pointed at schema 39 (the
API validates the complete migration list at start). Recover by restoring the verified dump
above into a new database with the `before-schema39-1b469a2` images, accepting data written
since; a forward fix is preferred. Private artifacts are in
`/opt/dynodoc-releases/20261013-schema39/`; the previous release's are in
`/opt/dynodoc-releases/20261013-schema38/`.

## Follow-ups

- Application licence metadata (`LICENSE`, `Cargo.toml`, `package.json`, `openapi.yaml`) still
  says AGPL-3.0 while the owner has decided the application stays private; and the
  `hyperformula` GPL-3.0-only dependency ships in the browser bundle. Both need an owner and
  counsel decision.
- Effective date, hosting provider, DNS provider and log retention remain
  "Awaiting confirmation" on the legal pages.
