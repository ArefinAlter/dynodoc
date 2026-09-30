# Projects — a GitHub-style workspace: implementation handoff

Status: **approved direction, not started.** Product-owner decision, 30 September 2026:
build a repository-style "Project" workspace in a later release; this file is the
handoff for whoever builds it. Read with [decision 008](../decisions/008-change-requests-presentations-and-ux.md),
[decision 009](../decisions/009-copies-roles-and-notifications.md) and the user guide
[GIT-STYLE-WORKFLOW.md](../GIT-STYLE-WORKFLOW.md).

Starting point: on 30 September the owner reset production — every document was
permanently deleted (accounts kept; see
[operations record](../operations/2026-09-30-document-reset.md)). Production has **0
documents and 0 workspaces**, so no data migration is needed for existing projects.

## 1. What the owner asked for

"A platform like GitHub for Dynodoc's Git-like features, for the workspace, where
people can make projects, compare files, branches, settings, see pull requests,
etc." (screenshot of a GitHub repository page: tabs Code, Issues, Pull requests,
Actions, Projects, Insights, Settings; branch selector; file list with the last
commit message and time; About panel; Releases.)

## 2. What exists today (and where)

| GitHub | Dynodoc now | Code |
| --- | --- | --- |
| Repository | A single document; workspaces (organization → team → folder) only group documents and grant inherited roles | `migrations/0015…`, `crates/engine-api/src/product.rs` (`/spaces`, `/spaces/:id/members`, `/workspace/index?space_id=`, `/documents/:id/location`), `SpaceManager.svelte` |
| Branches | Personal drafts, private to their author until submitted | `workspace.rs` (`/documents/:id/drafts…`, `sync` pull/rebase/share) |
| Pull requests | Change requests with three-way review, approvals, per-change comments, strict mode for unrelated content | `reviews.rs`, `ChangeRequestReview.svelte` |
| Settings / branch protection | Per-document roles and review rules | `access.rs`, `people.rs` (`/documents/:id/people`, `/policy`, `/owner`), `PeoplePanel.svelte` |
| Commits / history | Hash-chained events, named versions, "Who changed this?" | `/documents/:id/events`, `/versions`, `/history-points`, `/state?through_seq=` |
| Compare | Inside a request (ancestor/team/theirs) and version history only | `workspace_merge::diff`, `reviews.rs::detail` |
| Forks / network | Detected copies | `copies.rs`, `/documents/:id/copies` |
| Notifications / Watch | Per-person inbox; no watching | `notifications.rs`, `NotificationBell.svelte` |
| Releases | Named versions per document | `document_version` |

Everything is **per document**. There is no page that groups files and shows
requests, drafts, history and settings across them.

## 3. Product decisions (recommended; confirm open questions in §11)

1. **A project is a new workspace kind, `project`.** It can be top-level (so a person
   without an organization can create one) or sit inside an organization or team. It
   can contain folders (like directories) and documents. Permissions keep working
   through the existing inheritance (`inherited_space_role`): a project member sees
   every document in the project at their project role; direct document grants can
   only add access.
2. **Project roles reuse the ladder** from decision 009: Owner, Manager, Editor,
   Reviewer (`approver`), Contributor (`reviewer`), Viewer. The project Owner is the
   workspace `owner` member.
3. **Review rules are inherited.** A project has default rules; a document without its
   own `document_policy` row uses the nearest project default, then the global
   default. Changing the project default changes every document that has not set its
   own rules; the document's Share panel says where its rules come from.
4. **Drafts stay private.** Unlike Git branches, personal drafts are visible only to
   their author until submitted (decision 008 privacy rule). The Drafts tab shows *your*
   drafts across the project and everyone's *submitted* requests — never other
   people's private drafts.
5. **No project-wide "branch" in the first release.** GitHub's branch selector switches
   the whole tree; Dynodoc versions each document separately. A project-wide snapshot
   ("tag every document now") is proposed as *project releases* in phase 3.
6. **Issues are out of scope for phase 1.** Per-document discussions already exist; a
   project discussion board is phase 3.

## 4. Pages and navigation

Home gets **Projects** in the left rail and **New project**. `/projects` lists projects
the person can open (name, description, open requests, last activity, role).

