-- Durable encoded shared-object budgets. These exclude native uploads, events,
-- legacy JSON snapshots, indexes, WAL and backups; they are not total disk quotas.
create table checkpoint_storage_account (
    identity_id uuid primary key references identity(id),
    bytes_used bigint not null default 0 check (bytes_used >= 0),
    objects_used bigint not null default 0 check (objects_used >= 0),
    byte_limit bigint not null default 4294967296 check (byte_limit >= 0),
    object_limit bigint not null default 1000000 check (object_limit >= 0),
    check (bytes_used <= byte_limit and objects_used <= object_limit)
);
create table checkpoint_storage_document (
    document_id uuid primary key references document(id) on delete cascade,
    -- Charge the original creator. Moving files or granting access does not move
    -- the charge; account erasure preserves pseudonymous history and this budget.
    identity_id uuid not null references checkpoint_storage_account(identity_id),
    bytes_used bigint not null default 0 check (bytes_used >= 0),
    objects_used bigint not null default 0 check (objects_used >= 0),
    byte_limit bigint not null default 1073741824 check (byte_limit >= 0),
    object_limit bigint not null default 250000 check (object_limit >= 0),
    check (bytes_used <= byte_limit and objects_used <= object_limit)
);
create index checkpoint_storage_document_account on checkpoint_storage_document(identity_id);

-- Preserve already-retained objects. Existing stores above defaults are held at
-- their current usage until an operator deliberately increases the budget.
insert into checkpoint_storage_account(identity_id,bytes_used,objects_used,byte_limit,object_limit)
select d.created_by,sum(octet_length(o.bytes)),count(*),
       greatest(4294967296,sum(octet_length(o.bytes))),greatest(1000000,count(*))
from checkpoint_object o join document d on d.id=o.document_id group by d.created_by;
insert into checkpoint_storage_document(document_id,identity_id,bytes_used,objects_used,byte_limit,object_limit)
select d.id,d.created_by,sum(octet_length(o.bytes)),count(*),
       greatest(1073741824,sum(octet_length(o.bytes))),greatest(250000,count(*))
from checkpoint_object o join document d on d.id=o.document_id group by d.id,d.created_by;

-- Transition tables charge actual inserted/deleted rows once per SQL batch.
-- ON CONFLICT retries and content reuse cost no additional quota. Account rows
-- are acquired in UUID order, then document rows in UUID order, across all calls.
-- A failed quota check rolls back objects, counters and publication together.
create function account_shared_object_insert() returns trigger
language plpgsql security definer set search_path=pg_catalog,public,pg_temp as $$
declare item record;
begin
    for item in
        select coalesce(q.identity_id,d.created_by) as actor,
               sum(octet_length(n.bytes)) as added_bytes,count(*) as added_objects
        from inserted_objects n join public.document d on d.id=n.document_id
        left join public.checkpoint_storage_document q on q.document_id=d.id
        group by coalesce(q.identity_id,d.created_by) order by actor
    loop
        insert into public.checkpoint_storage_account(identity_id) values(item.actor) on conflict do nothing;
        update public.checkpoint_storage_account
            set bytes_used=bytes_used+item.added_bytes,objects_used=objects_used+item.added_objects
            where identity_id=item.actor and item.added_bytes<=byte_limit-bytes_used
              and item.added_objects<=object_limit-objects_used;
        if not found then
            raise exception 'shared history storage budget reached'
                using errcode='23514',constraint='checkpoint_storage_quota';
        end if;
    end loop;
    for item in
        select n.document_id,d.created_by,sum(octet_length(n.bytes)) as added_bytes,count(*) as added_objects
        from inserted_objects n join public.document d on d.id=n.document_id
        group by n.document_id,d.created_by order by n.document_id
    loop
        insert into public.checkpoint_storage_document(document_id,identity_id)
            values(item.document_id,item.created_by) on conflict do nothing;
        update public.checkpoint_storage_document
            set bytes_used=bytes_used+item.added_bytes,objects_used=objects_used+item.added_objects
            where document_id=item.document_id and item.added_bytes<=byte_limit-bytes_used
              and item.added_objects<=object_limit-objects_used;
        if not found then
            raise exception 'shared history storage budget reached'
                using errcode='23514',constraint='checkpoint_storage_quota';
        end if;
    end loop;
    return null;
end $$;
create trigger checkpoint_objects_account_insert after insert on checkpoint_object
    referencing new table as inserted_objects for each statement execute function account_shared_object_insert();

create function account_shared_object_delete() returns trigger
language plpgsql security definer set search_path=pg_catalog,public,pg_temp as $$
declare item record;
begin
    for item in
        select q.identity_id,sum(octet_length(n.bytes)) as removed_bytes,count(*) as removed_objects
        from deleted_objects n join public.checkpoint_storage_document q on q.document_id=n.document_id
        group by q.identity_id order by q.identity_id
    loop
        update public.checkpoint_storage_account
            set bytes_used=bytes_used-item.removed_bytes,objects_used=objects_used-item.removed_objects
            where identity_id=item.identity_id;
    end loop;
    for item in
        select document_id,sum(octet_length(bytes)) as removed_bytes,count(*) as removed_objects
        from deleted_objects group by document_id order by document_id
    loop
        update public.checkpoint_storage_document
            set bytes_used=bytes_used-item.removed_bytes,objects_used=objects_used-item.removed_objects
            where document_id=item.document_id;
    end loop;
    return null;
end $$;
create trigger checkpoint_objects_account_delete after delete on checkpoint_object
    referencing old table as deleted_objects for each statement execute function account_shared_object_delete();

create trigger checkpoint_objects_no_truncate before truncate on checkpoint_object
    for each statement execute function event_immutable();
revoke all on checkpoint_storage_account,checkpoint_storage_document from public;
revoke execute on function account_shared_object_insert(),account_shared_object_delete() from public;
