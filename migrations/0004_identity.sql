-- 0004_identity — actors and email + magic-link auth (PoC scope).
--
-- PoC auth is email + magic link only (per the brief): no phone verification, no
-- behavioral signals. An identity is the actor referenced by every event. Sessions
-- are stateless Paseto v4 tokens (no session table); the only stored auth state is
-- the one-time magic-link token.

create table identity (
    id           uuid        primary key default gen_random_uuid(),
    email        text        not null,
    display_name text,
    created_at   timestamptz not null default now()
);

-- Case-insensitive email uniqueness without the citext extension.
create unique index identity_email_lower_idx on identity (lower(email));

-- One-time login tokens. We store only the SHA-256 of the token (never the token
-- itself), so a database leak does not hand out live login links.
create table magic_link (
    id          uuid        primary key default gen_random_uuid(),
    identity_id uuid        not null references identity (id) on delete cascade,
    token_hash  bytea       not null check (octet_length(token_hash) = 32),
    expires_at  timestamptz not null,
    consumed_at timestamptz,
    created_at  timestamptz not null default now()
);

create index magic_link_token_idx on magic_link (token_hash);
create index magic_link_identity_idx on magic_link (identity_id);

-- Wire the forward-referenced FKs now that identity exists (columns created in 0001
-- and 0002). event.actor_id is RESTRICT by default — you cannot delete an identity
-- that authored events (the audit log must keep its actors resolvable).
alter table event
    add constraint event_actor_fk
    foreign key (actor_id) references identity (id);

alter table document
    add constraint document_created_by_fk
    foreign key (created_by) references identity (id);
