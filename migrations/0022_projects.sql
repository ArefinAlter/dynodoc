-- Independent invite-only repositories, with ordinary folders beneath them.
alter table access_space drop constraint access_space_kind_check;
alter table access_space add constraint access_space_kind_check
    check (kind in ('organization','team','folder','project'));
alter table access_space drop constraint access_space_check;
alter table access_space add constraint access_space_check
    check ((kind in ('organization','project')) = (parent_id is null));
create table project_settings (
    space_id uuid primary key references access_space(id),
    description text not null default '' check (length(description) <= 2000),
    protect_team_version boolean not null default true,
    required_approvals smallint not null default 0 check (required_approvals between 0 and 5),
    merge_roles text not null default 'editors' check (merge_roles in ('editors','owners')),
    revision bigint not null default 1 check (revision > 0)
);
create table project_watch (
    space_id uuid not null references project_settings(space_id),
    identity_id uuid not null references identity(id),
    level text not null check (level in ('requests','ignore')),
    primary key(space_id,identity_id)
);
create function document_project(target uuid) returns uuid language sql stable as $$
    with recursive ancestors as (
        select s.id,s.parent_id,s.kind from document d join access_space s on s.id=d.space_id where d.id=target
        union select s.id,s.parent_id,s.kind from access_space s join ancestors a on a.parent_id=s.id
    ) select id from ancestors where kind='project' limit 1
$$;
