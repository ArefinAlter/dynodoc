# Schema 40 release - 13 October 2026

## Deployed release

Application `0f67372c0dc793fc125decaebb2a8c459dd9ff11` is live on the VPS workspace,
private API and admin site at PostgreSQL schema 40. Activation finished at about
2026-10-13T11:36:30Z. Standalone engine source is `370de91`. The public marketing site is
deployed separately by Vercel from the same `main` branch.

This release adds migration 0040 (per-project device-key grants, the `history:review` scope
and the key access mode table), `GET /profiles/{id}/starred`, the profile grants editor and
starred-projects section, the omission of undecided facts from the public legal pages, and
the removal of the AGPL licence text and metadata from the private repository and the
generated API description. Command-line features that need no server change (retention
collector, same-file merge apply, key-file hardening) ship with the CLI from the same
revision.

Not changed: PostgreSQL and converter containers (unchanged IDs and start times), unrelated
applications on the host, the live `.env`, nginx configuration, and every optional writer.
No receipt-signing key is installed (see
[RECEIPT-AUTHORITY-DECISION.md](../RECEIPT-AUTHORITY-DECISION.md)).

## Evidence

- Application Rust CI and frontend CI pass on `0f67372`; engine CI passes on `370de91`.
- Local isolated authenticated browser suite: 105 of 105 tests across 24 spec files on a
  schema-40 stack. Rust fmt and Clippy clean; all Rust tests pass except the Windows-only
  100 ms SSE timing assertion.
- Archive: 887 committed files, SHA-256
  `5eae1d201617ec11c651329c1f21863a645448707b1ba0962cc20cebf96311a5`; every file matched
  `SOURCE_MANIFEST.sha256` on the server.
- Rehearsal on a production copy (schema 39 dump of 290,618 bytes into a throwaway
  PostgreSQL 16 on an internal network): migration 0.7 s, `grant-runtime` for the live role,
  per-document verification and `check-runtime` pass, event and snapshot digests identical,
  candidate API healthy on the restricted login, candidate web serving the public pages.
- Activation: verified backup
  `/opt/dynodoc-demo/backups/dynodoc-before-schema40-20261013T113609Z.dump`
  (290,618 bytes, SHA-256 `398de5d6142d6504d902f737dab5ea2cdd6be6cc2f8a5bf8ea37d70b557e863d`);
  migration, grants and verification passed; fingerprints before and after identical
  (2 documents, 36 events, 0 snapshots, 9 identities); the API environment has no migration
  URL, shared-writer flag or signing key; 23 live checks passed; nginx restored last.
- Outside checks after activation: app and admin health 200, `/source` 302 to the engine
  repository, `/licenses` contains no AGPL text, `/privacy` contains no placeholder text.

## Rollback

Same policy as the earlier schema releases: older images must not be pointed at schema 40
(the API validates the complete migration list at start). Recover by restoring the verified
dump above into a new database with the `before-schema40-0f67372` images, accepting data
written since; a forward fix is preferred. Private artifacts are in
`/opt/dynodoc-releases/20261013-schema40/`.

## Behaviour change to know

New device keys default to "granted" mode and open nothing until the owner grants a project
in Profile; existing keys keep working with all projects until switched. One-hour tokens
issued before this release with only `history:write` get 403 on review approval until they
expire.
