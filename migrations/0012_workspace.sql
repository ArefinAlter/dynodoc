-- Named immutable checkpoints and personal drafts. Canonical edits still enter event.
create table document_version (
    id uuid primary key default gen_random_uuid(),
    document_id uuid not null references document(id),
    snapshot_id uuid not null references snapshot(id),
    name text not null check (length(name) between 1 and 200),
    note text not null default '',
    created_by uuid not null references identity(id),
    created_at timestamptz not null default now()
);
create index document_version_document_idx on document_version(document_id, created_at);
create trigger document_version_no_update before update or delete on document_version
    for each row execute function event_immutable();

create table workspace_draft (
    id uuid primary key default gen_random_uuid(),
    document_id uuid not null references document(id),
    name text not null check (length(name) between 1 and 200),
    base_seq bigint not null,
    base_state jsonb not null,
    created_by uuid not null references identity(id),
    created_at timestamptz not null default now(),
    merged_at timestamptz
);
create table draft_edit (
    id bigint generated always as identity primary key,
    draft_id uuid not null references workspace_draft(id),
    payload jsonb not null,
    actor_id uuid not null references identity(id),
    created_at timestamptz not null default now()
);
create index draft_edit_order_idx on draft_edit(draft_id, id);
create trigger draft_edit_no_update before update or delete on draft_edit
    for each row execute function event_immutable();

create table document_upload (
    id uuid primary key default gen_random_uuid(),
    document_id uuid not null references document(id),
    filename text not null,
    content_type text not null,
    content bytea not null,
    report jsonb not null default '{}'::jsonb,
    created_by uuid not null references identity(id),
    created_at timestamptz not null default now(),
    check(octet_length(content) <= 20971520)
);

create table auth_request_window (
    key_hash bytea primary key,
    started_at timestamptz not null default now(),
    attempts integer not null default 1
);
