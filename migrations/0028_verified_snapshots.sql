-- Receipts are issued only after comparison with canonical event replay. Existing
-- snapshots deliberately receive no receipt: run engine-cli verify-snapshots
-- before exposing them through the new reader. Never bless old state in SQL.
create table snapshot_verification (
    snapshot_id uuid primary key references snapshot(id) on delete cascade,
    document_id uuid not null references document(id),
    through_seq bigint not null check (through_seq >= 0),
    version smallint not null default 1 check (version = 1),
    fingerprint bytea not null check (octet_length(fingerprint) = 32),
    verified_at timestamptz not null default now()
);
create index snapshot_verification_document_idx
    on snapshot_verification(document_id, through_seq);

create trigger snapshot_no_update before update on snapshot
    for each row execute function event_immutable();
create trigger snapshot_no_delete before delete on snapshot
    for each row execute function document_erasure_child_guard();
create trigger snapshot_no_truncate before truncate on snapshot
    for each statement execute function event_immutable();
create trigger snapshot_verification_no_update before update on snapshot_verification
    for each row execute function event_immutable();
create trigger snapshot_verification_no_delete before delete on snapshot_verification
    for each row execute function document_erasure_child_guard();
create trigger snapshot_verification_no_truncate before truncate on snapshot_verification
    for each statement execute function event_immutable();

-- Audited document erasure already deletes snapshots inside its trigger. The
-- receipt FK cascade runs while that parent document/erasure receipt still exists.
-- These guards do not constrain a DB owner who can disable/drop them. Runtime
-- privilege separation and independent commitments remain separate boundaries.
