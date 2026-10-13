# Project issues, labels and stars API

13 October 2026, schema 0039. Invite-only, per-project work tracking: a tracked
question or task about a project (a review follow-up, missing evidence, assigned work).
It is not a code host feature set: there are no milestones, reactions, mentions,
templates, cross references or notifications. Nothing here changes an event, hash,
checkpoint, change request or any existing response field.

All routes use the ordinary account login (`Authorization: Bearer <access token>`).
**Project device tokens are refused (401)**; the device allow-list is unchanged. Request
bodies are JSON with unknown fields rejected. Timestamps are RFC 3339. Text is stored
exactly as typed (after trimming); clients must render it as text or sanitised
Markdown, never as HTML.

## Authority

Roles are the existing project roles as returned by `GET /projects/{id}` (`role`).
Rank: owner 5, manager 4, editor 3, approver 2, reviewer 2, viewer 1. In the workspace
UI `approver` is "reviewer" and `reviewer` is "contributor".

| Action | Who |
|---|---|
| Read issues, comments, labels, stars | any current project member, viewers included |
| Open an issue, comment | rank 2 or higher (everyone except `viewer`) |
| Edit title, description, labels, assignee; close; reopen | the issue's author, or a manager or owner. The author must still be rank 2 or higher: a person demoted to viewer can no longer edit their own issue |
| Create, edit, delete a label | manager or owner |
| Assign | to a current project member only (checked when the assignee changes) |
| Star or unstar | any current member, for their own account only |

Non-members and unknown projects receive the same 403, so a project id reveals
nothing. A 403 caused by role carries a readable `details.reason`. A project member
who asks for an issue number or label that does not exist receives 404. A project has
no archived or disabled state in schema 39; if one is added, these writes must be
gated by it. Disabled and erased accounts cannot authenticate and are not members.

## Limits

| Item | Limit |
|---|---|
| Issues per project (never reused; closing does not free a number) | 5,000 |
| Issue title | 1-200 characters, one line (no control characters) |
| Issue description | 0-20,000 characters |
| Comment | 1-10,000 characters, at most 1,000 per issue |
| Labels per project | 100 |
| Labels per issue | 20 |
| Label name | 1-50 characters, unique per project ignoring case |
| Label colour | 6 hexadecimal digits, stored lowercase without `#` (a leading `#` and upper case are accepted) |
| Label description | 0-200 characters |
| List page | `limit` 1-100 (default 30); comments `limit` 1-100 (default 50) |
| Title search `q` | at most 100 characters, case-insensitive substring |
| Request body | 128 KiB |

Limit refusals are `409` with a readable `details.reason`; invalid values are `400`.
A body that is not the documented JSON shape (a missing or unknown field, a wrong type)
is the framework's `422`.
Every query has a 3 second database deadline (504 on timeout; writes also give up on a
lock after 3 seconds). Reads and writes share the existing per-account (2 at once, 120 a
minute) and per-project (4 at once, 600 a minute) admission budgets: `429` with
`Retry-After`.

## Shapes

`Person`: `{id, name}`. `name` is the chosen name, else the part of the email before
`@`, and `Deleted account` after erasure (the `id` stays).

`Label`: `{id, name, color, description}`.

`IssueSummary` (list items): `{id, number, title, state, author: Person,
assignee: null | {id, name, member}, labels: [Label], comment_count, created_at,
updated_at, closed_at}`. `state` is `open` or `closed`. `assignee.member` is false when
the assignee has since left the project (the assignment is kept, shown as stale).
`updated_at` moves on any edit, state change or comment.

`Issue` (one issue; returned by create, get and update): the summary fields plus
`body`, `revision`, `closed_by: null | Person` and
`viewer: {role, can_comment, can_edit}` (what the caller may do right now; `can_edit`
covers title, description, labels, assignee and state).

## Issues

`GET /projects/{id}/issues` query: `state=open|closed|all` (default `open`),
`label` (a label id, or a label name ignoring case), `assignee` (a person id, `me` or
`none`), `author` (a person id or `me`), `q`, `before` (an issue number; rows with a
smaller number), `limit`. Newest number first. Returns
`{items: [IssueSummary], next_before, counts: {open, closed}}`; `next_before` is the
value to send as `before`, or null on the last page. `counts` ignores the filters.

