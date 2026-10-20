-- AI-agent connector, Phase 1: the review-gated write path (docs/AI-AGENT-CONNECTORS.md
-- sections 4.1, 6 and 7.3, docs/AI-AGENT-PROTOCOL-SPEC.md sections 9.5 and 9.6). Additive
-- only: no existing row changes. As for 0043 this is not rollback-safe by leaving the
-- objects unused (the API refuses a newer schema; rolling back means restoring the dump).
-- Two function bodies of earlier migrations are replaced (document_erasure_child_guard,
-- below), with their ACLs and trigger bindings kept.
--
-- What an agent can do after this migration: nothing, until a project owner sets
-- write_mode (default 'none') and a person grants a key a write scope. And what it can do
-- then is stage a draft: no table below is read by the event log, and no route of the
-- write path appends to it.
--
-- 1. The write scopes. content:propose and content:submit join the device-session and
--    key-grant scope sets as independent scopes (none implies another). Only an agent key
--    can hold them; the application enforces that, as for content:read in 0042.
alter table device_session drop constraint device_session_scopes_check;
alter table device_session add constraint device_session_scopes_check check (
    cardinality(scopes) between 1 and 7
    and scopes <@ array['project:read', 'history:read', 'history:write', 'history:review',
                        'content:read', 'content:propose', 'content:submit']::text[]
);
alter table device_key_project_grant drop constraint device_key_project_grant_scopes_check;
alter table device_key_project_grant add constraint device_key_project_grant_scopes_check check (
    cardinality(scopes) between 1 and 7
    and scopes <@ array['project:read', 'history:read', 'history:write', 'history:review',
                        'content:read', 'content:propose', 'content:submit']::text[]
);

-- 2. The client product of an agent key, chosen by the person at enrolment, beside the
--    class (device_key_class is immutable: the column is written by the enrolment insert
--    and never changes). Null means 'device', the spec's value for a local key whose
--    product is unknown. A claim, never authority: it only names the agent identity.
alter table device_key_class add column client_kind text
    check (client_kind is null or client_kind in
        ('claude', 'chatgpt', 'gemini', 'copilot', 'cursor', 'codex', 'device', 'generic'));

-- 3. The agent identity (spec 4.1). An identity row, so that workspace_draft.created_by,
--    draft_edit.actor_id and operation_audit.actor_id can name it, with this sidecar. It is
--    born DISABLED (identity.disabled_at is set when the row is made and a trigger refuses
--    to clear it), which is what keeps it out of every sign-in, session, magic-link and
--    notification path by construction: they all test disabled_at. Its address is
--    agent+<id>@agents.invalid, in a domain no real mailbox can have; the guards below make
--    sure that nothing else can ever hold such an address. It holds no document_access or
--    space_member row and may never review (triggers, section 4).
--    One live identity per (principal, client product); revoked_at moves once, null to a
--    time, when the principal is disabled or erased (section 5). A revoked identity keeps
--    its drafts, which its principal (and reviewers, once submitted) still see.
create table agent_identity (
    identity_id uuid primary key references identity(id),
    principal_id uuid not null references identity(id),
    client_kind text not null check (client_kind in
        ('claude', 'chatgpt', 'gemini', 'copilot', 'cursor', 'codex', 'device', 'generic')),
    -- The key that first needed the identity: a claim for the audit trail, never authority.
    client_ref text not null check (char_length(client_ref) between 1 and 100),
    created_at timestamptz not null default now(),
    revoked_at timestamptz,
    check (identity_id <> principal_id)
);
create unique index agent_identity_live on agent_identity(principal_id, client_kind)
    where revoked_at is null;
create index agent_identity_principal on agent_identity(principal_id);