`/projects/[id]` — header: project name, **Private** badge, **Watch** (All activity /
Requests only / Ignore), **Share**, **New ▾** (document, workbook, deck, upload), open
request count. Tabs:

| Tab | GitHub analogue | Content |
| --- | --- | --- |
| **Files** | Code | Folders first, then files: kind icon, name, *last change summary* ("Tariq's 'Fix totals' merged by Olivia", "Edited by Olivia", "Restored 'Before fieldwork'"), relative time; filter by kind; search within project; drag to folders; upload. |
| **Change requests** | Pull requests | Open / Merged / Declined / Withdrawn across all files; filters: author, reviewer, "needs my review", "unrelated content"; each row shows file, approvals x/y, conflicts, age. Opens the existing review dialog. |
| **Drafts** | Branches | Your drafts across files with "N newer team changes" and Rebase; submitted requests grouped by author. |
| **Compare** | Compare | Pick two files, or two versions of one file (named versions, "the original upload", any revision), side by side with added/removed words marked. |
| **Activity** | Commits + Insights | Timeline across files (edits grouped per person per file per hour, merges, reviews, named versions, members joining); simple counts: changes per week, requests merged, median review time, active contributors. |
| **Settings** | Settings | Name, description, members and roles (existing space members API), default review rules, danger zone (move to Trash, transfer ownership). |

Right-hand panel on Files: About (description), People (top contributors), Releases
(latest named versions across files), Copies (detected copies of project files).

Mobile: tabs become a select; the file table becomes a list.

## 5. Data model (new migration; never edit applied ones)

```sql
-- 00NN_projects.sql (sketch)
alter table access_space drop constraint access_space_kind_check;
alter table access_space add constraint access_space_kind_check
  check (kind in ('organization', 'team', 'folder', 'project'));
-- Organizations and projects may be top level; teams and folders need a parent.
alter table access_space drop constraint access_space_check;   -- confirm name first
alter table access_space add constraint access_space_parent_check
  check ((kind = 'organization' and parent_id is null)
      or (kind = 'project')
      or (kind in ('team', 'folder') and parent_id is not null));

create table project_settings (
  space_id uuid primary key references access_space(id) on delete cascade,
  description text not null default '' check (length(description) <= 2000),
  default_policy jsonb not null default '{}'::jsonb,   -- same fields as document_policy
  created_at timestamptz not null default now(),
  archived_at timestamptz
);
create table project_watch (
  space_id uuid not null references access_space(id) on delete cascade,
  identity_id uuid not null references identity(id),
  level text not null check (level in ('all', 'requests', 'ignore')),
  primary key (space_id, identity_id)
);
-- Fast "last change per document" and activity queries.
create index if not exists event_document_created_idx on event(document_id, created_at desc);
```

Check actual constraint names in `0015_access_lifecycle_and_public_views.sql` before
writing the migration (the parent/kind rule is an unnamed table check). Folder
creation rules in `product.rs::create_space` must allow folders under projects and
projects under organizations/teams. Account erasure already blocks on owned
workspaces; projects inherit that.

## 6. API (engine-api; OpenAPI + `pnpm gen:api` after)

All list endpoints filter by `effective_document_role(d.id, $viewer) is not null` so a
title never leaks to someone who cannot open the document.

| Method & path | Returns / does |
| --- | --- |
| `GET /projects` | Projects the caller can open: id, name, description, role, parent, counts (files, open requests), last activity. |
| `POST /projects` | `{name, description, parent_id?}` → creates `access_space(kind='project')`, owner membership, `project_settings`. |
| `GET /projects/:id` | Header + About + counts + caller capabilities. |
| `POST /projects/:id/settings` | Description, default rules (managers). Audit `project.settings_changed`. |
| `GET /projects/:id/files?folder=` | Folders and documents with last-change summary (lateral join on the latest event per document plus the latest merged request). Paginate by 100. |
| `GET /projects/:id/change-requests?status=&author=&reviewer=&needs_review=` | Submitted requests across files with approvals, conflicts flag (cheap: store `conflicts_at_submit` or compute lazily on open), `source.unrelated`. |
| `GET /projects/:id/drafts` | The caller's drafts across files with `behind`; submitted requests. |
| `GET /projects/:id/activity?before=` | Merged stream of events (grouped), request lifecycle audit rows, named versions, membership changes; 50 per page. |
| `POST /projects/:id/compare` | `{left:{document_id, seq?}, right:{document_id, seq?}}` → both states (permission-checked). Same document: node-ID diff (`workspace_merge::diff`). Different documents: the client aligns content with the existing push planners (`file-push.ts`, `structured-push.ts`). |
| `POST /projects/:id/watch` | `{level}`. |
| `GET /projects/:id/versions` | Named versions across files ("Releases"). |

