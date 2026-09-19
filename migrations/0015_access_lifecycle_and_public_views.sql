-- Access/lifecycle metadata is separate from immutable semantic document history.
alter table identity add column disabled_at timestamptz;
alter table identity add column session_generation bigint not null default 0;
alter table refresh_token add column session_generation bigint not null default 0;
alter table magic_link add column session_generation bigint not null default 0;
alter table document add column deleted_at timestamptz;
create table access_space (
 id uuid primary key default gen_random_uuid(),
 parent_id uuid references access_space(id),
 kind text not null check(kind in ('organization','team','folder')),
 name text not null check(length(name) between 1 and 160),
 created_by uuid not null references identity(id),
 created_at timestamptz not null default now(),
 check ((kind='organization') = (parent_id is null))
);
create index access_space_parent_idx on access_space(parent_id);
create table space_member (
 space_id uuid not null references access_space(id),
 identity_id uuid not null references identity(id),
 role text not null check(role in ('owner','manager','editor','reviewer','viewer')),
 primary key(space_id,identity_id)
);
create index space_member_identity_idx on space_member(identity_id);
alter table document add column space_id uuid references access_space(id);
create index document_space_idx on document(space_id);
create function access_role_rank(value text) returns integer language sql immutable as $$
 select case value when 'owner' then 5 when 'manager' then 4 when 'editor' then 3 when 'author' then 3 when 'reviewer' then 2 when 'viewer' then 1 when 'auditor' then 1 else 0 end
$$;
create function inherited_space_role(target uuid, person uuid) returns text language sql stable as $$
 with recursive ancestors as (
 select id,parent_id from access_space where id=target
 union select s.id,s.parent_id from access_space s join ancestors a on a.parent_id=s.id
 ) select m.role from ancestors a join space_member m on m.space_id=a.id
 join identity i on i.id=m.identity_id and i.disabled_at is null
 where m.identity_id=person order by access_role_rank(m.role) desc limit 1
$$;
create function effective_document_role(target uuid, person uuid, include_trash boolean default false) returns text language sql stable as $$
 select case max(access_role_rank(r.role)) when 5 then 'author' when 4 then 'author' when 3 then 'author' when 2 then 'reviewer' when 1 then 'auditor' end
 from document d join identity i on i.id=person and i.disabled_at is null
 cross join lateral (
 select a.role from document_access a where a.document_id=d.id and a.identity_id=person
 union all select inherited_space_role(d.space_id, person)
 ) r where d.id=target and (include_trash or d.deleted_at is null)
$$;
create function can_manage_document(target uuid, person uuid) returns boolean language sql stable as $$
 select coalesce((select (d.created_by=person or access_role_rank(inherited_space_role(d.space_id,person))>=4)
 from document d join identity i on i.id=person and i.disabled_at is null where d.id=target),false)
$$;
create table public_document_view (
 id uuid primary key default gen_random_uuid(),
 document_id uuid not null references document(id),
 token_hash bytea not null unique,
 published_title text not null,
 published_kind text not null,
 snapshot_id uuid not null references snapshot(id),
 created_by uuid not null references identity(id),
 created_at timestamptz not null default now(),
 revoked_at timestamptz,
 expires_at timestamptz
);
create index public_document_view_document_idx on public_document_view(document_id);
create table operation_audit (
 id bigint generated always as identity primary key,
 actor_id uuid not null references identity(id),
 action text not null,
 resource_id uuid not null,
 detail jsonb not null default '{}'::jsonb,
 created_at timestamptz not null default now()
);
create trigger operation_audit_immutable before update or delete on operation_audit
 for each row execute function event_immutable();
