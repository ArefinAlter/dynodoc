-- Operator-issued sign-in links (docs/decisions/018-operator-issued-sign-in-links.md).
--
-- A self-hosted installation without a working mail server (or with `AUTH_EMAIL_LINK=off`)
-- needs a way to let the FIRST administrator in, and a break-glass way in later. The operator
-- (the person with shell access to the host and the database OWNER credential) mints a one-time
-- sign-in link for an address with `engine-cli auth issue-link`; the link is verified by the same
-- `/auth/verify` route as an emailed one and makes the same kind of login.
--
-- The point of this table is WHO CAN WRITE IT. No HTTP route makes a row here: the API's runtime
-- login is granted SELECT, UPDATE and DELETE only (engine_core::database, `CONSUME_PRUNE`), never
-- INSERT, so a compromised or buggy API cannot mint a link. The owner credential is the trust
-- root: whoever holds it can already read and alter the whole database, so this adds no privilege.
--
-- Like every sign-in token, only the SHA-256 of the token is stored. A row lives at most 24 hours
-- (the table refuses more), can move from live to ended exactly once (a trigger refuses every other
-- edit), and is never otherwise changed. How it ended is `outcome`: `used` (signed in), `revoked`
-- (the operator, or a newer link for the same account), `expired` (swept by the retention job).
-- The permanent record is the operation_audit rows `auth.operator_link.issued`, `.used`, `.refused`,
-- `.revoked` and `.expired`, which hold no secret. The retention job deletes ended rows after
-- `RETENTION_AUTH_DAYS`; erasing an account deletes its rows (the free-text `reason` could name the
-- person).
--
-- Additive: nothing existing changes, nothing reads this table unless `/auth/verify` is given a
-- token that is not an emailed one, and hosted behaviour is unchanged (the table stays empty there
-- unless an operator uses the CLI). Like 0043 to 0046 this is not rollback-safe by leaving the
-- table unused: the API refuses a newer schema, so rolling back means restoring the dump. The
-- restricted runtime login needs `engine-cli grant-runtime` after this migration, as always.
create table operator_sign_in_link (
    id uuid primary key default gen_random_uuid(),
    identity_id uuid not null references identity(id) on delete cascade,
    -- SHA-256 of the token (32 bytes). Unique: a token names exactly one row.
    token_hash bytea not null unique check (octet_length(token_hash) = 32),
    -- The account's session generation when the link was made: signing the account out
    -- everywhere (or removing it) between issue and use ends the link, as it ends a magic link.
    session_generation bigint not null,
    -- Who made it: the operator's operating-system user name as the CLI saw it. A label, not
    -- an authentication (the owner credential can write any value); no control characters.
    operator text not null check (octet_length(operator) between 1 and 64 and operator !~ '[[:cntrl:]]'),
    -- Free text from the operator. Keep personal data out of it: the audit row keeps it for good.
    reason text not null default '' check (octet_length(reason) <= 200 and reason !~ '[[:cntrl:]]'),
    created_at timestamptz not null default now(),
    expires_at timestamptz not null,
    consumed_at timestamptz,
    outcome text check (outcome in ('used', 'revoked', 'expired')),
    check (expires_at > created_at and expires_at <= created_at + interval '24 hours'),
    check ((consumed_at is null) = (outcome is null))
);
create index operator_sign_in_link_identity_idx on operator_sign_in_link(identity_id, created_at);
-- "Which links are live?" (supersede at issue, the sweep, the list command).
create index operator_sign_in_link_live_idx on operator_sign_in_link(expires_at) where consumed_at is null;

create function operator_sign_in_link_guard() returns trigger language plpgsql as $$
begin
    if new.id is distinct from old.id
        or new.identity_id is distinct from old.identity_id
        or new.token_hash is distinct from old.token_hash
        or new.session_generation is distinct from old.session_generation
        or new.operator is distinct from old.operator
        or new.reason is distinct from old.reason
        or new.created_at is distinct from old.created_at
        or new.expires_at is distinct from old.expires_at
        -- ended once: a used, revoked or expired row never changes again
        or old.consumed_at is not null
        or new.consumed_at is null
        or new.outcome is null
        -- an expired link can never be used (whatever the caller's query said), and only an
        -- expired one is marked expired
        or (new.outcome = 'used' and old.expires_at <= now())
        or (new.outcome = 'expired' and old.expires_at > now()) then
        raise exception 'an operator sign-in link can only be ended, once';
    end if;
    return new;
end $$;
create trigger operator_sign_in_link_end_once before update on operator_sign_in_link
    for each row execute function operator_sign_in_link_guard();

-- A row is born live: nobody (the owner included, by mistake) inserts one already used.
create function operator_sign_in_link_born_live() returns trigger language plpgsql as $$
begin
    if new.consumed_at is not null or new.outcome is not null then
        raise exception 'an operator sign-in link is created live';
    end if;
    return new;
end $$;
create trigger operator_sign_in_link_born_live before insert on operator_sign_in_link
    for each row execute function operator_sign_in_link_born_live();

revoke all on operator_sign_in_link from public;
revoke execute on function operator_sign_in_link_guard() from public;
revoke execute on function operator_sign_in_link_born_live() from public;
