# Web product operations release — 21 September 2026

This increment implements [decision 005](decisions/005-product-operations-and-recovery.md).
Off-site backups are explicitly deferred. Only web clients are in scope.

## Changes

- Modernized homepage, workspace and administration layout while preserving the
  cream/blue brand, sans serif typography, product demonstrations and SEO work.
- Administrator erasure with dependency/impact preview, typed confirmation,
  immutable receipts and transaction-scoped database guards. Ownership transfer,
  forced sign-out, public-link revocation and current-page CSV exports.
- Canonical comment replies and resolve/reopen actions, thread revision checks,
  pagination, block filters and exclusion from public copies.
- Trusted-device opt-in browser recovery with original save base, explicit review,
  account/document scoping, quotas/expiry and cross-tab sign-out clearing.
- Remembered exclusions when sharing selected draft blocks.
- Product/admin/discussion OpenAPI coverage and authenticated browser CI.

## Compatibility and deployment

Migrations 0017–0019 are additive to the baseline's sixteen migrations; migration
0017 deliberately replaces DELETE guards for the approved whole-resource erasure
path. Event UPDATEs and ordinary history deletion remain prohibited.

`office-v3` recognizes the new discussion event variants. Deploy compatible web
readers/gateway first, then the backend writers. Once new events exist, do not
roll back to the old event vocabulary. Preserve a compatible image/source archive.
Deployment does not erase any live document or account.

Pre-rollout inventory: 553 canonical events, 10 documents, 5 uploads; migration 16.
The existing on-server backup command completed successfully before rollout.
Unrelated server-api, server-db, server-minio and Chakma containers were healthy.
Release commit: `e73db397c4d3afae846223977a8f58102c6bd71b`, pushed to `main`.
The VPS web/admin readers were replaced first, followed by the API. All three
services are healthy; schema is now migration 19. Post-rollout counts remain
**553 events, 10 documents and 5 uploads**, with **zero erasure receipts**. No live
record was erased. Unrelated services remain running and healthy.

Pre-rollout backup: `backups/dynodoc-20260921T155437Z.dump` (archive listing validated).
Previous images are tagged `dynodoc-demo-{api,web,admin}:pre-product-operations` and
previous source is retained at `releases/pre-product-operations-source.tar.gz`.
Do not use the old API after new discussion events have been recorded; forward-fix
or restore a compatible application. Never overwrite live data to roll back a UI.

The final runtime-source archive has SHA-256
`55d41a3f9813a842eac2583adb0d632f7370513251ad56fa3d1f94d2fe8a844a`.
Private environment files were preserved. HTTPS health endpoints, workspace/admin
login pages, anonymous document rejection and sitemap exclusion of private pages
were checked. Both production Google routes redirect to Google's authorization
endpoint with the correct workspace/admin callback. This is not completed user
consent or real email-delivery acceptance.

All 553 stored event sequence/previous-hash/chain-hash links are consistent after
rollout. This SQL check checks linkage against stored content hashes; it does not
recompute semantic payload hashes. Production `fsync` and `synchronous_commit`
remain `on`. The disposable local verification database was removed.

Vercel successfully deployed the release and `dynodoc.online` serves the new hero
and correct workspace link. All release CI jobs pass, including the new authenticated browser job:
[Rust workflow](https://github.com/ArefinAlter/dynodoc/actions/runs/35623722037) and
[frontend + authenticated workflows](https://github.com/ArefinAlter/dynodoc/actions/runs/35623721976).

## Verification

- Frontend: 103 unit tests, lint and Svelte/TypeScript checks pass; production Node
  build passes. All 37 browser scenarios pass across the full run and focused
  reruns. A low-contrast admin ID style was corrected and rechecked. Erasure impact
  dialogs, public pages, workspace/editor and admin pass the included Axe checks.
  Desktop workspace/admin and mobile homepage screenshots were visually reviewed.
- Backend: The final complete `cargo test --workspace` run passes all 171 tests across
  API/auth/core/schema/snapshot/rich-text/shared suites. The schema DELETE test was
  updated for the new guard's error wording;
  direct deletion remains rejected. Tests include erasure guards, shared-chain
  preservation, thread permissions and persistent draft exclusions. Strict
  workspace Clippy passes. Set-based inclusion/exclusion lookups avoid quadratic
  scans and the final selection regression passes.
- Browser failures corrected during verification were stale heading/select/table
  selectors and the expected privacy-preserving 403 for an erased document.
  Repeated local admin sign-in attempts hit the real throttle; it was reset only
  in the disposable test database, with no production bypass introduced.
- All destructive tests use a disposable local PostgreSQL database. Disk durability
  was disabled there to avoid Docker Desktop checkpoint delays; this does not
  establish crash/power-loss durability. Production PostgreSQL was not modified.

## Remaining acceptance and engineering

This release does not establish complete Word/Excel parity. Editable pagination,
advanced Office fidelity, footnotes/bibliography, charts/pivots/protected ranges,
shared branch review, notification delivery, distributed/offline synchronization
and the later research/AI roadmap remain open. The complete backlog is preserved
in HANDOFF.md and PROJECT-STATUS.md; the historical pilot count is not completion.

Real Google/Resend delivery and provider-study acceptance, legal operator details,
native IME/screen-reader sessions and broader file-corpus acceptance still require
owner/real-user evidence. No off-site backup destination was configured, by request.
