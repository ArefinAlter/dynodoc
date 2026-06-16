-- 0003_snapshots — snapshotted fold + Merkle root over the event log.
--
-- Reading current state by replaying every event is O(events). A snapshot stores the
-- folded state at a given seq so reads cost only O(events since last snapshot)
-- (docs/04 §E.4, §D.3). Each snapshot also carries a Merkle root over the events it
-- covers (domain-separated leaves H(0x00||content), internal H(0x01||l||r), docs/04
-- §E.3) plus the chain_hash at its through_seq, binding the snapshot to the hash chain.

create table snapshot (
    id               uuid        primary key default gen_random_uuid(),
    document_id      uuid        not null references document (id) on delete cascade,
    -- The event seq this snapshot folds the log up to (inclusive).
    through_seq      bigint      not null,
    -- The folded document state at through_seq (the materialization, as JSONB).
    state            jsonb       not null,
    -- Merkle root over the covered events — 32-byte SHA-256 (docs/04 §E.3).
    merkle_root      bytea       not null check (octet_length(merkle_root) = 32),
    -- The event.chain_hash at through_seq; ties this snapshot to a chain position so
    -- verify can confirm the snapshot matches the replayed log (docs/04 §E.4).
    event_chain_hash bytea       not null check (octet_length(event_chain_hash) = 32),
    created_at       timestamptz not null default now(),

    constraint snapshot_document_seq_unique unique (document_id, through_seq)
);

-- Latest-snapshot-for-a-document lookup (read path starts from the newest snapshot).
create index snapshot_latest_idx on snapshot (document_id, through_seq desc);

-- A deployed document pins the exact immutable snapshot captured at deploy time
-- (docs/04 §7.4 "as-of pin"). The column was created in 0001; wire the FK now that
-- snapshot exists. RESTRICT: you cannot delete a snapshot a document is pinned to.
alter table document
    add constraint document_deployed_snapshot_fk
    foreign key (deployed_snapshot_id) references snapshot (id);
