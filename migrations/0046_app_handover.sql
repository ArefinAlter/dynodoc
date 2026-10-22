-- Native-app sign-in hand-over (docs/decisions/017-native-app-sign-in-handover.md).
--
-- The Dynodoc app (desktop, Android, iPhone) cannot be signed in by the emailed link or by
-- Google: both finish in the system browser, which shares no session with the app's web
-- view. The browser instead asks for a one-time hand-over code, bound to a PKCE challenge
-- the app made, and the app redeems it from inside its own web view with the matching
-- verifier (RFC 8252 + RFC 7636).
--
-- A row is a short-lived, single-use ledger entry, not a session. Only the SHA-256 of the
-- code is stored (a database leak never yields a live code), and the code alone is useless
-- without the verifier whose SHA-256 is `challenge`, which never leaves the app. A row
-- lives 60 seconds (the application sets expires_at; the table refuses more than two
-- minutes), can move from unused to used exactly once, and is never otherwise edited. The
-- permanent record is the operation_audit rows `auth.app_handover.issued`, `.redeemed`
-- and `.refused`, which hold no secret. Rows older than a day are pruned by the next
-- issuance for the same account.
--
-- Additive: nothing existing changes, and nothing reads this table unless the new routes
-- are called. Like 0043 to 0045 this is not rollback-safe by leaving the table unused: the
-- API refuses a newer schema, so rolling back means restoring the dump.
create table app_handover_code (
    id uuid primary key default gen_random_uuid(),
    identity_id uuid not null references identity(id) on delete cascade,
    -- SHA-256 of the code (32 bytes). Unique: a code names exactly one row.
    code_hash bytea not null unique check (octet_length(code_hash) = 32),
    -- SHA-256 of the app's PKCE verifier (the raw 32 bytes of base64url(challenge)).
    challenge bytea not null check (octet_length(challenge) = 32),
    -- The app's own anti-forgery value, echoed back in the app link (not a secret).
    state text not null check (state ~ '^[A-Za-z0-9_-]{16,64}$'),
    platform text not null check (platform in ('desktop', 'android', 'ios')),
    -- The account's session generation when the code was issued: signing out everywhere
    -- between issue and redeem invalidates the code, as it does a magic link.
    session_generation bigint not null,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null,
    consumed_at timestamptz,
    -- 'redeemed' when the verifier matched; 'burned' when a redeem failed while the code
    -- was live (a failed redeem uses the code up).
    outcome text check (outcome in ('redeemed', 'burned')),
    check (expires_at > created_at and expires_at <= created_at + interval '2 minutes'),
    check ((consumed_at is null) = (outcome is null))
);
create index app_handover_code_identity_idx on app_handover_code(identity_id, created_at);

create function app_handover_code_guard() returns trigger language plpgsql as $$
begin
    if new.id is distinct from old.id
        or new.identity_id is distinct from old.identity_id
        or new.code_hash is distinct from old.code_hash
        or new.challenge is distinct from old.challenge
        or new.state is distinct from old.state
        or new.platform is distinct from old.platform
        or new.session_generation is distinct from old.session_generation
        or new.created_at is distinct from old.created_at
        or new.expires_at is distinct from old.expires_at
        or old.consumed_at is not null
        or new.consumed_at is null then
        raise exception 'a hand-over code can only be used up, once';
    end if;
    return new;
end $$;
create trigger app_handover_code_use_once before update on app_handover_code
    for each row execute function app_handover_code_guard();

revoke all on app_handover_code from public;
revoke execute on function app_handover_code_guard() from public;
