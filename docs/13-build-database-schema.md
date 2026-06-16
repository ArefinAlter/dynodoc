# 02 · Database Schema and Migrations

### Claude Code prompt to start this stage

> Building on the scaffolding from stage 01, implement the database schema for the research-ide platform. The full schema is documented in the prior summary message and rests on the design from `research_ide_versioning_complete.md` Volume II §D.1 and §E. For this stage:
>
> 1. Use **sqlx-cli** for migrations. Create migration files numbered `0001_core.sql`, `0002_events.sql`, `0003_snapshots.sql`, `0004_identity.sql`, `0005_moderation.sql`, `0006_deliberation.sql`, `0007_closeout.sql`, `0008_rate_and_signals.sql`, mirroring the schema sections.
> 2. Implement the schema exactly as specified in the build summary, including all indexes. Add the `pgcrypto` and `pg_trgm` extensions in `0000_extensions.sql`.
> 3. Use Postgres `CHECK` constraints on enum-like columns (e.g. `node.type IN ('section', 'item', 'choice', 'clause', 'paragraph')`) to keep the schema self-documenting.
> 4. Add a `0009_views.sql` migration with read-only views: `current_documents` (non-archived), `pending_moderation` (Stage 2 holds + community-flagged), `verified_citizen_count_per_consultation` (for closeout reports).
> 5. In `crates/engine-shared`, write Rust types for every table using `sqlx::FromRow` derive. Use newtype wrappers for `NodeId(String)` (ULID), `EventId(String)` (ULID), `IdentityId(Uuid)`, `DocumentId(Uuid)`. Add `serde` derives.
> 6. Write integration tests in `crates/engine-core/tests/schema_test.rs` that: (a) run all migrations on a fresh database, (b) insert a sample document + node + event, (c) verify the indexes exist via `pg_indexes`, (d) verify the chain_hash column accepts and stores 32-byte values.
> 7. Add a `make migrate` and `make migrate-reset` target. `migrate` runs pending migrations; `migrate-reset` drops the database and re-applies everything (dev only).
>
> Do not implement application logic for events yet — that's stage 03. The goal here is a verified schema you can `psql` into, with type bindings ready to use.

### Why this stage matters more than it looks

Migrations are forever. Once a migration runs in production, you cannot edit it — you can only write a new migration that changes the schema. Investing the time now to get the schema right saves significant pain later. Three things in particular:

**Stable identity columns.** The `node.id` is a ULID string and primary key. It is the load-bearing data-model decision from Volume II §D.1. Never let anything reassign it. Add a Postgres trigger that raises on `UPDATE` of `node.id` for paranoia.

**The hash chain columns are not optional.** `event.content_hash`, `event.chain_hash`, `event.prev_chain_hash` are `NOT NULL`. They are computed application-side inside the event-append transaction (stage 03 handles this). Adding them later when there's data is much harder.

**Append-only semantics enforced by the database.** Add a trigger to the `event` table that raises on `UPDATE` or `DELETE`. The audit guarantee depends on events being immutable at the storage layer, not just in the application's conscience.

```sql
-- Append-only enforcement (add this in 0002_events.sql)
create function event_immutable() returns trigger as $$
begin
  raise exception 'event table is append-only; UPDATE/DELETE forbidden';
end;
$$ language plpgsql;

create trigger event_no_update before update on event
  for each row execute function event_immutable();
create trigger event_no_delete before delete on event
  for each row execute function event_immutable();
```

### Specific decisions to make

**UUID generation.** Use `gen_random_uuid()` (from pgcrypto) on the database side for `document.id`, `identity.id`, `moderation_decision.id`. Use application-side ULID generation for `node.id` and `event.id` because their time-ordered property matters (snapshots benefit from time-sorted node IDs; event seq is monotonic locally but event IDs need to be unique across replicas).

**Timezones.** Always `TIMESTAMPTZ`. Never `TIMESTAMP`. Store everything as UTC; convert at the edge for display.

**JSONB vs separate columns for `node.current_fields`.** Use JSONB. The shape varies by node type (a `section` has different fields than a `choice`), and adding a column per field type would balloon the schema. The cost: you can't put a CHECK constraint on the *content* of the JSONB easily. The fix: validate in the application layer at write time, and use JSONB indexes (`CREATE INDEX ... USING gin (current_fields)`) where you need to query into them.

**Partitioning the event table.** Not yet. Add a comment in `0002_events.sql` noting that when any single document approaches 1M events, partition by `(document_id, seq_range)` using declarative partitioning. At the §2 scale numbers, you won't hit this for years.

### Verification checklist

- [ ] `make migrate-reset && make migrate` runs cleanly.
- [ ] All schema_test integration tests pass.
- [ ] `psql -c "\dt"` shows all expected tables.
- [ ] `psql -c "\di"` shows all expected indexes.
- [ ] Attempting `UPDATE event SET ...` raises the immutability error.
- [ ] Inserting an event without `content_hash` fails with NOT NULL violation.