create function agent_identity_guard() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
begin
    if tg_op = 'DELETE' then
        raise exception 'agent identities are retained: revoke, never delete';
    elsif tg_op = 'INSERT' then
        -- The identity row must be the one this sidecar describes: disabled, unerased, with
        -- the reserved address; the principal must be an active person, not an agent.
        if not exists(select 1 from public.identity i
                      where i.id = new.identity_id and i.disabled_at is not null
                        and i.erased_at is null
                        and i.email = 'agent+' || new.identity_id::text || '@agents.invalid') then
            raise exception 'an agent identity needs a disabled identity row with the reserved address';
        end if;
        if exists(select 1 from public.agent_identity a where a.identity_id = new.principal_id)
           or not exists(select 1 from public.identity p
                         where p.id = new.principal_id and p.disabled_at is null
                           and p.erased_at is null) then
            raise exception 'an agent identity acts for an active person';
        end if;
        return new;
    end if;
    if new.identity_id is distinct from old.identity_id
       or new.principal_id is distinct from old.principal_id
       or new.client_kind is distinct from old.client_kind
       or new.client_ref is distinct from old.client_ref
       or new.created_at is distinct from old.created_at
       or old.revoked_at is not null
       or new.revoked_at is null then
        raise exception 'an agent identity can only be revoked, once';
    end if;
    return new;
end $$;
create trigger agent_identity_guard before insert or update or delete on agent_identity
    for each row execute function agent_identity_guard();

-- The reserved domain is for agent identities and for nothing else: not at sign-up, not by
-- sharing a file with an invented address, not by editing an address later. Checked at
-- commit (the identity row is made before its sidecar, in one transaction). The pattern
-- covers the domain, its subdomains and the trailing-dot spelling.
create function agent_identity_domain_guard() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
declare
    reserved boolean := lower(new.email) ~ '@([^@]*[.])?agents[.]invalid[.]?$';
    sidecar boolean := exists(select 1 from public.agent_identity a where a.identity_id = new.id);
begin
    if reserved and not (sidecar and new.email = 'agent+' || new.id::text || '@agents.invalid') then
        raise exception 'the agents.invalid address domain is reserved for agent identities';
    end if;
    if sidecar and not reserved then
        raise exception 'an agent identity keeps its reserved address';
    end if;
    return null;
end $$;
create constraint trigger identity_agent_domain after insert or update of email on identity
    deferrable initially deferred for each row execute function agent_identity_domain_guard();

