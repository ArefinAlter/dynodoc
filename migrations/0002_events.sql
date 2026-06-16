-- 0002_events — the append-only, hash-chained event log: the system of record.
--
-- Every change is a typed semantic event on a stable-identity node. Current state
-- (document/node, 0001) is a fold over this log. The hash chain (docs/04 §E.2) makes
-- the log tamper-evident: anyone can replay it and verify it has not been altered.
--
-- Hash chain (computed application-side inside the append transaction, stage 14):
--   content_hash = SHA-256 over the event's canonical content (type, target, payload, …)
--   chain_hash   = SHA-256(prev_chain_hash || content_hash)
--   prev_chain_hash = the previous event's chain_hash; for a document's genesis
--                     event it is 32 zero bytes (so the column stays NOT NULL).
-- All three are 32-byte SHA-256 digests, enforced by the octet_length checks.

create table event (
    id              text        primary key,                 -- ULID, app-generated, unique across replicas
    document_id     uuid        not null references document (id) on delete cascade,
    -- Monotonic per-document sequence (the fold order). Unique per document.
    seq             bigint      not null,
    -- The semantic event vocabulary for the Research IDE PoC. Payload (JSONB) carries
    -- the type-specific detail. Extend with a later migration as the vocabulary grows.
    type            text        not null check (type in (
                        'NodeCreated',
                        'NodeMoved',
                        'NodeDeleted',
                        'FieldEdited',
                        'ChoiceAdded',
                        'ChoiceRemoved',
                        'CommentAdded',
                        'SuggestionProposed',
                        'SuggestionAccepted',
                        'SuggestionRejected',
                        'Deployed'
                    )),
    -- Actor (identity) that authored the event. FK to identity added in 0004.
    actor_id        uuid        not null,
    -- Node this event targets; null for document-level events (e.g. Deployed).
    -- No ON DELETE action: events are immutable, so nothing may rewrite this column.
    -- Nodes are tombstoned (deleted=true), never row-deleted, so this never blocks.
    target_node_id  text        references node (id),
    payload         jsonb       not null default '{}'::jsonb,
    content_hash    bytea       not null check (octet_length(content_hash) = 32),
    prev_chain_hash bytea       not null check (octet_length(prev_chain_hash) = 32),
    chain_hash      bytea       not null check (octet_length(chain_hash) = 32),
    created_at      timestamptz not null default now(),

    constraint event_document_seq_unique unique (document_id, seq),
    -- A chain hash is unique within a document's chain (it folds in seq + content).
    constraint event_document_chain_unique unique (document_id, chain_hash)
);

create index event_document_seq_idx on event (document_id, seq);
create index event_target_node_idx on event (target_node_id);
create index event_type_idx on event (type);

-- ---------------------------------------------------------------------------
-- Append-only enforcement at the storage layer (docs/13). The audit guarantee
-- depends on events being immutable in the database, not just by convention.
-- ---------------------------------------------------------------------------
create function event_immutable() returns trigger as $$
begin
    raise exception 'event table is append-only; UPDATE/DELETE forbidden';
end;
$$ language plpgsql;

create trigger event_no_update before update on event
    for each row execute function event_immutable();
create trigger event_no_delete before delete on event
    for each row execute function event_immutable();

-- Partitioning note: not yet. When a single document approaches ~1M events,
-- partition declaratively by (document_id, seq range). At PoC scale this is years
-- away, so a single table keeps things simple.
