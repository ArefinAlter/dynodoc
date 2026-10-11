# Project rules and metadata authority

11 October 2026. Source contract; verification and deployment state are recorded in
[the handoff](ENGINE-EVOLUTION-HANDOFF.md). Schema remains 0029. No event, checkpoint,
hash, applied migration or runtime database grant changes.

## Rules and authority

Project rules are minimum protection for every contained file, including files in
nested folders and files with older saved overrides. Effective protection is the
logical OR of project/file protection; approvals use the maximum; owner/manager-only
merging wins if either policy requires it. New file rules that weaken the project
are rejected. Project owners/managers can strengthen a file or restore inheritance;
they change the project minimum in Project settings. This increment adds no file
exception that weakens the project.

File ownership still identifies the creator and preserves existing file access,
sharing, transfer and trash behavior. It does **not** grant project review authority.
Only project owners/managers get the management bypass for direct content writes,
approval overrides, settings/archive and file moves. A file owner or nested-folder
manager without project management is treated as an editor for those policy checks.
Explicit file access otherwise remains additive: this is not a redesign that removes
all directly shared access when project membership changes. Standalone files keep
their existing owner/manager authority. Administrative whole-resource erasure is a
separate audited boundary.

The same effective rules govern ordinary writes, suggestions accepted into the team
version, draft sharing/restoration, request merge/decline and UI capabilities. A
file-owner/project-editor may submit changes and merge when the project permits
editors and approval requirements are satisfied. They cannot waive those approvals.
The existing audited override remains available to the governing managers.

Moving a project file requires its source project's management authority, plus
editor access to the destination. Importing one's standalone file into a project
remains available to project editors. Destination rules apply immediately. There
is no API for relocating folders; inherited rules are tested with files in nested
folders. Removing the file from a project is also a move and needs source authority.

Mutations lock the document before reading project authority. Project policy setters
and member changes exclusively lock their project row; document operations share
that row. Moves lock source/destination project roots in UUID order. Tests observe
the database lock wait, then commit a policy tightening or manager demotion and
confirm the waiting request is refused. Unrelated project writes share no global
lock. Existing folder membership behavior outside this project boundary is unchanged.

`GET /documents/{id}/people` reports effective `policy`. A project `policy_source`
includes its `floor` and nullable `file_rules`; the source stays `project` even when
file rules exist. `manage_rules`, `edit_team` and `merge` use governing authority;
new `archive` and `edit_settings` capabilities describe metadata access. The sharing
panel displays the project minimum and prevents selecting weaker rules.

## Metadata writes

`POST /documents/{id}/metadata` accepts only `title`, `settings`, `archived`.
Unknown top-level fields are rejected. Renaming requires direct team-edit permission;
settings and archive changes require the governing owner/manager. Titles are trimmed
and remain bounded to 500 bytes. A request body is capped at 80 KiB.

Settings are a **shallow patch**, rather than replacement of the entire object.
Omitted stored keys survive, including legacy adapter data. Submitted keys are:

| Key | Accepted value |
|---|---|
| `kind` | `document`, `spreadsheet`, `presentation`, `questionnaire` |
| `copied_from` | Accessible, different document UUID, or null |
| `import_warnings` | At most 256 strings, each at most 2,000 bytes |
| `research_source` | Null or the existing six-field adapter record: provider, origin, project, form, kind, fetched_at; bounded strings and known provider/kind enums |

Submitted and resulting settings are capped at 64 KiB of encoded JSON. Unknown keys,
arbitrary nested values, review/access rules and credentials in research-source
records are rejected. Research origin/copy metadata remains a client claim, not
proof of authorship or ancestry. A source UUID never grants document access.

Actual title/settings/status changes append `document.metadata_changed` with actor,
resource, changed-field names and document-bound before/after SHA-256 digests in
the same transaction as the UPDATE. Receipts deliberately omit readable titles and
settings: operational audit survives document erasure. Digests use the canonical
JSON fingerprint of `{domain:"dynodoc.metadata.v1", document_id, metadata}`; the
receipt is tagged `dynodoc.metadata-audit.v1`. Low-entropy values can still be guessed
against a digest; it is not encryption or a recoverable metadata version history.
Failure to append the audit rolls back the metadata; identical retries/no-ops add no audit row.
These internal operational receipts are not canonical content events or independent
signatures. They do not retroactively audit older metadata changes. Concurrent
metadata writes serialize; no new optimistic revision API is claimed.

Web creation/import initializes the standalone file before moving it into the chosen
project, so new project-editor uploads do not need a protection bypass. Failed
imports remain recoverable in the editor library. The project rules apply as soon
as the initialized file is attached.

## Rollout and remaining gates

Deploy API and web together after the existing schema-27/28/29 session, snapshot
backfill and restricted-login upgrade rehearsal. Old API builds retain the policy
bypass, even though this increment adds no migration. Retain a compatible patched
image for rollback. No VPS workspace/API deployment is implied by source checks.

This closes the identified API policy/metadata paths; the runtime SQL credential
still has operational-table DML and the database owner remains trusted. General
CSP/HSTS, search/deadline/rate/concurrency budgets and independent commitments remain
open. Rules add bounded hierarchy/policy queries, not a new document-diff algorithm;
the shared write path still materializes state. Bounded chunks/shared references,
portable commits/branches, offline convergence and native-host acceptance remain
on the evolution plan.
