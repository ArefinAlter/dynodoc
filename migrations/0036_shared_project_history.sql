-- Explicitly published portable history is separate from account-private staging.
-- Published envelopes survive account erasure; whole-document erasure removes
-- every envelope whose transitive scope mentions that document. Ref generations
-- remain as opaque audit commitments, never silently falling back to older heads.
create table project_history_storage (
    project_id uuid primary key references project_settings(space_id),
    bytes_used bigint not null default 0 check(bytes_used>=0),
    rows_used bigint not null default 0 check(rows_used>=0),
    branches_used bigint not null default 0 check(branches_used>=0),
    byte_limit bigint not null default 1073741824 check(byte_limit>=bytes_used),
    row_limit bigint not null default 1000000 check(row_limit>=rows_used),
    branch_limit bigint not null default 4096 check(branch_limit>=branches_used)
);
create table project_published_commit (
    project_id uuid not null references project_settings(space_id),
    address text not null check(address ~ '^[0-9a-f]{64}$'),
    parents text[] not null check(cardinality(parents)<=2 and array_position(parents,null) is null),
    body bytea not null check(octet_length(body) between 1 and 1049600),
    scope uuid[] not null check(cardinality(scope)<=256 and array_position(scope,null) is null),
    height bigint not null check(height between 0 and 100000),
    device_id uuid not null references device_signing_key(id),
    publisher uuid not null references identity(id),
    published_at timestamptz not null default now(),
    charged_bytes bigint generated always as (octet_length(body)::bigint+16*cardinality(scope)+64*cardinality(parents)+128) stored,
    primary key(project_id,address)
);
create index project_published_commit_scope on project_published_commit using gin(scope);
create table project_published_object (
    project_id uuid not null,
    state_root text not null check(state_root ~ '^[0-9a-f]{64}$'),
    document_id uuid not null references document(id),
    address text not null,
    charged_bytes bigint not null default 160 check(charged_bytes=160),
    primary key(project_id,state_root,document_id,address),
    foreign key(document_id,address) references checkpoint_object(document_id,address)
);
create index project_published_object_document on project_published_object(document_id);
create table project_history_review (
    id bigint generated always as identity primary key,
    project_id uuid not null,
    candidate text not null,
    branch text not null check(octet_length(branch) between 1 and 200),
    expected_generation bigint not null check(expected_generation>=0),
    expected_commit text check(expected_commit ~ '^[0-9a-f]{64}$'),
    reviewer uuid not null references identity(id),
    decision text not null check(decision in ('approve','request_changes')),
    created_at timestamptz not null default now(),
    charged_bytes bigint generated always as (320+octet_length(branch)::bigint) stored,
    foreign key(project_id,candidate) references project_published_commit(project_id,address),
    check((expected_generation=0)=(expected_commit is null))
);
create index project_history_review_target on project_history_review(project_id,branch,expected_generation,candidate,reviewer,id desc);
-- Explicit operator installation; the runtime can only read the public binding.
create table project_ref_authority (
    singleton boolean primary key default true check(singleton),
    public_key text not null check(public_key ~ '^[0-9a-f]{64}$')
);
create trigger ref_authority_no_change before update or delete on project_ref_authority
    for each row execute function event_immutable();
create trigger ref_authority_no_truncate before truncate on project_ref_authority
    for each statement execute function event_immutable();
create table project_ref_event (
    project_id uuid not null references project_settings(space_id),
    branch text not null check(octet_length(branch) between 1 and 200),
    generation bigint not null check(generation>=1),
    commit_address text not null check(commit_address ~ '^[0-9a-f]{64}$'),
    receipt bytea not null check(octet_length(receipt) between 1 and 4096),
    receipt_address text not null check(receipt_address ~ '^[0-9a-f]{64}$'),
    previous_commit text check(previous_commit ~ '^[0-9a-f]{64}$'),
    actor uuid not null references identity(id),
    created_at timestamptz not null default now(),
    charged_bytes bigint generated always as (320+octet_length(branch)::bigint+octet_length(receipt)) stored,
    primary key(project_id,branch,generation),
    check((generation=1)=(previous_commit is null))
);

create function account_project_history() returns trigger
language plpgsql security definer set search_path=pg_catalog,public,pg_temp as $$
declare project uuid; amount bigint; rows_delta bigint; branches bigint:=0;
begin
    if tg_op='INSERT' then project:=new.project_id; amount:=new.charged_bytes; rows_delta:=1;
    else project:=old.project_id; amount:=-old.charged_bytes; rows_delta:=-1; end if;
    if tg_table_name='project_ref_event' then
        if new.generation=1 then branches:=1; end if;
    end if;
    insert into public.project_history_storage(project_id) values(project) on conflict do nothing;
    update public.project_history_storage
        set bytes_used=bytes_used+amount,rows_used=rows_used+rows_delta,branches_used=branches_used+branches
        where project_id=project and amount<=byte_limit-bytes_used
            and rows_delta<=row_limit-rows_used and branches<=branch_limit-branches_used;
    if not found then raise exception 'published project history storage budget reached'
        using errcode='23514',constraint='project_history_quota'; end if;
    return null;
end $$;
create function project_history_delete_guard() returns trigger
language plpgsql set search_path=pg_catalog,public,pg_temp as $$
declare documents uuid[];
begin
    if tg_table_name='project_published_commit' then documents:=old.scope;
    elsif tg_table_name='project_published_object' then documents:=array[old.document_id];
    else
        select scope into documents from public.project_published_commit
            where project_id=old.project_id and address=old.candidate;
    end if;
    if pg_trigger_depth()>1 and exists (
        select 1 from public.erasure_receipt r join public.document d on d.id=r.resource_id
        where r.kind='documents' and r.transaction_id=txid_current()
            and r.resource_id=any(documents) and d.deleted_at is not null
    ) then return old; end if;
    raise exception 'published history is immutable outside audited document erasure';
end $$;
do $$ declare t text; begin
    foreach t in array array['project_published_commit','project_published_object','project_history_review','project_ref_event'] loop
        execute format('create trigger history_account after insert or delete on %I for each row execute function account_project_history()',t);
        execute format('create trigger history_no_update before update on %I for each row execute function event_immutable()',t);
        execute format('create trigger history_no_truncate before truncate on %I for each statement execute function event_immutable()',t);
        if t='project_ref_event' then
            execute format('create trigger history_no_delete before delete on %I for each row execute function event_immutable()',t);
        else
            execute format('create trigger history_no_delete before delete on %I for each row execute function project_history_delete_guard()',t);
        end if;
    end loop;
end $$;
create function erase_published_project_history() returns trigger
language plpgsql security definer set search_path=pg_catalog,public,pg_temp as $$
begin
    delete from public.project_history_review r using public.project_published_commit c
        where r.project_id=c.project_id and r.candidate=c.address and c.scope @> array[old.id];
    delete from public.project_published_object where document_id=old.id;
    delete from public.project_published_commit where scope @> array[old.id];
    return old;
end $$;
-- Before shared byte erasure; candidate parents/scopes are not rewritten.
create trigger document_project_published_erasure before delete on document
    for each row execute function erase_published_project_history();
revoke all on project_ref_authority,project_history_storage,project_published_commit,project_published_object,project_history_review,project_ref_event from public;
revoke execute on function account_project_history(),project_history_delete_guard(),erase_published_project_history() from public;
