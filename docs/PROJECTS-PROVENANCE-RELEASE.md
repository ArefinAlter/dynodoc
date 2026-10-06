# Projects and external-editor provenance — development release

6 October 2026. Implements [decision 010](decisions/010-external-editor-provenance-and-projects.md).
This release prioritizes collecting and reviewing changes from existing editors.
The basic web editors stay; further Word/Excel/Slides feature parity is not the goal.

## Implemented

Projects creation/listing, invite-only membership, Files, cross-file change
requests, private own Drafts, revision-checked Settings, inherited review rules
and submission Watch notifications. Files can be imported/moved without entering
the web editor. Review uses the existing approvals/conflict/merge workflow.
Projects expose checkpoints, bundle uploads/downloads and scoped connection keys.
Workspace invitations link to the project; role updates/removals remain email-free.

Migration 0022 adds Projects and rules; 0023 adds scoped connector grants and
immutable provenance uploads/receipts. Existing migrations and event variants are
unchanged. Project settings/membership changes serialize with content writes.
Audited erasure removes the new child data without weakening history guards.

Word task-pane and Google Docs sidebar sources share one capture/retry/pull
protocol. Queues are saved before uploads, exact retries return one draft, base
hashes are checked, and uploader attribution comes from authenticated credentials.
Development text-only adapters use content-control tags / named ranges. Capture
is opt-in and pane-dependent; it cannot prove every local edit or human authorship.

## Verification and publication

New Rust integration tests exercise project privacy, inherited policy overrides,
watch notification delivery, concurrent exact retries, invalid-operation rollback,
base-hash mismatches, host/file scoping, revocation and audited document erasure.
Shared protocol tests exercise offline recovery, late typing, receipt mismatches,
storage failure, concurrent capture, overlaps and Word-to-Docs checkpoint exchange.
Google storage tests exercise Unicode chunks, stale-generation rejection and
preserving a prior queue after a failed write. A signed-in browser workflow covers
project creation, invitations without email, scoped upload/retry, review/merge,
checkpoint retrieval and revocation through the real gateway/engine.

Local verification: Rust formatting/Clippy and OpenAPI generation pass. All
remaining Rust tests pass with the existing SSE timing test filtered locally;
the three new Projects/provenance integration tests pass, including forced
sign-out, expiry, access removal and private-account erasure. Frontend lint and
production build pass with zero Svelte diagnostics. The full frontend suite
passes 212 tests with one worker; four subsequently added gateway/sign-in tests
also pass. The shared connector/host-contract/storage subset passes 15 tests.
The signed-in Projects browser workflow passes against an isolated PostgreSQL
16/API/web stack, including real gateway upload/retry, review/merge and revocation.
The broader signed-in browser run passed 54 of 55 tests. Its existing Word-file
test used Control+Home on a paginated editor and appended text to the wrong
paragraph, creating an unintended numbers conflict. Targeting the intended
introduction paragraph explicitly preserves the test's original assertions; its
full upload/review/merge/rebase scenario then passed in a focused rerun.

An initial parallel frontend run exceeded the existing five-second Excel test
timeout during concurrent compilation; the serial full rerun passed unchanged.
The browser test was corrected to use the combobox's accessible name and the
existing review panel's split diff text/selected-change button; no UI check was
removed. Linux CI references follow once the committed revision is verified.
Native Windows retains the previously recorded 100 ms SSE test timing failure;
that assertion is unchanged and remains enabled in Linux CI.

No VPS rollout, live editor acceptance or marketplace publication has been
performed for these changes. Source availability is not installed-host support.
Follow [connector installation/acceptance](../extensions/README.md); structural
sync, durable background capture, secure pairing/publication and richer project
activity/releases remain prioritized work.
