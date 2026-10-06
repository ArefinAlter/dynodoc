# 010 — External-editor provenance and Projects come first

6 October 2026. The owner clarified that Dynodoc is a document version-control
platform, not a competitor to Microsoft or Google editors. Word and Google Docs
receive equal priority. This direction supersedes further Office-parity work in
decisions 006–008; existing web editors remain supported.

The primary workflow is: opt into capture in an existing editor, retain local
block observations, push a portable change bundle, review and merge in Projects,
then pull a checkpoint to update another working copy. Stable block IDs identify
changes. Existing filename/content matching suggests document relationships;
matching alone never proves authorship or silently attaches an unrelated history.

## First implementation

- Top-level Projects are invite-only; ordinary folders can live beneath them.
  Files, submitted change requests, the caller's private drafts and Settings work
  across a project. Project membership does not expose other people's drafts.
- Files inherit project review rules unless they have an explicit override.
  New projects protect the team version from direct editor writes. Owners and
  managers retain the existing documented override capability. Settings use a
  revision check; writes serialize with project role/rule changes.
- Watch adds in-app notifications for new requests. Existing role notifications
  still apply. Merge/activity digests and additional email notifications are later
  work; the UI does not offer an inert “watch everything” option.
- `dynodoc.change-bundle` version 1 records a document ID, client/bundle/change IDs,
  exact base sequence and hash, capture description, observed timestamps and
  content operations. Uploads create private drafts or submitted requests,
  never canonical writes. Durable receipts make exact retries safe.
- Stored bundles cannot be edited or deleted through ordinary operations.
  Existing audited whole-document/account erasure removes them with their drafts.
  Uploader identity and reception time are server facts. Host, local filename,
  timestamps and “observed” descriptions are unverified client claims. A digest
  detects changes to the stored normalized upload; it is not an external timestamp
  or proof that a particular human/AI produced the text.
- Seven-day, revocable editor keys read/propose on one document only. They cannot
  merge or manage access and cannot be used as full account sessions. Disabled
  accounts, changed session generations and removed access invalidate their use.

## Word and Google Docs development connectors

Both use the same capture/retry/pull implementation. Word stores stable IDs in
content-control tags, and Google Docs uses named ranges in the active tab. The
first supported exchange is existing paragraph text: no insert/delete/move sync,
tables, lists, images, spreadsheet cells, slide shapes or formatting provenance.
Unsupported remote block structures fail explicitly. Host-side formatting itself
is not captured, and text replacements can inherit host formatting.

Capture starts through an explicit control and observes snapshots while a pane is
open. It is not a complete keystroke recorder and does not observe work done while
closed. Google Docs has no general `onEdit` trigger; its sidebar polls. Queues are
saved before sending, persisted uploads keep their IDs through retries, and late
typing remains available. A second upload waits for the earlier request's review.
Pull previews use a three-way comparison, stop on overlaps or structural changes,
recheck the local text before writing, and advance the checkpoint only after the
host write. Local observations are archived in the exportable ledger during rebase.

These are development installations, not marketplace publications or verified
host integrations. See [installation and acceptance](../../extensions/README.md)
and [protocol](../PROVENANCE-SYNC.md). Next: actual Word/Docs cross-host acceptance,
structural block sync and durable background/local-agent capture, then richer
project activity/releases. Office-parity work and AI-grading features are not the
next priority.
