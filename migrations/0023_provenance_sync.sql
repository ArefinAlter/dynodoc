-- Editor credentials cannot write canonical history or access another file.
create table connector_grant (
    id uuid primary key default gen_random_uuid(),
    document_id uuid not null references document(id) on delete cascade,
    identity_id uuid not null references identity(id),
    token_hash bytea not null unique check (octet_length(token_hash)=32),
    session_generation bigint not null,
    host text not null check (host in ('word','google-docs')),
    created_at timestamptz not null default now(),
    expires_at timestamptz not null default now()+interval '7 days',
    revoked_at timestamptz
);
create index connector_grant_document_idx on connector_grant(document_id,identity_id);
create table provenance_bundle (
    document_id uuid not null references document(id) on delete cascade,
    bundle_id uuid not null,
    draft_id uuid not null references workspace_draft(id) on delete cascade,
    actor_id uuid not null references identity(id),
    body jsonb not null check (octet_length(body::text)<=16777216),
    digest bytea not null check (octet_length(digest)=32),
    receipt jsonb not null,
    received_at timestamptz not null default now(),
    primary key(document_id,bundle_id)
);
create trigger provenance_bundle_no_update before update on provenance_bundle
    for each row execute function event_immutable();
create function provenance_bundle_erasure_guard() returns trigger
language plpgsql set search_path=pg_catalog,public as $$
begin
    if pg_trigger_depth()>1 and (
        exists(select 1 from public.erasure_receipt r join public.document d on d.id=r.resource_id
            where r.resource_id=old.document_id and r.kind='documents' and r.transaction_id=txid_current() and d.deleted_at is not null)
        or exists(select 1 from public.erasure_receipt where resource_id=old.actor_id
            and kind='users' and transaction_id=txid_current())
    ) then return old; end if;
    raise exception 'provenance is immutable outside audited whole-resource erasure';
end;
$$;
create trigger provenance_bundle_no_delete before delete on provenance_bundle
    for each row execute function provenance_bundle_erasure_guard();
