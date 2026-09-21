-- Explicit whole-resource erasure is separate from ordinary append-only editing.
-- No runtime trigger disabling or session-wide bypass is permitted. A deletion
-- receipt and the document's deletion must occur in the same transaction.
alter table identity add column erased_at timestamptz;
create table erasure_receipt (
    resource_id uuid primary key,
    kind text not null check (kind in ('documents','users')),
    actor_id uuid not null references identity(id),
    reason text not null check (reason in ('owner_request','duplicate','policy','other')),
    counts jsonb not null,
    transaction_id bigint not null default txid_current(),
    erased_at timestamptz not null default now()
);
create trigger erasure_receipt_immutable before update or delete on erasure_receipt
    for each row execute function event_immutable();

-- Deployed snapshots may be removed only together with their owning document.
alter table document alter constraint document_deployed_snapshot_fk
    deferrable initially deferred;

create function document_erasure_child_guard() returns trigger
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

drop trigger event_no_delete on event;
create trigger event_no_delete before delete on event
    for each row execute function document_erasure_child_guard();
drop trigger document_version_no_update on document_version;
create trigger document_version_no_update before update on document_version
    for each row execute function event_immutable();
create trigger document_version_no_delete before delete on document_version
    for each row execute function document_erasure_child_guard();
drop trigger draft_edit_no_update on draft_edit;
create trigger draft_edit_no_update before update on draft_edit
    for each row execute function event_immutable();
create trigger draft_edit_no_delete before delete on draft_edit
    for each row execute function document_erasure_child_guard();

create function erase_document_children() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
begin
    if old.deleted_at is null or not exists (
        select 1 from public.erasure_receipt where resource_id=old.id
          and kind='documents' and transaction_id=txid_current()
    ) then
        raise exception 'move to Trash and create an erasure receipt before deleting a document';
    end if;
    delete from public.public_document_view where document_id=old.id;
    delete from public.document_version where document_id=old.id;
    delete from public.draft_edit where draft_id in (
        select id from public.workspace_draft where document_id=old.id
    );
    delete from public.workspace_draft where document_id=old.id;
    delete from public.document_upload where document_id=old.id;
    delete from public.event where document_id=old.id;
    delete from public.node where document_id=old.id;
    delete from public.snapshot where document_id=old.id;
    delete from public.idempotency_key where request_path='/documents/'||old.id::text
       or starts_with(request_path,'/documents/'||old.id::text||'/');
    return old;
end;
$$;
create trigger document_erasure before delete on document
    for each row execute function erase_document_children();

-- A pseudonymous actor remains because other documents' hashes include its UUID.
-- Never restore an erased profile or authenticate it again.
create function erased_identity_guard() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
begin
    if old.erased_at is null and new.erased_at is not null then
        if new.disabled_at is null or not exists (
            select 1 from public.erasure_receipt where resource_id=old.id
                and kind='users' and transaction_id=txid_current()
        ) then
            raise exception 'account erasure requires a same-transaction receipt';
        end if;
        delete from public.draft_edit where draft_id in (
            select id from public.workspace_draft where created_by=old.id
        );
        delete from public.workspace_draft where created_by=old.id;
    end if;
    if old.erased_at is not null and (
        new.erased_at is distinct from old.erased_at or new.disabled_at is null
        or new.email is distinct from old.email or new.display_name is distinct from old.display_name
    ) then
        raise exception 'an erased account cannot be restored';
    end if;
    return new;
end;
$$;
create trigger erased_identity_immutable before update on identity
    for each row execute function erased_identity_guard();
