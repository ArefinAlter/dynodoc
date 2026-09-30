-- Copy detection, a clearer role ladder, review rules, approvals and notifications.
-- See docs/decisions/009-copies-roles-and-notifications.md. Nothing here changes
-- canonical history: fingerprints, matches and notifications are derived or
-- operational data, removed with their document by audited whole-document erasure.

-- 1. Roles. Stored document roles keep their historical names:
--      author = Editor, approver = Reviewer (new; approves change requests),
--      reviewer = Contributor ("can comment & suggest"), auditor = Viewer.
--    The document's creator (document.created_by) is its Owner. Workspace spaces add
--    approver alongside owner/manager/editor/reviewer/viewer.
alter table document_access drop constraint document_access_role_check;
alter table document_access add constraint document_access_role_check
    check (role in ('author', 'approver', 'reviewer', 'auditor'));
alter table space_member drop constraint space_member_role_check;
alter table space_member add constraint space_member_role_check
    check (role in ('owner', 'manager', 'editor', 'approver', 'reviewer', 'viewer'));

-- Engine capability is unchanged: approvers, like contributors, comment and propose.
create or replace function access_role_rank(value text) returns integer language sql immutable as $$
 select case value when 'owner' then 5 when 'manager' then 4 when 'editor' then 3 when 'author' then 3
   when 'approver' then 2 when 'reviewer' then 2 when 'viewer' then 1 when 'auditor' then 1 else 0 end
$$;
-- A finer order that tells approvers from contributors when naming a person's role.
create function member_role_order(value text) returns integer language sql immutable as $$
 select case value when 'owner' then 60 when 'manager' then 50 when 'editor' then 40 when 'author' then 40
   when 'approver' then 30 when 'reviewer' then 20 when 'viewer' then 10 when 'auditor' then 10 else 0 end
$$;
create or replace function inherited_space_role(target uuid, person uuid) returns text language sql stable as $$
 with recursive ancestors as (
 select id,parent_id from access_space where id=target
 union select s.id,s.parent_id from access_space s join ancestors a on a.parent_id=s.id
 ) select m.role from ancestors a join space_member m on m.space_id=a.id
 join identity i on i.id=m.identity_id and i.disabled_at is null
 where m.identity_id=person order by member_role_order(m.role) desc limit 1
$$;
-- owner | manager | editor | reviewer | contributor | viewer, or null without access.
create function document_member_role(target uuid, person uuid, include_trash boolean default false)
returns text language sql stable as $$
 select case when d.created_by = person then 'owner' else (
   select case max(member_role_order(r.role))
     when 60 then 'manager' when 50 then 'manager' when 40 then 'editor'
     when 30 then 'reviewer' when 20 then 'contributor' when 10 then 'viewer' end
   from (select a.role from document_access a where a.document_id=d.id and a.identity_id=person
         union all select inherited_space_role(d.space_id, person)) r) end
 from document d join identity i on i.id=person and i.disabled_at is null
 where d.id=target and (include_trash or d.deleted_at is null)
$$;

-- 2. Review rules per document, like branch protection for the team version.
create table document_policy (
    document_id uuid primary key references document(id) on delete cascade,
    -- Only the owner and managers change the team version directly; everyone else
    -- works in personal drafts and sends change requests.
    protect_team_version boolean not null default false,
    -- Approvals needed before a change request can be merged.
    required_approvals smallint not null default 0 check (required_approvals between 0 and 5),
    -- 'editors': the owner, managers and editors merge; 'owners': the owner and managers.
    merge_roles text not null default 'editors' check (merge_roles in ('editors', 'owners')),
    updated_by uuid references identity(id),
    updated_at timestamptz not null default now()
);

