-- The installation identity is not a signing secret or an external audit anchor.
create table device_authority (
    singleton boolean primary key default true check (singleton),
    id uuid not null unique default gen_random_uuid()
);
insert into device_authority(singleton) values(true);

-- Public key bindings and revocations are retained with pseudonymous authorship.
-- No device names, emails, private keys or bearer credentials are stored here.
create table device_signing_key (
    id uuid primary key,
    identity_id uuid not null references identity(id),
    public_key text not null unique check (public_key ~ '^[0-9a-f]{64}$'),
    registered_at timestamptz not null default now(),
    proof jsonb not null check (octet_length(proof::text) <= 2048)
);
create index device_signing_key_identity on device_signing_key(identity_id, registered_at, id);
create table device_key_revocation (
    key_id uuid primary key references device_signing_key(id),
    revoked_at timestamptz not null default now()
);
create table device_enrollment_challenge (
    id uuid primary key,
    identity_id uuid not null references identity(id),
    expires_at timestamptz not null,
    challenge jsonb not null check (octet_length(challenge::text) <= 2048)
);
create index device_enrollment_identity on device_enrollment_challenge(identity_id, expires_at);

do $$ declare t text; begin
    foreach t in array array['device_authority','device_signing_key','device_key_revocation'] loop
        execute format('create trigger immutable_update before update or delete on %I for each row execute function event_immutable()', t);
        execute format('create trigger immutable_truncate before truncate on %I for each statement execute function event_immutable()', t);
    end loop;
end $$;

-- Account removal permanently revokes existing device keys. Restoration cannot
-- reactivate them. The same identity row serializes registration and removal.
create function revoke_removed_account_devices() returns trigger language plpgsql as $$
begin
    if (new.disabled_at is not null and old.disabled_at is null)
       or (new.erased_at is not null and old.erased_at is null) then
        insert into device_key_revocation(key_id)
            select id from device_signing_key where identity_id=new.id
            on conflict do nothing;
        delete from device_enrollment_challenge where identity_id=new.id;
    end if;
    return new;
end $$;
create trigger identity_revoke_devices after update on identity
    for each row execute function revoke_removed_account_devices();

revoke all on device_authority,device_signing_key,device_key_revocation,device_enrollment_challenge from public;
revoke execute on function revoke_removed_account_devices() from public;
