-- 0011_idempotency — Postgres-backed idempotency keys for write endpoints (stage 18).
--
-- Numbering: continues after 0010_auth (0005–0008 stay reserved for the consultation
-- tables, see .cursor/stage-map.md). Append-only: never edit an applied migration.
--
-- PoC deviation (docs/decisions/000-initial.md): the full design stores idempotency
-- keys in Redis. The PoC has no Redis, so keys live in Postgres. Every write endpoint
-- accepts an optional `Idempotency-Key` header. The first request for a key executes
-- and caches its response (status + JSON body); a replay within the TTL window returns
-- the cached response verbatim WITHOUT re-executing — so a retried POST never appends a
-- duplicate event (docs/18 §6).
--
-- The row is keyed by (identity_id, idempotency_key) so one identity's key namespace
-- cannot collide with another's. `INSERT ... ON CONFLICT DO NOTHING RETURNING` is the
-- atomic first-vs-replay test the handler uses (the same conditional-write technique as
-- the magic-link / refresh-token rotation in 0010).

create table idempotency_key (
    -- Who presented the key. Scopes the key namespace per identity.
    identity_id     uuid        not null references identity (id) on delete cascade,
    -- The client-supplied key (opaque, bounded length to bound storage).
    idempotency_key text        not null check (char_length(idempotency_key) between 1 and 255),
    -- The request route, so the same key on two different endpoints is distinct and a
    -- mismatched replay can be rejected rather than served the wrong cached body.
    request_path    text        not null,
    -- The cached HTTP status code of the first execution's response.
    response_status integer     not null,
    -- The cached JSON response body returned verbatim to replays.
    response_body   jsonb       not null,
    created_at      timestamptz not null default now(),

    primary key (identity_id, idempotency_key)
);

-- TTL sweep support: a replay older than 24h is treated as a fresh request. A periodic
-- job (or the handler's own pre-check) deletes/ignores expired rows; this index makes
-- the "is this key still live?" scan and any cleanup cheap.
create index idempotency_key_created_idx on idempotency_key (created_at);
