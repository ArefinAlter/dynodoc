# Projects workspace increment — 10 October 2026

## Behavior

Authentication and homepage workspace links now lead to Projects. The new shared
navigation separates Projects, Editors, editor installations and the member's
profile. Profiles contain an editable name and short bio, with initials as the
avatar. Only the account itself and current shared Project members can read them;
profile responses contain no email. Saving checks the profile revision, and later
provider sign-in preserves manually chosen names.
Account erasure clears the bio and invalidates the profile revision as well as the
existing credentials and identity metadata; the erasure regression checks this.

Projects have a file table, current main revisions, pull requests, private drafts,
web editors and settings. Upload multiple supported files or choose an existing
file you manage by name. The picker pages through existing files without loading
the whole library. Profile links appear in the authorized membership view.

Opening a file shows a read-only preview with main/private-draft selection. Create
a private draft, compare it with main, submit it for review and use existing
approval/merge rules. History reads at most 50 revisions per page and exposes
retained-state downloads, restore and net range revert/cherry-pick proposals.
Recovery creates a private draft; main changes only through an authorized merge.
The existing web editor has a separate tab and a path back to the repository.

Original uploaded files are limited to **3,000,000 bytes**, checked before browser
parsing and in API upload, legacy conversion and provider-import paths. Base64 is
bounded before decoding. Rejection creates no original-upload row. Provenance JSON,
expanded ZIP content and state accumulated through later edits have separate
bounds; the original-byte cap is not a proof of bounded large-document memory.
Existing historical files are not erased by this change.

## Compatibility and remaining work

Migration **0026** adds `identity.bio` and `identity.profile_revision`. Existing
events, stable IDs and hash-chain semantics are unchanged. Deploy matching API/web
builds together: the new frontend requires the profile endpoints, and the new API
requires migration 0026. Regenerated OpenAPI/types include the profile routes.
The converter image also has the new byte cap and needs its own rebuild on rollout.

The selector exposes **per-file private drafts**, not project-wide Git branches.
Portable commits/parents, atomic multi-file branches and resumable exchange remain
part 3. Part 2 bounded values/assets, compression and shared named-version/draft
references remain the next engine priority. Shared checkpoint writes remain off.
The six native connectors still need installed-host acceptance and publication;
native exports retain the existing supported-format fidelity limits.

## Verification and release state

Standalone engine source `a5c8eda` is on main and its CI is green. Local engine lint,
all 233 Rust tests, docs generation and OpenAPI generation pass; rustdoc reports
existing link warnings. Frontend unit tests: 282 pass. Profile authorization and
exact/oversize upload integration tests pass. Both production-mode Project browser
journeys pass, including external proposals/revocation and profile/files/drafts/
merge/native history/recovery. Desktop/mobile layouts were inspected; the file table
passes Axe contrast and mobile overflow checks. The read-only Word preview's editing
ribbon was removed after visual inspection and is receiving the final build check.

Application Rust tests passed 232 checks with the existing 100 ms SSE assertion
isolated after local timing failures. Standalone serial and CI suites pass that
same assertion. Full application browser regression and source CI are still pending.
This source increment is **not deployed**. The VPS remains on application `536a448`,
schema 25. Keep its validated backup and compatible rollback release; rehearse a
schema-26-compatible rollback before replacing this deployment. No production data,
accounts, invitations or containers were changed during this implementation.
