-- 0010_auth — per-document access list and refresh-token rotation (PoC auth).
--
-- Numbering: 0005–0008 are reserved for the consultation tables (added post-PoC, see
-- .cursor/stage-map.md) and 0009 is the views migration, so this stage-17 PoC addition
-- takes the next free number after that block. It depends only on `document` and
-- `identity` (both from 0001/0004), so running after 0009_views is order-safe.
--
-- PoC auth is email + magic link only (no tiers, phone, OAuth, or Sybil signals; the
-- magic-link tables are in 0004). Two pieces of stored state land here:
--
--   document_access  the access list mapping an identity to its role on a document
--                    (Author | Reviewer | Auditor) — PRD §6.8 FR-27. The governance
--                    gate (stage 16) reads this role to decide capability.
--   refresh_token    long-lived rotation tokens (PRD §6.8 FR-26). Access tokens are
--                    stateless Paseto v4; refresh tokens are server state so they can
--                    be rotated on use and revoked. Only the SHA-256 of the token is
--                    stored — a database leak never yields a live token.

-- ---------------------------------------------------------------------------
-- document_access — who may act on a document, and in what role.
-- ---------------------------------------------------------------------------
create table document_access (
    document_id uuid        not null references document (id) on delete cascade,
    identity_id uuid        not null references identity (id) on delete cascade,
    -- The PoC role model (PRD §3.2 NG2): Author (full authoring), Reviewer (comment +
    -- suggest), Auditor (read-only). NOT the consultation tier model (T0–T3).
    role        text        not null check (role in ('author', 'reviewer', 'auditor')),
    created_at  timestamptz not null default now(),

    -- One role per identity per document.
    primary key (document_id, identity_id)
);

-- "Which documents can this identity reach?" — the researcher's working set.
create index document_access_identity_idx on document_access (identity_id);

-- ---------------------------------------------------------------------------
-- refresh_token — rotating session continuation (FR-26). The opaque token is sent to
-- the client (HttpOnly cookie in the browser); only its hash lives here.
-- ---------------------------------------------------------------------------
create table refresh_token (
    id          uuid        primary key default gen_random_uuid(),
    identity_id uuid        not null references identity (id) on delete cascade,
    -- SHA-256 of the issued opaque token (32 bytes); the token itself is never stored.
    token_hash  bytea       not null check (octet_length(token_hash) = 32),
    expires_at  timestamptz not null,
    -- Set when this token is exchanged for a successor (rotation): a rotated token can
    -- never be used again, so token theft + replay is detectable and bounded.
    rotated_at  timestamptz,
    -- Set when explicitly revoked (logout, credential change).
    revoked_at  timestamptz,
    created_at  timestamptz not null default now()
);

-- Look up a presented token by its hash; list/revoke an identity's tokens.
create unique index refresh_token_hash_idx on refresh_token (token_hash);
create index refresh_token_identity_idx on refresh_token (identity_id);