-- Once an agent identity, never an account: it cannot be enabled (that would make it a
-- sign-in-able person) or erased directly (its erasure is its principal's).
create function agent_identity_account_guard() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
begin
    if exists(select 1 from public.agent_identity a where a.identity_id = old.id)
       and (new.disabled_at is null or new.erased_at is distinct from old.erased_at) then
        raise exception 'an agent identity cannot be enabled or erased as an account';
    end if;
    return new;
end $$;
create trigger identity_agent_account before update on identity
    for each row execute function agent_identity_account_guard();

-- 4. An agent is no member and no reviewer (invariant I4), by construction as well as by the
--    explicit checks of the application: none of these tables can name one.
create function refuse_agent_identity() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
begin
    if exists(select 1 from public.agent_identity a
              where a.identity_id::text = (to_jsonb(new) ->> tg_argv[0])) then
        raise exception 'an agent identity cannot be a member, reviewer or key holder (%.%)',
            tg_table_name, tg_argv[0];
    end if;
    return new;
end $$;
create trigger document_access_no_agent before insert or update of identity_id on document_access
    for each row execute function refuse_agent_identity('identity_id');
create trigger space_member_no_agent before insert or update of identity_id on space_member
    for each row execute function refuse_agent_identity('identity_id');
create trigger change_request_review_no_agent before insert on change_request_review
    for each row execute function refuse_agent_identity('reviewer_id');
create trigger connector_grant_no_agent before insert on connector_grant
    for each row execute function refuse_agent_identity('identity_id');
create trigger device_signing_key_no_agent before insert on device_signing_key
    for each row execute function refuse_agent_identity('identity_id');

-- 5. The principal's removal ends the agent identity, in the family of
--    revoke_removed_account_devices (0033), which has already ended the person's keys.
--    Disabling or erasing the person revokes their live agent identities; erasing also
--    scrubs the display name (it names the person) and, below, deletes their drafts.
create function revoke_removed_account_agents() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
begin
    if (new.disabled_at is not null and old.disabled_at is null)
       or (new.erased_at is not null and old.erased_at is null) then
        update public.agent_identity set revoked_at = now()
            where principal_id = new.id and revoked_at is null;
    end if;
    if new.erased_at is not null and old.erased_at is null then
        update public.identity set display_name = 'Deleted agent'
            where id in (select identity_id from public.agent_identity where principal_id = new.id);
    end if;
    return new;
end $$;
create trigger identity_revoke_agents after update on identity
    for each row execute function revoke_removed_account_agents();

-- An account's erasure deletes its drafts (erased_identity_guard, 0017); the drafts of its
-- agent identities are the same person's private work and go with them. Like that guard
-- this runs as its owner, only inside the erasure transaction, and only after the guard
-- has accepted the receipt (triggers fire in name order: erased_identity_immutable first).
create function erase_agent_children() returns trigger
language plpgsql security definer set search_path = pg_catalog, public, pg_temp as $$
begin
    if old.erased_at is null and new.erased_at is not null
       and exists(select 1 from public.erasure_receipt where resource_id = old.id
                  and kind = 'users' and transaction_id = txid_current()) then
        delete from public.draft_edit where draft_id in (
            select d.id from public.workspace_draft d
            join public.agent_identity a on a.identity_id = d.created_by
            where a.principal_id = old.id);
        delete from public.workspace_draft where created_by in (
            select identity_id from public.agent_identity where principal_id = old.id);
    end if;
    return new;
end $$;
create trigger identity_agent_erasure before update on identity
    for each row execute function erase_agent_children();

-- The draft_edit rows above may leave only in the erasure of the person who owns the draft
-- or, new here, of the principal of the agent that does. This is the 0017 function with one
-- added clause (marked); ACL and trigger bindings are those of 0017, 0025 and 0028.
create or replace function document_erasure_child_guard() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
declare target uuid;
begin
    if tg_table_name = 'draft_edit' then
        select document_id into target from public.workspace_draft where id=old.draft_id;
        if pg_trigger_depth() > 1 and exists (
            select 1 from public.erasure_receipt r join public.workspace_draft d on d.created_by=r.resource_id
            where d.id=old.draft_id and r.kind='users' and r.transaction_id=txid_current()
        ) then
            return old;
        end if;
        -- 0044: the drafts of the agent identities of an erased person.
        if pg_trigger_depth() > 1 and exists (
            select 1 from public.erasure_receipt r
            join public.agent_identity a on a.principal_id=r.resource_id
            join public.workspace_draft d on d.created_by=a.identity_id
            where d.id=old.draft_id and r.kind='users' and r.transaction_id=txid_current()
        ) then
            return old;
        end if;
    else
        target := old.document_id;
    end if;
    if tg_op = 'DELETE' and pg_trigger_depth() > 1 and exists (
        select 1 from public.erasure_receipt r join public.document d on d.id=r.resource_id
        where r.resource_id=target and r.kind='documents'
          and r.transaction_id=txid_current() and d.deleted_at is not null
    ) then
        return old;
    end if;
    raise exception 'history is immutable outside an audited whole-document erasure';
end;
$$;

-- 6. Proposal receipts (spec 9.3 and 9.6). One append-only row per propose call that
--    stored edits, and one per submit and withdraw that changed state: provenance, kept as
--    long as the draft. The idempotency key is (agent identity, request_id): the same pair
--    with the same request_digest is a replay of this receipt, with another digest a
--    conflict. A proposal is a workspace_draft (its id is the proposal id; the
--    server chooses it, not the client, so that a client's request ids can never collide
--    with, or reveal, another client's drafts); the receipts hold no document text. `why`
--    is the agent's own reason per op (at most 280 characters each, control and hidden
--    characters refused on the way in), shown to reviewers labelled as the agent's
--    statement, never as an instruction. Rows leave only with their draft or document, in
--    an audited erasure.
create table agent_proposal_receipt (
    id uuid primary key default gen_random_uuid(),
    draft_id uuid not null references workspace_draft(id) on delete cascade,
    document_id uuid not null references document(id) on delete cascade,
    project_id uuid not null,
    agent_identity_id uuid not null references agent_identity(identity_id),
    principal_id uuid not null references identity(id),
    key_id uuid references device_signing_key(id),
    kind text not null check (kind in ('propose', 'submit', 'withdraw')),
    -- propose only: the idempotency key; true on the call that opened the proposal.
    request_id uuid,
    opened boolean not null default false,
    -- The revision the agent's refs came from, the draft's base, and the team head.
    rev bigint not null check (rev >= 0),
    base_seq bigint not null check (base_seq >= 0),
    head_seq bigint not null check (head_seq >= 0),
    ops_in integer not null default 0 check (ops_in between 0 and 200),
    ops_engine integer not null default 0 check (ops_engine between 0 and 10000),
    stale integer not null default 0 check (stale >= 0),
    stale_refs text[] not null default '{}'::text[]
        check (cardinality(stale_refs) <= 5),
    -- The draft's revision after the call.
    revision bigint not null check (revision >= 0),
    request_digest bytea not null check (octet_length(request_digest) = 32),
    outcome text not null check (outcome ~ '^[a-z_]{2,40}$'),
    why jsonb not null default '[]'::jsonb
        check (jsonb_typeof(why) = 'array' and octet_length(why::text) <= 65536),
    at timestamptz not null default now(),
    check ((kind = 'propose') = (request_id is not null)),
    check (kind = 'propose' or not opened)
);
create unique index agent_proposal_receipt_request on agent_proposal_receipt(agent_identity_id, request_id)
    where request_id is not null;
create index agent_proposal_receipt_draft on agent_proposal_receipt(draft_id, at);
-- The proposal budgets count the identity's openings: per UTC day and still open.
create index agent_proposal_receipt_opened on agent_proposal_receipt(agent_identity_id, project_id, at)
    where opened;
create trigger agent_proposal_receipt_no_update before update on agent_proposal_receipt
    for each row execute function event_immutable();
create function agent_proposal_receipt_erasure_guard() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
begin
    -- Arrives as the foreign-key cascade of a draft or document delete, which their own
    -- erasure triggers allow only with a same-transaction receipt. The nesting depth rules
    -- out a direct DELETE.
    if pg_trigger_depth() > 1 and (
        exists(select 1 from public.erasure_receipt r where r.resource_id = old.document_id
               and r.kind = 'documents' and r.transaction_id = txid_current())
        or exists(select 1 from public.erasure_receipt r where r.resource_id = old.principal_id
                  and r.kind = 'users' and r.transaction_id = txid_current())
    ) then
        return old;
    end if;
    raise exception 'agent proposal receipts are immutable outside audited whole-resource erasure';
end $$;
create trigger agent_proposal_receipt_no_delete before delete on agent_proposal_receipt
    for each row execute function agent_proposal_receipt_erasure_guard();

-- 7. The call ledger learns the write tools and, for them, what was proposed (counts and the
--    proposal id; never text, why or ops). status of a proposal is the existing 'status'.
alter table agent_call drop constraint agent_call_tool_check;
alter table agent_call add constraint agent_call_tool_check
    check (tool in ('files', 'outline', 'read', 'find', 'diff', 'status', 'propose', 'submit'));
alter table agent_call
    add column proposal_id uuid,
    add column ops_in integer check (ops_in is null or ops_in >= 0),
    add column ops_engine integer check (ops_engine is null or ops_engine >= 0),
    add column bytes_in integer check (bytes_in is null or bytes_in >= 0);

-- 8. The project policy learns the write fields (spec 5.2). Writing is OFF by default:
--    write_mode 'none' means a project that enables agents for reading enables nothing
--    else, and the kill switch sets it back to 'none' (application). 'draft': proposals
--    stay private drafts until a person submits. 'change_request': an agent may submit,
--    unless submit_requires_person (default true) says a person must. The proposal budgets
--    count per agent identity and project (stricter than per key: more keys of one product
--    do not multiply them): proposals opened per UTC day, proposals still open, and engine
--    operations in one proposal (hard cap 10,000, the compiler's own). The spec's defaults
--    are used; they apply only once write_mode is set. The existing triggers (revision
--    moves by one, project fixed, cut-off never moves back) cover the new columns.
alter table project_agent_policy
    add column write_mode text not null default 'none'
        check (write_mode in ('none', 'draft', 'change_request')),
    add column submit_requires_person boolean not null default true,
    add column proposals_per_day integer not null default 20
        check (proposals_per_day between 0 and 1000),
    add column open_proposals integer not null default 5
        check (open_proposals between 0 and 100),
    add column ops_per_proposal integer not null default 500
        check (ops_per_proposal between 1 and 10000);

revoke all on agent_identity, agent_proposal_receipt from public;
revoke execute on function agent_identity_guard(), agent_identity_domain_guard(),
    agent_identity_account_guard(), refuse_agent_identity(), revoke_removed_account_agents(),
    erase_agent_children(), agent_proposal_receipt_erasure_guard() from public;
