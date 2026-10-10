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

Application source `6f1ac21` and standalone engine `8eef6dd` are on main with green
source CI. All 233 Rust tests pass in each repository, including the unchanged SSE
timing assertion and extended account-erasure check. Local engine lint, docs and
OpenAPI generation pass; rustdoc reports existing link warnings. Frontend lint,
format, production build, 282 unit tests, 16 public browser tests and all 67
signed-in browser tests pass. Profile authorization and exact/oversize upload
integration tests pass.

Project browser journeys cover external proposals/revocation and profile/files/
drafts/merge/native history/recovery. Desktop/mobile and dark file previews were
inspected; the file table passes Axe contrast and mobile overflow checks. The final
read-only Word preview hides the editing ribbon, with a browser regression.
Actual Nginx configuration tests pass for workspace, Projects, profile, Project and
file routes, plus a 9,601-byte synthetic auth-header fixture.

Source CI: [application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38061246753),
[frontend and authenticated stack](https://github.com/ArefinAlter/dynodoc/actions/runs/38061246764),
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38061236800).
Owned local API/web servers are stopped and the labelled disposable database and
volume are removed. Initial local admin setup/rate-limit and load-sensitive timing
failures were resolved; the full source CI passes without weakening assertions.

This increment is now deployed as application `ecbc643`, schema 26. The matching
API/web/admin/converter images passed builds and an isolated upgrade/compatible
rollback/re-upgrade rehearsal before activation. A fresh database backup was
validated first. PostgreSQL, Nginx and unrelated containers were preserved, as were
document/event counts and the aggregate event fingerprint. All 17 live workspace
checks and 22 connector delivery checks pass; a clean browser confirms the Projects
sign-in destination and six downloads without page errors. No real production
sign-in, invitation or document mutation was used. Native-host acceptance remains
open. See the application [deployment and rollback record](https://github.com/ArefinAlter/dynodoc/blob/main/docs/operations/2026-10-10-projects-workspace-release.md).
