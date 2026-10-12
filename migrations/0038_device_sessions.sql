-- Project-scoped device sessions. A row is the one-use replay ledger and the
-- issuance-rate window for one signed device-session challenge. It is not a
-- credential, a refresh secret or an authorization grant: the bearer token is
-- never stored, nothing here can mint a token, and project access is re-read from
-- the live membership tables on every request. The permanent record of an
-- issuance is its operation_audit row. Rows older than a day are deleted by the
-- next issuance for the same key; challenges expire after two minutes.
create table device_session (
    id uuid primary key,
    key_id uuid not null references device_signing_key(id),
    identity_id uuid not null references identity(id),
    project_id uuid not null,
    scopes text[] not null check (
        cardinality(scopes) between 1 and 3
        and scopes <@ array['history:read','history:write','project:read']::text[]
    ),
    issued_at timestamptz not null default now(),
    expires_at timestamptz not null,
    check (expires_at > issued_at and expires_at <= issued_at + interval '1 hour')
);
create index device_session_key_issued on device_session(key_id, issued_at);

revoke all on device_session from public;
