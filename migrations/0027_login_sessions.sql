-- Bind new access tokens and every refresh successor to one revocable login.
-- Existing refresh tokens retain NULL: logout of a legacy cookie invalidates the
-- account generation, since its bearer token has no individual session identity.
alter table refresh_token add column session_id uuid;
create index refresh_token_session_idx on refresh_token(identity_id,session_id)
    where session_id is not null;
create unique index refresh_token_active_session_idx on refresh_token(identity_id,session_id)
    where session_id is not null and rotated_at is null and revoked_at is null;
