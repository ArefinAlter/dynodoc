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
Live rollout verification is recorded below when complete.

## Verification

- Frontend: 103 unit tests, lint and Svelte/TypeScript checks pass; production Node
  build passes. All 37 browser scenarios pass across the full run and focused
  reruns. A low-contrast admin ID style was corrected and rechecked. Erasure impact
  dialogs, public pages, workspace/editor and admin pass the included Axe checks.
  Desktop workspace/admin and mobile homepage screenshots were visually reviewed.
- Backend: 171 tests pass across API/auth/core/schema/snapshot/rich-text/shared
  suites. The schema DELETE test was updated for the new guard's error wording;
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