-- 3. Reviews of change requests. Append-only. An approval counts only while the
--    contributor has not changed the request since it was given.
alter table workspace_draft add column content_changed_at timestamptz;
create table change_request_review (
    id uuid primary key default gen_random_uuid(),
    document_id uuid not null references document(id) on delete cascade,
    draft_id uuid not null references workspace_draft(id) on delete cascade,
    reviewer_id uuid not null references identity(id),
    verdict text not null check (verdict in ('approved', 'changes_requested', 'commented')),
    -- A comment on one changed block, when set.
    node_id text check (node_id is null or length(node_id) <= 64),
    note text not null default '' check (length(note) <= 4000),
    created_at timestamptz not null default now()
);
create index change_request_review_draft_idx on change_request_review(draft_id, created_at);
create trigger change_request_review_no_update before update on change_request_review
    for each row execute function event_immutable();

-- 4. In-app notifications. Payloads hold identifiers and scores; names and titles
--    are read at display time, so erasing an account or document also removes them.
create table notification (
    id uuid primary key default gen_random_uuid(),
    recipient_id uuid not null references identity(id),
    kind text not null check (length(kind) between 1 and 64),
    document_id uuid references document(id) on delete cascade,
    actor_id uuid references identity(id),
    subject_id uuid,
    payload jsonb not null default '{}'::jsonb check (octet_length(payload::text) <= 8192),
    created_at timestamptz not null default now(),
    read_at timestamptz
);
create index notification_recipient_idx on notification(recipient_id, created_at desc);
create index notification_unread_idx on notification(recipient_id) where read_at is null;

-- 5. Content fingerprints (engine_core::similarity). Derived and rebuildable.
create table document_fingerprint (
    document_id uuid primary key references document(id) on delete cascade,
    version smallint not null,
    through_seq bigint not null,
    title text not null,
    title_key text not null,
    content_hash bigint not null,
    signature bytea not null check (octet_length(signature) in (0, 512)),
    block_count integer not null,
    substantive_count integer not null,
    word_count integer not null,
    shingle_count integer not null,
    updated_at timestamptz not null default now()
);
create index document_fingerprint_title_idx on document_fingerprint(title_key) where title_key <> '';
create index document_fingerprint_content_idx on document_fingerprint(content_hash) where content_hash <> 0;
-- Locality-sensitive hashing keys: documents sharing a key are candidate copies.
create table document_fingerprint_band (
    document_id uuid not null references document(id) on delete cascade,
    band smallint not null,
    key bigint not null,
    primary key (document_id, band)
);
create index document_fingerprint_band_key_idx on document_fingerprint_band(band, key);
-- Hashes of substantive blocks, for containment (excerpts, extended copies).
create table document_block_hash (
    document_id uuid not null references document(id) on delete cascade,
    hash bigint not null,
    primary key (document_id, hash)
);
create index document_block_hash_idx on document_block_hash(hash);

-- 6. Detected relationships. `source` arrived later (the copy); `target` is the
--    document it resembles. One row per pair, in either direction.
create table document_match (
    id uuid primary key default gen_random_uuid(),
    source_document_id uuid not null references document(id) on delete cascade,
    target_document_id uuid not null references document(id) on delete cascade,
    relation text not null,
    method text not null,
    score real not null,
    jaccard real not null,
    coverage_source real not null,
    coverage_target real not null,
    shared_blocks integer not null,
    title_match boolean not null,
    detected_at timestamptz not null default now(),
    status text not null default 'open' check (status in ('open', 'invited', 'linked', 'dismissed')),
    resolved_by uuid references identity(id),
    resolved_at timestamptz,
    check (source_document_id <> target_document_id)
);
create unique index document_match_pair_idx on document_match(
    least(source_document_id, target_document_id), greatest(source_document_id, target_document_id));
create index document_match_target_idx on document_match(target_document_id, detected_at desc);
create index document_match_source_idx on document_match(source_document_id);

-- Copies among documents that already existed when detection was installed are
-- listed on each document but not announced, so the first index does not notify
-- owners about every historical copy at once.
create table copy_detection_baseline (
    id boolean primary key default true check (id),
    started_at timestamptz not null default now()
);
insert into copy_detection_baseline default values;
