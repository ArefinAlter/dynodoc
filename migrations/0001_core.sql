-- 0001_core — document and node: the materialized current state.
--
-- The system of record is the append-only event log (0002). `document` and `node`
-- here are the MATERIALIZED current state derived from the log via snapshotted fold
-- (docs/04 §E.4) — a rebuildable cache, not the source of truth. They exist so the
-- API can read current state cheaply without replaying every event.
--
-- Identity model (docs/04 §D.1): node.id is an application-generated ULID, stable
-- for the node's lifetime and never reused — the load-bearing data-model decision.
-- document.id is a db-generated UUID.

-- ---------------------------------------------------------------------------
-- document
-- ---------------------------------------------------------------------------
create table document (
    id                   uuid        primary key default gen_random_uuid(),
    title                text        not null,
    -- Multiple languages are structural (docs/04 §1.3), not a setting.
    languages            text[]      not null default array['en'],
    -- Lifecycle: draft -> deployed (pinned, signed) -> archived. The Deployed
    -- event (0002) is what flips this to 'deployed' and pins deployed_snapshot_id.
    status               text        not null default 'draft'
                                     check (status in ('draft', 'deployed', 'archived')),
    settings             jsonb       not null default '{}'::jsonb,
    -- Pin to the immutable snapshot captured at deploy time. FK added in 0003
    -- (snapshot table does not exist yet); created_by FK added in 0004 (identity).
    deployed_snapshot_id uuid,
    created_by           uuid,
    created_at           timestamptz not null default now(),
    updated_at           timestamptz not null default now()
);

create index document_status_idx on document (status);

-- ---------------------------------------------------------------------------
-- node — typed, stable-identity tree (Form > Section > Item > Choice).
-- The engine stays regime-agnostic, so the type check also admits the policy-
-- document node types (clause, paragraph) used by the later consultation layer.
-- ---------------------------------------------------------------------------
create table node (
    id             text        primary key,                  -- ULID, app-generated
    document_id    uuid        not null references document (id) on delete cascade,
    parent_id      text        references node (id) on delete cascade,  -- null for the root/form
    type           text        not null
                               check (type in ('form', 'section', 'item', 'choice', 'clause', 'paragraph')),
    -- Dense order key for sibling ordering (fractional index, docs/04 §B.3.2).
    -- Stored as text so arbitrarily many positions fit between two siblings.
    pos            text        not null,
    -- Type-specific field record (label map, item type, constraint, etc.). Shape
    -- varies by node type, so JSONB rather than a column per field (docs/13).
    current_fields jsonb       not null default '{}'::jsonb,
    -- Analysis-facing variable name (items only); the referential-integrity pass
    -- (stage 21) checks relevance/constraint expressions against these.
    var_name       text,
    -- Materialized-state tombstone: NodeDeleted is an event, but the fold marks
    -- the node deleted rather than removing the row (history stays in the log).
    deleted        boolean     not null default false,
    created_at     timestamptz not null default now(),
    updated_at     timestamptz not null default now()
);

create index node_document_idx on node (document_id);
create index node_parent_idx on node (parent_id);
-- varName lookups for the referential-integrity pass (not unique: the app-layer
-- integrity check owns duplicate-varName detection; transient dup states are legal
-- mid-edit). Scoped to a document, live nodes only.
create index node_varname_idx on node (document_id, var_name) where var_name is not null and not deleted;
-- Query into current_fields (e.g. find items referencing a varName) — docs/13.
create index node_fields_gin on node using gin (current_fields);

-- ---------------------------------------------------------------------------
-- Stable-identity guard: node.id must never be reassigned (docs/13 "Stable
-- identity columns" — paranoia trigger). UPDATEs that touch any other column are
-- fine; only changing id raises.
-- ---------------------------------------------------------------------------
create function node_id_immutable() returns trigger as $$
begin
    if new.id is distinct from old.id then
        raise exception 'node.id is immutable (% -> %); stable identity must never be reassigned', old.id, new.id;
    end if;
    return new;
end;
$$ language plpgsql;

create trigger node_id_no_reassign before update on node
    for each row execute function node_id_immutable();
