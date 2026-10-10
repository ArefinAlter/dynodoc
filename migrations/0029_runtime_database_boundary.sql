-- A dedicated migration owner applies DDL. The API gets explicit DML grants via
-- engine-cli grant-runtime, never ownership or migration credentials.
revoke create on schema public from public;
do $$ begin
    execute format('revoke create, temporary on database %I from public', current_database());
end $$;
revoke all on all tables in schema public from public;
revoke all on all sequences in schema public from public;
revoke execute on all functions in schema public from public;
alter default privileges in schema public revoke all on tables from public;
alter default privileges in schema public revoke all on sequences from public;
-- Per-schema REVOKE cannot subtract the built-in global PUBLIC EXECUTE default.
-- Clear both the global default and any previous schema-specific additions.
alter default privileges revoke execute on functions from public;
alter default privileges in schema public revoke execute on functions from public;

-- Only these existing trigger entrypoints need to delete immutable children.
-- Their existing same-transaction receipt/trash guards still apply. They cannot
-- be invoked as ordinary SQL functions or installed on attacker-owned tables by
-- the runtime role. Explicit pg_temp last prevents implicit temporary shadowing.
alter function erase_document_children() security definer;
alter function erase_document_children() set search_path = pg_catalog, public, pg_temp;
alter function erase_shared_checkpoint_children() security definer;
alter function erase_shared_checkpoint_children() set search_path = pg_catalog, public, pg_temp;
alter function erased_identity_guard() security definer;
alter function erased_identity_guard() set search_path = pg_catalog, public, pg_temp;

-- Runtime roles have no DELETE on history. The definer triggers perform approved
-- erasure; ordinary reads/inserts and mutable operational tables stay invoker-only.
-- This migration creates no cluster-wide roles and never embeds passwords.
