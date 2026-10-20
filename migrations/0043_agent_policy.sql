-- AI-agent connector, Phase 1: project agent policy v0 (docs/AI-AGENT-CONNECTORS.md
-- sections 5.1 to 5.3 and 7.2). Additive only: no existing row or behaviour changes, and a
-- previous API build simply leaves these objects unused.
--
-- The policy is the project owner's switch for AI agents. A project with NO row is
-- disabled: the read routes of migration 0042 refuse every agent request until an owner or
-- manager writes a row with enabled = true. The application re-reads the row on every
-- request; nothing is cached.
--
-- History. Both tables hold only the CURRENT value and are edited in place by the runtime
-- login (select, insert and update; never delete). The history of every change is the
-- operation_audit row written in the same transaction (action agent.policy.changed,
-- agent.file_policy.changed or agent.killswitch: actor, the fields that changed and their
-- values before and after), and operation_audit is append-only. No policy value is text a
-- person typed, so the audit rows keep the values readable.
--
-- 1. The project policy. Only the fields of spec 5.2 that the read path can enforce today
--    exist: enabled, kinds, the call and byte budgets and revision. min_role is a floor on
--    the member's project role, which is not a spec 5.2 field: every role may read
--    (spec 14.3), so a ceiling on reads cannot be expressed and the floor is the one role
--    rule a read can enforce. Left out until the routes that need them exist: write_mode,
--    submit_requires_person, clients.*, scopes_max (only content:read exists), the
--    proposal budgets, dlp.*, egress.* and retention.call_days.
create table project_agent_policy (
    project_id uuid primary key references project_settings(space_id) on delete cascade,
    enabled boolean not null default false,
    -- Lowest project role whose agents may read (product vocabulary of document_member_role).
    min_role text not null default 'viewer'
        check (min_role in ('viewer', 'contributor', 'reviewer', 'editor', 'manager', 'owner')),
    -- File kinds agents may read; at least one. The application stores them sorted.
    kinds text[] not null default array['document', 'spreadsheet', 'presentation']::text[]
        check (cardinality(kinds) between 1 and 3
               and kinds <@ array['document', 'spreadsheet', 'presentation']::text[]),
    -- Per grant (one key in this project). Counted from agent_call.
    calls_per_minute integer not null default 60 check (calls_per_minute between 1 and 600),
    read_bytes_per_day bigint not null default 20000000
        check (read_bytes_per_day between 10000 and 1000000000),
    -- Optimistic concurrency, as project_settings.revision. A new row is revision 1; the
    -- application treats "no row" as revision 0.
    revision bigint not null default 1 check (revision >= 1),
    updated_by uuid not null references identity(id),
    updated_at timestamptz not null default now()
);

-- 2. The per-file override (spec 5.3 agent_access). No row, and inherit, mean the project
--    policy decides. none hides the file from agents entirely (404, as if it did not
--    exist). read is accepted and stored, but means nothing yet because no write route
--    exists: it can never widen what the project allows. propose (spec) is not admitted.
create table file_agent_policy (
    document_id uuid primary key references document(id) on delete cascade,
    access text not null check (access in ('inherit', 'none', 'read')),
    updated_by uuid not null references identity(id),
    updated_at timestamptz not null default now()
);

create function project_agent_policy_guard() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
begin
    if new.project_id is distinct from old.project_id then
        raise exception 'project_agent_policy.project_id cannot change';
    end if;
    if new.revision is distinct from old.revision + 1 then
        raise exception 'project_agent_policy.revision must increase by exactly one';
    end if;
    return new;
end $$;
create trigger project_agent_policy_revision before update on project_agent_policy
    for each row execute function project_agent_policy_guard();

create function file_agent_policy_guard() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
begin
    if new.document_id is distinct from old.document_id then
        raise exception 'file_agent_policy.document_id cannot change';
    end if;
    return new;
end $$;
create trigger file_agent_policy_key_fixed before update on file_agent_policy
    for each row execute function file_agent_policy_guard();

-- 3. The per-grant budgets read the ledger on every admitted request (calls in the last
--    minute, bytes since midnight UTC), per key and project. Covering, so the sum is an
--    index-only scan.
create index agent_call_key_project_at on agent_call(key_id, project_id, at) include (bytes_out);

revoke all on project_agent_policy, file_agent_policy from public;
revoke execute on function project_agent_policy_guard(), file_agent_policy_guard() from public;
