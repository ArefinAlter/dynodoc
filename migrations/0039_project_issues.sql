-- Project issues, labels and stars. See docs/PROJECT-ISSUES-API.md.
--
-- An issue is a tracked question or task about a project. It never touches the
-- event log, hash chain, checkpoints or change requests. Issues are not deleted by
-- the runtime role (it can insert and update them), comments are insert-only, and
-- labels and stars are the only rows it removes. Foreign keys to the project
-- cascade so that a database operator removing a project removes its issues,
-- comments, labels and stars; there is no API entrypoint for that today. Account
-- erasure keeps the identity row (the existing anonymised form), so authorship
-- references stay valid.

-- One row per project; the row lock serialises issue numbering. `next_number`
-- moves only in the transaction that inserts the issue, so a rolled-back creation
-- never consumes a number and numbers are never reused. The check is the
-- 5,000-issue limit's backstop.
create table project_issue_counter (
    project_id uuid primary key references project_settings(space_id) on delete cascade,
    next_number integer not null default 1 check (next_number between 1 and 5001)
);

create table project_issue (
    id uuid primary key default gen_random_uuid(),
    project_id uuid not null references project_settings(space_id) on delete cascade,
    number integer not null check (number between 1 and 5000),
    title text not null check (char_length(title) between 1 and 200 and btrim(title) <> ''),
    body text not null default '' check (char_length(body) <= 20000),
    state text not null default 'open' check (state in ('open','closed')),
    author_id uuid not null references identity(id),
    assignee_id uuid references identity(id),
    -- Moves on edits, state changes and comments.
    updated_at timestamptz not null default now(),
    created_at timestamptz not null default now(),
    closed_at timestamptz,
    closed_by uuid references identity(id),
    -- Optimistic text editing: bumped by every change except a comment.
    revision bigint not null default 1 check (revision >= 1),
    comment_count integer not null default 0 check (comment_count between 0 and 1000),
    check ((state = 'open' and closed_at is null and closed_by is null)
        or (state = 'closed' and closed_at is not null and closed_by is not null)),
    unique (project_id, number),
    unique (id, project_id)
);
create index project_issue_state_number on project_issue(project_id, state, number desc);
create index project_issue_author on project_issue(author_id);
create index project_issue_assignee on project_issue(assignee_id) where assignee_id is not null;

-- Immutable once written: the runtime role has no UPDATE or DELETE privilege, and
-- the trigger refuses an UPDATE from any role.
create table project_issue_comment (
    id uuid primary key default gen_random_uuid(),
    issue_id uuid not null references project_issue(id) on delete cascade,
    seq integer not null check (seq between 1 and 1000),
    author_id uuid not null references identity(id),
    body text not null check (char_length(body) between 1 and 10000 and btrim(body) <> ''),
    created_at timestamptz not null default now(),
    unique (issue_id, seq)
);
create index project_issue_comment_author on project_issue_comment(author_id);
create trigger project_issue_comment_no_update before update on project_issue_comment
    for each row execute function event_immutable();

create table project_label (
    id uuid primary key default gen_random_uuid(),
    project_id uuid not null references project_settings(space_id) on delete cascade,
    name text not null check (char_length(name) between 1 and 50 and btrim(name) <> ''),
    color text not null check (color ~ '^[0-9a-f]{6}$'),
    description text not null default '' check (char_length(description) <= 200),
    created_by uuid not null references identity(id),
    created_at timestamptz not null default now(),
    unique (id, project_id)
);
create unique index project_label_name on project_label(project_id, lower(name));

-- The composite keys make it impossible to attach a label to an issue of another
-- project, whatever the API does.
create table project_issue_label (
    issue_id uuid not null,
    label_id uuid not null,
    project_id uuid not null,
    primary key (issue_id, label_id),
    foreign key (issue_id, project_id) references project_issue(id, project_id) on delete cascade,
    foreign key (label_id, project_id) references project_label(id, project_id) on delete cascade
);
create index project_issue_label_label on project_issue_label(label_id);

create table project_star (
    project_id uuid not null references project_settings(space_id) on delete cascade,
    identity_id uuid not null references identity(id),
    created_at timestamptz not null default now(),
    primary key (project_id, identity_id)
);
create index project_star_identity on project_star(identity_id);

revoke all on project_issue_counter, project_issue, project_issue_comment,
    project_label, project_issue_label, project_star from public;
