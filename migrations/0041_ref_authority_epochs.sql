-- Receipt-authority epochs: planned rotation and recorded compromise/loss recovery.
-- See docs/REF-AUTHORITY-ROTATION.md.
--
-- project_ref_authority (migration 0036) stays the immutable, operator-installed
-- anchor of epoch 1. Every later key is an epoch row whose signed statement is
-- stored verbatim. Rows are only ever appended; the runtime login has SELECT only,
-- and operators write through engine-cli with the migration login.
--
-- Receipts keep their version-1 encoding: they identify their epoch by issuer_key,
-- and a key belongs to exactly one epoch (unique public_key).
create table project_ref_authority_epoch (
    epoch integer primary key check (epoch between 1 and 1024),
    public_key text not null unique check (public_key ~ '^[0-9a-f]{64}$'),
    kind text not null check (kind in ('genesis', 'rotation', 'recovery')),
    predecessor integer unique references project_ref_authority_epoch(epoch),
    statement bytea check (octet_length(statement) between 1 and 4096),
    statement_address text check (statement_address ~ '^[0-9a-f]{64}$'),
    sealed_receipts bigint check (sealed_receipts >= 0),
    sealed_digest text check (sealed_digest ~ '^[0-9a-f]{64}$'),
    installed_at timestamptz not null default now(),
    check ((epoch = 1) = (predecessor is null)),
    check (predecessor is null or predecessor = epoch - 1),
    check ((kind = 'genesis') = (epoch = 1)),
    check ((statement is null) = (kind = 'genesis')),
    check ((statement_address is null) = (statement is null)),
    check ((sealed_receipts is null) = (statement is null)),
    check ((sealed_digest is null) = (statement is null))
);
-- One unsigned marker per epoch: the operator's note that the key must not be
-- trusted, with the best-known effective time. It cannot be edited or removed.
create table project_ref_authority_revocation (
    epoch integer primary key references project_ref_authority_epoch(epoch),
    effective_at timestamptz not null,
    recorded_at timestamptz not null default now(),
    reason text not null check (octet_length(reason) between 1 and 500),
    check (effective_at <= recorded_at)
);

create trigger ref_authority_epoch_no_change before update or delete on project_ref_authority_epoch
    for each row execute function event_immutable();
create trigger ref_authority_epoch_no_truncate before truncate on project_ref_authority_epoch
    for each statement execute function event_immutable();
create trigger ref_authority_revocation_no_change before update or delete on project_ref_authority_revocation
    for each row execute function event_immutable();
create trigger ref_authority_revocation_no_truncate before truncate on project_ref_authority_revocation
    for each statement execute function event_immutable();

-- An installation of the original key is epoch 1. Existing installations are
-- backfilled below; later installs create it through this trigger.
create function project_ref_authority_genesis() returns trigger
language plpgsql set search_path = pg_catalog, public, pg_temp as $$
begin
    insert into public.project_ref_authority_epoch(epoch, public_key, kind)
        values (1, new.public_key, 'genesis');
    return new;
end $$;
create trigger project_ref_authority_genesis after insert on project_ref_authority
    for each row execute function project_ref_authority_genesis();
insert into project_ref_authority_epoch(epoch, public_key, kind)
    select 1, public_key, 'genesis' from project_ref_authority;

-- The signed statement is verified by the operator CLI and by clients. The
-- database refuses an epoch whose stored statement disagrees with its columns,
-- whose predecessor or seal is wrong, or whose kind does not fit the predecessor's
-- revocation state. Operators hold a table lock on project_ref_event while this
-- runs, so the seal count cannot move.
create function project_ref_authority_epoch_guard() returns trigger
language plpgsql set search_path = pg_catalog, public, pg_temp as $$
declare
    prior record;
    body jsonb;
    revoked boolean;
