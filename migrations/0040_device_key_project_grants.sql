-- Per-project grants for device signing keys. See docs/DEVICE-KEY-GRANTS-API.md.
--
-- Before this migration an enrolled key could open a session for any project its
-- account could reach, and the key holder chose the project and scopes. The account
-- holder now decides, from an authenticated browser session, which projects and
-- scopes a key may use.
--
-- device_signing_key is a retained, immutable key binding (immutable_update trigger,
-- no runtime UPDATE/DELETE); a mutable policy does not belong inside it. The mode is
-- therefore one small row per key in device_key_access, beside the binding.
--
-- A key with NO row is treated as 'granted' by the application: an unknown key can
-- reach nothing. The rows inserted below keep every key that already exists on its
-- former account-wide behaviour; keys enrolled afterwards get an explicit row from
-- the enrolment transaction ('granted' unless the person asked for 'all').
create table device_key_access (
    key_id uuid primary key references device_signing_key(id),
    project_access text not null check (project_access in ('all', 'granted')),
    updated_at timestamptz not null default now()
);
insert into device_key_access(key_id, project_access)
    select id, 'all' from device_signing_key;

-- One row per grant. A row is never deleted by the runtime login and, once written,
-- only its revoked_at may move (null -> a timestamp, once): changing the scopes of a
-- project is a revocation plus a new row, so the history of what a key could do
-- stays in the table as well as in operation_audit. At most one ACTIVE grant per
-- (key, project). The application caps a key at 50 active grants.
create table device_key_project_grant (
    id uuid primary key default gen_random_uuid(),
    key_id uuid not null references device_signing_key(id),
    project_id uuid not null references project_settings(space_id) on delete cascade,
    scopes text[] not null check (
        cardinality(scopes) between 1 and 4
        and scopes <@ array['project:read', 'history:read', 'history:write', 'history:review']::text[]
    ),
    created_at timestamptz not null default now(),
    created_by uuid not null references identity(id),
    revoked_at timestamptz,
    check (revoked_at is null or revoked_at >= created_at)
);
create unique index device_key_project_grant_active
    on device_key_project_grant(key_id, project_id) where revoked_at is null;
create index device_key_project_grant_project on device_key_project_grant(project_id);

-- The session ledger records the scopes of each issuance; it must accept the new
-- review scope. Existing rows already satisfy the wider check.
alter table device_session drop constraint device_session_scopes_check;
alter table device_session add constraint device_session_scopes_check check (
    cardinality(scopes) between 1 and 4
    and scopes <@ array['project:read', 'history:read', 'history:write', 'history:review']::text[]
);

create function device_key_access_guard() returns trigger language plpgsql as $$
begin
    if new.key_id is distinct from old.key_id then
        raise exception 'device_key_access.key_id cannot change';
    end if;
    return new;
end $$;
create trigger device_key_access_key_fixed before update on device_key_access
    for each row execute function device_key_access_guard();

create function device_key_project_grant_guard() returns trigger language plpgsql as $$
begin
    if new.id is distinct from old.id
        or new.key_id is distinct from old.key_id
        or new.project_id is distinct from old.project_id
        or new.scopes is distinct from old.scopes
        or new.created_at is distinct from old.created_at
        or new.created_by is distinct from old.created_by
        or old.revoked_at is not null
        or new.revoked_at is null then
        raise exception 'device key grants can only be revoked, once';
    end if;
    return new;
end $$;
create trigger device_key_project_grant_revoke_only before update on device_key_project_grant
    for each row execute function device_key_project_grant_guard();

revoke all on device_key_access, device_key_project_grant from public;
revoke execute on function device_key_access_guard(), device_key_project_grant_guard() from public;
