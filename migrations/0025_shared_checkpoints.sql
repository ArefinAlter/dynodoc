-- Opt-in derived checkpoints. Legacy snapshots and event encodings are unchanged.
create table checkpoint_object (
    document_id uuid not null references document(id),
    address text not null check (address ~ '^[0-9a-f]{64}$'),
    bytes bytea not null check (octet_length(bytes) between 1 and 1048576),
    primary key(document_id,address)
);
create table shared_checkpoint (
    document_id uuid not null references document(id),
    through_seq bigint not null check (through_seq>=0),
    codec_version smallint not null check (codec_version=1),
    state_root text not null,
    event_chain_hash bytea not null check (octet_length(event_chain_hash)=32),
    created_at timestamptz not null default now(),
    primary key(document_id,through_seq),
    foreign key(document_id,state_root) references checkpoint_object(document_id,address)
);
create trigger checkpoint_object_no_update before update on checkpoint_object
    for each row execute function event_immutable();
create trigger checkpoint_object_no_delete before delete on checkpoint_object
    for each row execute function document_erasure_child_guard();
create trigger shared_checkpoint_no_update before update on shared_checkpoint
    for each row execute function event_immutable();
create trigger shared_checkpoint_no_delete before delete on shared_checkpoint
    for each row execute function document_erasure_child_guard();

-- Runs while the owning document is still present, so the existing audited child
-- guard can verify its trash state and same-transaction erasure receipt.
create function erase_shared_checkpoint_children() returns trigger
language plpgsql set search_path=pg_catalog,public as $$
begin
    delete from public.shared_checkpoint where document_id=old.id;
    delete from public.checkpoint_object where document_id=old.id;
    return old;
end;
$$;
create trigger document_shared_checkpoint_erasure before delete on document
    for each row execute function erase_shared_checkpoint_children();