begin
    if new.epoch = 1 then
        return new;
    end if;
    select epoch, public_key into prior from public.project_ref_authority_epoch
        order by epoch desc limit 1;
    if not found or prior.epoch <> new.epoch - 1 then
        raise exception 'authority epochs must be consecutive' using errcode = '23514';
    end if;
    begin
        body := convert_from(new.statement, 'UTF8')::jsonb;
    exception when others then
        raise exception 'authority statement is not JSON' using errcode = '23514';
    end;
    if body #>> '{transition,authority}' is distinct from
           (select id::text from public.device_authority where singleton)
       or (body #>> '{transition,from_epoch}')::integer is distinct from prior.epoch
       or (body #>> '{transition,to_epoch}')::integer is distinct from new.epoch
       or body #>> '{transition,old_key}' is distinct from prior.public_key
       or body #>> '{transition,new_key}' is distinct from new.public_key
       or body #>> '{transition,kind}' is distinct from new.kind
       or (body #>> '{transition,sealed,receipts}')::bigint is distinct from new.sealed_receipts
       or body #>> '{transition,sealed,digest}' is distinct from new.sealed_digest
       or (new.kind = 'rotation') is distinct from (body ? 'old_signature')
       or not (body ? 'new_signature') then
        raise exception 'authority statement differs from its epoch row' using errcode = '23514';
    end if;
    if new.sealed_receipts <> (select count(*) from public.project_ref_event) then
        raise exception 'authority seal is stale: receipts changed' using errcode = '23514';
    end if;
    revoked := exists (select 1 from public.project_ref_authority_revocation where epoch = prior.epoch);
    if new.kind = 'rotation' and revoked then
        raise exception 'a revoked authority epoch cannot sign its own successor' using errcode = '23514';
    end if;
    if new.kind = 'recovery' and not revoked then
        raise exception 'recovery requires a revocation marker on the previous epoch' using errcode = '23514';
    end if;
    return new;
end $$;
create trigger ref_authority_epoch_guard before insert on project_ref_authority_epoch
    for each row execute function project_ref_authority_epoch_guard();

-- Defence in depth for issuance: a receipt may only be stored when it was signed
-- with the active epoch's key and that epoch is not revoked. The API checks this
-- first; the trigger holds even if API code is bypassed by the runtime login.
create function project_ref_event_issuer_guard() returns trigger
language plpgsql set search_path = pg_catalog, public, pg_temp as $$
declare
    issuer text;
    active record;
begin
    begin
        issuer := convert_from(new.receipt, 'UTF8')::jsonb #>> '{receipt,issuer_key}';
    exception when others then
        issuer := null;
    end;
    select e.epoch, e.public_key,
           exists (select 1 from public.project_ref_authority_revocation r where r.epoch = e.epoch) as revoked
        into active from public.project_ref_authority_epoch e order by e.epoch desc limit 1;
    if not found then
        raise exception 'no receipt authority is installed' using errcode = '23514';
    end if;
    if active.revoked then
        raise exception 'the active receipt authority epoch is revoked' using errcode = '23514';
    end if;
    if issuer is distinct from active.public_key then
        raise exception 'receipt was not signed by the active authority epoch' using errcode = '23514';
    end if;
    return new;
end $$;
create trigger project_ref_event_issuer before insert on project_ref_event
    for each row execute function project_ref_event_issuer_guard();

create function project_ref_authority_revocation_guard() returns trigger
language plpgsql set search_path = pg_catalog, public, pg_temp as $$
begin
    if new.effective_at > now() then
        raise exception 'revocation cannot take effect in the future' using errcode = '23514';
    end if;
    return new;
end $$;
create trigger ref_authority_revocation_guard before insert on project_ref_authority_revocation
    for each row execute function project_ref_authority_revocation_guard();

revoke all on project_ref_authority_epoch, project_ref_authority_revocation from public;
revoke execute on function
    project_ref_authority_genesis(),
    project_ref_authority_epoch_guard(),
    project_ref_event_issuer_guard(),
    project_ref_authority_revocation_guard()
    from public;
