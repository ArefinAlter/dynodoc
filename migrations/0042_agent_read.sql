-- AI-agent connector, Phase 0 (read only). See docs/AI-AGENT-CONNECTORS.md section 11.2
-- and docs/AI-AGENT-PROTOCOL-SPEC.md sections 9.6 and 14. Additive only: no existing
-- row or behaviour changes, and a previous API build simply leaves these objects unused.
--
-- 1. The read scope. `content:read` joins the device-session and key-grant scope sets.
--    It is independent of the other scopes (none implies another). `content:propose`
--    and `content:submit` of the specification are NOT admitted yet; a later migration
--    widens the same checks when the write routes exist.
alter table device_session drop constraint device_session_scopes_check;
alter table device_session add constraint device_session_scopes_check check (
    cardinality(scopes) between 1 and 5
    and scopes <@ array['project:read', 'history:read', 'history:write', 'history:review', 'content:read']::text[]
);
alter table device_key_project_grant drop constraint device_key_project_grant_scopes_check;
alter table device_key_project_grant add constraint device_key_project_grant_scopes_check check (
    cardinality(scopes) between 1 and 5
    and scopes <@ array['project:read', 'history:read', 'history:write', 'history:review', 'content:read']::text[]
);

-- 2. Key class. One immutable row per key that was enrolled with an explicit class;
--    a key without a row is a person's key (every key that exists today). The class
--    is chosen at enrolment, from an authenticated browser session, and never
--    changes: an agent key can only ever obtain agent scopes and a person's key can
--    never obtain them, so a signing key file cannot be re-purposed. The label is a
--    display name chosen by the person ("Claude Desktop on laptop"); never a secret.
create table device_key_class (
    key_id uuid primary key references device_signing_key(id),
    class text not null check (class in ('person', 'agent')),
    label text check (label is null or char_length(label) between 1 and 80),
    created_at timestamptz not null default now()
);
create trigger device_key_class_no_update before update on device_key_class
    for each row execute function event_immutable();
create trigger device_key_class_no_delete before delete on device_key_class
    for each row execute function event_immutable();

-- 3. The agent call ledger (protocol spec 9.6). One row per authorised agent read:
--    which credential, which project and file, which tool, the revision served, a
--    digest of the request (method, path and query; never document text), how many
--    bytes went out and the outcome. No prompt, completion, title, cell value or
--    document text is ever stored here; `client` and `traceparent` are the caller's
--    own claims and are never used for authorisation. Rows are append-only and leave
--    only with their document, in an audited whole-document erasure (as for
--    provenance bundles); an account is anonymised, never deleted, so principal_id
--    stays valid.
create table agent_call (
    id uuid primary key default gen_random_uuid(),
    at timestamptz not null default now(),
    principal_id uuid not null references identity(id),
    key_id uuid references device_signing_key(id),
    project_id uuid not null,
    document_id uuid references document(id) on delete cascade,
    tool text not null check (tool in ('files', 'outline', 'read', 'find', 'diff', 'status')),
    rev bigint check (rev is null or rev >= 0),
    request_digest bytea not null check (octet_length(request_digest) = 32),
    bytes_out integer not null check (bytes_out >= 0),
    tok_est integer not null check (tok_est >= 0),
    outcome text not null check (outcome ~ '^[a-z_]{2,40}$'),
    client text check (client is null or char_length(client) between 1 and 120),
    traceparent text check (traceparent is null or traceparent ~ '^[0-9a-f]{2}-[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{2}$')
);
create index agent_call_project_at on agent_call(project_id, at);
create index agent_call_document on agent_call(document_id) where document_id is not null;
create trigger agent_call_no_update before update on agent_call
    for each row execute function event_immutable();
create function agent_call_erasure_guard() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
begin
    if pg_trigger_depth() > 1 and exists(
        select 1 from public.erasure_receipt r join public.document d on d.id = r.resource_id
        where r.resource_id = old.document_id and r.kind = 'documents'
          and r.transaction_id = txid_current() and d.deleted_at is not null
    ) then return old; end if;
    raise exception 'agent calls are immutable outside audited whole-document erasure';
end;
$$;
create trigger agent_call_no_delete before delete on agent_call
    for each row execute function agent_call_erasure_guard();

revoke all on device_key_class, agent_call from public;
revoke execute on function agent_call_erasure_guard() from public;
