-- 0000_extensions — Postgres extensions the dynodoc schema depends on.
--
-- pgcrypto: gen_random_uuid() for db-generated UUID primary keys
--           (document.id, identity.id, snapshot.id, magic_link.id).
-- pg_trgm:  trigram indexes for fuzzy text lookup (document titles, varName
--           search) used by later stages; harmless to enable now.
--
-- Both are idempotent via IF NOT EXISTS so migrate-reset replays cleanly.

create extension if not exists pgcrypto;
create extension if not exists pg_trgm;
