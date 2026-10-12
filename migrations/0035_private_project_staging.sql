-- Private transfer area. No branch, review, event or public snapshot is changed.
-- Object bytes remain shared per document; upload acknowledgements confer access
-- only to bytes this account has supplied in full, never to guessed hashes.
create table project_staging_storage (
    identity_id uuid primary key references identity(id),
    bytes_used bigint not null default 0 check(bytes_used>=0),
    objects_used bigint not null default 0 check(objects_used>=0),
    commits_used bigint not null default 0 check(commits_used>=0),
    byte_limit bigint not null default 4294967296 check(byte_limit>=bytes_used),
    object_limit bigint not null default 1000000 check(object_limit>=objects_used),
    commit_limit bigint not null default 100000 check(commit_limit>=commits_used)
);
create table project_staged_object (
    identity_id uuid not null references identity(id),
    project_id uuid not null references project_settings(space_id),
    document_id uuid not null references document(id),
    address text not null,
    charged_bytes bigint not null check(charged_bytes between 1 and 1048576),
    primary key(identity_id,project_id,document_id,address),
    foreign key(document_id,address) references checkpoint_object(document_id,address)
);
create index project_staged_object_document on project_staged_object(document_id);
create table project_staged_commit (
    identity_id uuid not null references identity(id),
    project_id uuid not null references project_settings(space_id),
    address text not null check(address ~ '^[0-9a-f]{64}$'),
    body bytea not null check(octet_length(body) between 1 and 1049600),
    -- Includes all dependency scopes, even removed files. Current authorization
    -- and whole-document erasure cannot be evaded by a descendant empty tree.
    scope uuid[] not null check(cardinality(scope)<=4096 and array_position(scope,null) is null),
    height bigint not null check(height between 0 and 100000),
    device_id uuid not null references device_signing_key(id),
    charged_bytes bigint generated always as (octet_length(body)::bigint+16*cardinality(scope)) stored,
    primary key(identity_id,project_id,address)
);
create index project_staged_commit_scope on project_staged_commit using gin(scope);

create function account_project_staging() returns trigger
language plpgsql security definer set search_path=pg_catalog,public,pg_temp as $$
declare actor uuid; amount bigint; objects bigint; commits bigint;
begin
    if tg_op='INSERT' then actor:=new.identity_id; amount:=new.charged_bytes;
    else actor:=old.identity_id; amount:=-old.charged_bytes; end if;
    objects:=case when tg_table_name='project_staged_object' then 1 else 0 end;
    commits:=1-objects;
    if tg_op='DELETE' then objects:=-objects; commits:=-commits; end if;
    insert into public.project_staging_storage(identity_id) values(actor) on conflict do nothing;
    update public.project_staging_storage
        set bytes_used=bytes_used+amount,objects_used=objects_used+objects,commits_used=commits_used+commits
        where identity_id=actor and amount<=byte_limit-bytes_used
          and objects<=object_limit-objects_used and commits<=commit_limit-commits_used;
    if not found then raise exception 'private project history storage budget reached'
        using errcode='23514',constraint='project_staging_quota'; end if;
    return null;
end $$;

create function project_staging_delete_guard() returns trigger
language plpgsql set search_path=pg_catalog,public,pg_temp as $$
declare documents uuid[];
begin
    if tg_table_name='project_staged_object' then documents:=array[old.document_id];
    else documents:=old.scope; end if;
    if pg_trigger_depth()>1 and exists (
        select 1 from public.erasure_receipt r
        where r.transaction_id=txid_current() and (
            (r.kind='users' and r.resource_id=old.identity_id) or
            (r.kind='documents' and r.resource_id=any(documents) and exists (
                select 1 from public.document d where d.id=r.resource_id and d.deleted_at is not null))
        )
    ) then return old; end if;
    raise exception 'private history is immutable outside audited erasure';
end $$;
create trigger staged_object_account after insert or delete on project_staged_object
    for each row execute function account_project_staging();
create trigger staged_commit_account after insert or delete on project_staged_commit
    for each row execute function account_project_staging();
create trigger staged_object_no_update before update on project_staged_object
    for each row execute function event_immutable();
create trigger staged_commit_no_update before update on project_staged_commit
    for each row execute function event_immutable();
create trigger staged_object_no_delete before delete on project_staged_object
    for each row execute function project_staging_delete_guard();
create trigger staged_commit_no_delete before delete on project_staged_commit
    for each row execute function project_staging_delete_guard();
create trigger staged_object_no_truncate before truncate on project_staged_object
    for each statement execute function event_immutable();
create trigger staged_commit_no_truncate before truncate on project_staged_commit
    for each statement execute function event_immutable();

create function erase_project_staging() returns trigger
language plpgsql security definer set search_path=pg_catalog,public,pg_temp as $$
begin
    if tg_table_name='document' then
        delete from public.project_staged_commit where scope @> array[old.id];
        delete from public.project_staged_object where document_id=old.id;
        return old;
    end if;
    if old.erased_at is null and new.erased_at is not null then
        delete from public.project_staged_commit where identity_id=old.id;
        delete from public.project_staged_object where identity_id=old.id;
    end if;
    return new;
end $$;
-- Alphabetically before document_shared_checkpoint_erasure (which deletes bytes).
create trigger document_project_staging_erasure before delete on document
    for each row execute function erase_project_staging();
create trigger identity_project_staging_erasure before update on identity
    for each row execute function erase_project_staging();
revoke all on project_staging_storage,project_staged_object,project_staged_commit from public;
revoke execute on function account_project_staging(),project_staging_delete_guard(),erase_project_staging() from public;