`POST /projects/{id}/issues` body `{title, body?, labels?: [label id], assignee_id?}`.
Returns the `Issue`. 400 for invalid text, an unknown label or more than 20 labels, an
assignee who is not a member; 409 at 5,000 issues.

`GET /projects/{id}/issues/{number}` query `after` (a comment `seq`, default 0) and
`limit`. Returns the `Issue` plus `comments: {items, next_after}`, oldest first. A comment
is `{id, seq, author: Person, body, created_at}`; `seq` is 1, 2, 3... within the issue.
`next_after` is the value to send as `after`, or null on the last page.

`PATCH /projects/{id}/issues/{number}` body: any of `title`, `body`,
`state` (`open` or `closed`), `labels` (the complete new set; an empty array clears),
`assignee_id` (a person id, or `null` to unassign) and `revision`. At least one field
is required. `revision` is required whenever `title` or `body` is sent and must equal
the issue's current `revision`, otherwise 409 ("changed since you opened it"); it is
optional for the other fields and checked if sent. `revision` increases by one on every
change that is not a comment. A request that changes nothing succeeds without a write.
Returns the `Issue` (without `comments`).

`POST /projects/{id}/issues/{number}/comments` body `{body}`. Returns
`{comment, comment_count}`. Comments are immutable and cannot be deleted. They are
allowed on closed issues. 409 at 1,000 comments.

## Labels

`GET /projects/{id}/labels` returns `{items: [{id, name, color, description,
open_issue_count}], can_manage}` ordered by name, at most 100.

`POST /projects/{id}/labels` body `{name, color, description?}` returns the `Label`.
409 for a duplicate name or the 100-label limit.

`PATCH /projects/{id}/labels/{label_id}` body: any of `name`, `color`, `description`.
Returns the `Label`. 409 for a duplicate name. Existing assignments follow the label.

`DELETE /projects/{id}/labels/{label_id}` removes the label from every issue and returns
`{ok: true, issues_updated}`. The issues' `updated_at` is not changed.

## Stars

`PUT /projects/{id}/star` and `DELETE /projects/{id}/star` set or clear the caller's
own star; both are idempotent and return `{starred, star_count}`. `star_count` counts
current, active members who starred the project.

## Project payloads

`GET /projects` items and `GET /projects/{id}` gain three fields; every existing field
is unchanged: `starred` (the caller's own star), `star_count` and `open_issue_count`.

## Audit

Every state change writes an `operation_audit` row with the project id as the
resource and the actor's account: `issue.created`, `issue.edited`, `issue.closed`,
`issue.reopened`, `issue.assigned`, `issue.unassigned`, `issue.labels_changed`,
`issue.commented`, `label.created`, `label.edited`, `label.deleted`,
`project.starred`, `project.unstarred`. The detail holds the issue `id` and `number`
(or the label) and the changed values. An `issue.edited` row keeps the previous title
(in full) and the previous description (first 2,000 characters, with
`previous_body_chars` giving the original length). Comment text is not copied into
the audit. A no-op star, unstar or patch writes nothing. Audit rows are append-only.

## Erasure semantics

- **Account erasure** (the audited administrator entrypoint) keeps the account row
  as the existing anonymised form: issues and comments it authored stay, shown as
  `Deleted account`; the account's stars are deleted and any issue assigned to it is
  unassigned. The administrator preview adds `issue_contributions_retained` (a count);
  the receipt keeps only opaque counts, never titles or text.
- **Project removal** (no API entrypoint exists in schema 39; a database operator
  deleting a project) removes its issues, comments, labels, issue-label links, issue
  counter and stars through foreign-key cascades. Existing audit rows are append-only and
  keep the prior title/description text of edited issues, bounded as above; neither
  erasure rewrites them.
- **Document erasure** does not touch issues: issues belong to the project, not to a
  file.
- The runtime database role can insert and update issues but cannot delete them, and can
  only insert comments (no update, no delete). Labels and stars are the only rows it can
  delete.

## Rollout

Migration 0039 only adds tables and indexes. Run `engine-cli migrate`, then
`engine-cli grant-runtime ROLE`, then `check-runtime` (see
[database privileges](DATABASE-PRIVILEGES.md)). A schema-38 API build refuses a
schema-39 database; deploy the matching API and web builds together.