Notifications: fan out `change_request_submitted`, `change_request_merged`,
`copy_detected` (project files) and a new `project_member_added` to watchers
(`all` or `requests`), deduplicated with the existing per-person rules and never to
the actor.

Policy inheritance: change `access::policy` to fall back to the nearest ancestor
project's `default_policy` when the document has no row; include `source:
"document" | "project" | "default"` in `/documents/:id/people`.

## 7. Frontend

- Routes: `src/routes/projects/+page.svelte`, `src/routes/projects/[id]/+page.svelte`
  (tabs as child components under `src/lib/components/projects/`).
- Reuse: `ChangeRequestReview.svelte` (open a request in place), `PeoplePanel.svelte`
  patterns for members/rules, `NotificationBell.svelte`, `DiffText.svelte`,
  `SpaceManager.svelte` role labels. Use theme tokens from `app.css` only (day/night).
- Home: Projects rail item, "New project" in the create dialog, project filter chips;
  documents created from a project page get `location` set to the project.
- Editor header: breadcrumb "Project › folder › file" linking back.

## 8. Performance

- Files tab: one query with `left join lateral (select … from event where
  document_id=d.id order by seq desc limit 1)`; the event PK covers it.
- Activity: page by `created_at` with the new index; group in SQL by (actor, document,
  date_trunc('hour')).
- Change requests: existing `workspace_draft_submitted_idx`; add `(document_id,
  review_outcome, merged_at)` only if profiling shows a need.
- Compare across documents runs in the browser (alignment already takes ~0.2 s for a
  3,000-row sheet).

## 9. Tests and acceptance

Rust integration tests (`crates/engine-api/tests/projects_test.rs`): create project,
invite with each role, inherited access, private drafts never listed for others,
title non-leakage (a document with no project access is invisible), default rules
inherited and overridden, watchers notified once, compare permission checks, erasure
of a project document cascades out of every tab.

Browser spec (`e2e/projects.spec.ts`): create project → add files → invite two people
→ contributor pushes an edited Word file → request appears in the project's Change
requests tab → reviewer approves → owner merges → Files tab shows the merge summary →
Activity shows the sequence → Compare two versions → night mode screenshot.

Acceptance criteria:
1. A person with no organization can create a project from Home in ≤ 3 clicks.
2. Every tab loads in < 1 s with 200 files and 2,000 events on the VPS.
3. No endpoint returns a title, email or content from a document the caller cannot open.
4. Default rules apply to new files and show their source in Share.
5. All pages pass the axe checks used by the existing suite in day and night mode.

## 10. Phasing

1. **Phase 1:** project kind, `/projects` list and page, Files, Change requests, Drafts,
   Settings (members + default rules), Watch, breadcrumbs.
2. **Phase 2:** Compare, Activity + counts, Releases panel, project search.
3. **Phase 3:** project releases (snapshot every file at once, compare releases),
   discussion board ("Issues"), templates, project-level copy report, export the whole
   project as a zip with a provenance manifest.

## 11. Open questions for the owner

1. Should projects be visible to everyone in an organization by default, or only to
   invited members? (Recommended: invited only.)
2. May a document belong to more than one project? (Recommended: no — one location;
   link instead.)
3. Should managers see other people's private drafts? (Recommended: no.)
4. Is "Issues" wanted in phase 1?
5. Should the agentic/AI provenance work ([proposal](AGENT-PROVENANCE-PROPOSAL.md)) show
   per-project AI-usage summaries in Activity from the start?
