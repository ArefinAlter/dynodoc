# Project activity, contributors and calendar API

12 October 2026. Read-only endpoints for the project workspace and the administrator
console. They add no write path and change no event, hash, checkpoint, grant or
existing response field. Schema 0037 adds three indexes only.

## Authority

The project feed, contributor totals and file `last_change` are visible to current
project members, with the same project and file checks as the file list. Non-members
and unknown projects receive 403. A document id is not a project. Unsubmitted drafts,
private staged commits and file contents are never part of any response.

Names follow the existing display rule: the chosen name, else the part of the email
before the `@` (members already see emails), and `Deleted account` once an account
has been erased. Timestamps are RFC 3339.

## Endpoints

`GET /projects/{id}/files` keeps its fields and paging. Each item may add
`last_change {seq, at, actor_name, summary}`, the newest team event of that file.
`summary` is the change request's name when that event came from a merge, otherwise a
short label of the event type. Files without events omit the key.

`GET /projects/{id}/activity?limit=50&before=<cursor>` returns
`{items, next}`, newest first. `limit` is clamped to 1-100; `next` is an opaque cursor
to pass back as `before`, or null on the last page. A malformed cursor is a 400. An
item has `at`, `kind`, `actor_id`, `actor_name`, `document_id`, `document_title`,
`summary`, `seq`, and `count` for edits. Kinds:

An item gains `"by_agent": true` when the change request it is about was written by an
AI agent identity (`request_opened`, `declined` and `merged` items only); the key is
absent otherwise, so no other row changes.

- `edit`: team events. Events of one write transaction are one item (`count`,
  `summary` "Made N changes", `seq` the newest event). A merge's events are folded
  into its `merged` item.
- `request_opened`, `merged`, `declined`: submitted change requests; `summary` is the
  request name, and `merged` is attributed to the person who merged.
- `version`: a named version; `seq` is the event it covers.
- `file_added`: when a file was created (not when it was moved into the project).
- `member_added`: a project or folder access grant or role change from the audit
  trail. It has no document; `summary` names the person and role.

A page that is cut inside a run of one transaction's events can continue that run as
a separate edit item on the next page. At most 2,000 files and 2,000 folders are
considered; a larger project shows its most recently updated files.

`GET /projects/{id}/contributors` returns
`{contributors:[{identity_id, name, merges, edits, last_at}], capped}` for current
members of the project, at most 200, ordered by merged requests, then events, then
recency. `merges` counts fully merged change requests the member authored; `edits`
counts events they authored. To bound the work, each file contributes at most its
newest N events (N is 100,000 divided by the number of files, between 20 and 20,000);
`capped` is true when a file reached that allowance, so older events may be missing
from `edits`. `last_at` is null for a member without activity.

`GET /profiles/{id}/activity?year=YYYY` returns `{year, total, days:[{date, count}]}`
with UTC calendar days that have events; other days are omitted. The profile must be
visible under the existing profile rule (yourself, or a shared project member), else
404. Your own calendar counts every event you authored. Anyone else's counts only
files in projects or folders both of you can access, so a directly shared standalone
file, a private project or a title is never revealed. `year` is 2000-2100 and
defaults to the current year.

`GET /admin/records?kind=projects` (administrator allowlist) returns 50 rows per page
with the usual `q` substring search and `offset`: `id`, `name`, `members`, `files`,
`bytes`, `created_at`, `last_activity_at` (and `label`, a copy of `name`). `bytes` is
uploaded originals plus shared checkpoint objects, not events, legacy snapshots or
database overhead. `kind=audit` rows add `actor_id` and `detail`, the stored audit
record with values under obviously secret key names replaced by `[redacted]` and
bounded to 4 KiB (a larger record returns `{truncated, bytes, keys}`). Audit also
accepts `actor` (account id or part of the email), `since` and `until` (RFC 3339, or
`YYYY-MM-DD`; a date-only `until` includes that whole day).

## Bounds and operations

Each query is limited by rows, a 3 second database deadline (5 seconds for the admin
project totals) and the existing per-account and per-project read budgets; a timeout
is reported as the usual 504 deadline error and budget exhaustion as 429. The feed
reads the newest day, then 30 days, then a year, then everything, stopping at the
first window that fills a page, and takes at most one page of rows per file.

Migration 0037 adds `event(document_id, created_at desc, id) include (actor_id)`,
`event(actor_id, created_at) include (document_id)` and
`operation_audit(resource_id, id desc)`. Building them blocks event and audit inserts
for the duration of the migration; plan it with the other pending schema upgrades.
Old API builds ignore the indexes. No grants change.

Not claimed: a per-keystroke edit history, exact lifetime edit totals for very
active files, time zones other than UTC, or activity from drafts and staged commits.
