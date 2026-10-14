# Schema 41 release - 14 October 2026

## Deployed release

Application `f62d62c641396a1ecf9c5c588e1af2436127a7f6` is live on the VPS workspace, private
API and admin site at PostgreSQL schema 41. Activation finished at about 2026-10-14T08:45:30Z
(14:45 in Asia/Dhaka). Standalone engine source is `ebf2319`. The public marketing site is
deployed separately by Vercel from the same `main` branch.

This release adds migration 0041 (receipt-authority epochs, dual-signed rotation statements,
revocation markers; tables immutable and read-only for the runtime role) with the API additions
that serve the chain, and the web build with the in-house spreadsheet formula engine that
replaces `hyperformula`. Command-line features that need no server change ship with the CLI from
the same revision: passphrase-encrypted key files with optional OS key storage, local repository
schema 3 object compression with `compact`, the retention collector and the same-file merge
apply.

Not changed: PostgreSQL and converter containers (unchanged IDs and start times), unrelated
applications on the host, the live `.env`, nginx configuration, and every optional writer. No
receipt-signing key is installed (see [RECEIPT-AUTHORITY-DECISION.md](../RECEIPT-AUTHORITY-DECISION.md));
the API starts because no authority exists, exactly as before.

## Evidence

- Application Rust CI and frontend CI pass on `f62d62c`; engine CI passes on `ebf2319`. An earlier
  revision (`fbf831a`) failed Rust CI on Linux only: six key-file unit tests wrote default-mode
  fixtures that the new Unix permission check correctly refuses. The fix (owner-only fixtures) was
  validated by running the engine-cli unit and integration tests in a Linux container (54 and 3
  passing) before the re-run. The released binaries do not differ in behaviour from `fbf831a`.
- Local isolated authenticated browser suite: 105 of 105 tests across 24 spec files on a schema-41
  stack, including the spreadsheet specs against the new engine. Unit tests 461; the formula
  engine corpus has 2,020 Excel-evaluated cases.
- Archive: 965 committed files, SHA-256
  `a23dd9ccb5fd074b408d5cb2edbeafe0b1ad17603e12abf851e7c1dc1c2890ac`; every file matched
  `SOURCE_MANIFEST.sha256` on the server.
- Rehearsal on a production copy (schema 40 dump of 300,442 bytes into a throwaway PostgreSQL 16 on
  an internal network): migration 0.6 s, `grant-runtime` for the live role, per-document
  verification and `check-runtime` pass, event and snapshot digests identical, candidate API
  healthy on the restricted login, candidate web serving the public pages.
- Activation: verified backup
  `/opt/dynodoc-demo/backups/dynodoc-before-schema41-20261014T084502Z.dump` (300,442 bytes, SHA-256
  `3bc70ab47a35145a4a9c402698843917797d459718383da99b5506d1aa459ab6`); migration, grants and
  verification passed; fingerprints before and after identical (2 documents, 36 events, 0
  snapshots, 9 identities); API environment has no migration URL, shared-writer flag or signing key;
  23 live checks passed; nginx restored last.

## Rollback

As for earlier schema releases: older images must not be pointed at schema 41. Restore the verified
dump above into a new database with the `before-schema41-f62d62c` images, accepting data written
since; a forward fix is preferred. Private artifacts are in `/opt/dynodoc-releases/20261014-schema41/`.

## Environment note

The development PC's system drive reached zero free space during this work (a Linux build volume
inside Docker Desktop) and Docker Desktop failed to start. Disposable caches and the volume were
removed and Docker restarted; the development containers came back healthy. Keep several GB free
on C: before running Linux container builds.
