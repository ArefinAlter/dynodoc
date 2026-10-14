# Local retention plan and collector

13 October 2026. A local project repository can report what a retention policy could
affect (`gc-plan`, read-only) and can delete exactly one class of data: stored
objects that no stored commit references (`gc`). It never deletes a commit, a
signature, a branch ref, receipt-journal data or any object a commit or a
recovery source reaches. The PostgreSQL service schema is unchanged; the local
SQLite schema is now 3 because of compression at rest (see
[local repository capacity](LOCAL-STORAGE-BUDGETS.md)), which retention only
reports; the first deleting run adds one append-only log table (see "Idempotency
and the log").

```text
engine-cli repository DATABASE PROJECT_UUID gc-plan [--top N] [--list N] [--format json|text]
engine-cli repository DATABASE PROJECT_UUID gc --plan-hash HASH [--vacuum] [--format json|text]
```

The library entry points are `LocalRepository::gc_plan` and
`LocalRepository::gc_apply`, beside `inventory`.

## Procedure

1. Quiesce transfers and anything preparing a commit from `store-state` output
   (see "Grace periods" below).
2. `gc-plan --format text` and review the staged objects, the pinned reasons and
   the unreachable commits. The JSON carries the same facts and a `plan_hash`.
3. `gc --plan-hash HASH`. The collector recomputes the plan inside one write
   transaction and refuses unless its hash is identical, so a branch that moved or
   an object that arrived since the review means nothing is deleted. Run
   `gc-plan` again and review again.
4. Optionally add `--vacuum` (below).

The result lists deleted objects and encoded bytes, what is still retained
(total, reachable, unreachable-but-kept) and the capacity counters before and
after. Exit status is non-zero for a refusal; a failed `--vacuum` after a
committed deletion prints the full report, says so, and exits non-zero.

## The plan (read-only)

The database is opened with `LocalRepository::open_read_only`: it never creates a
missing file, never upgrades a schema 1 database, and every connection is opened
with SQLite's `query_only` setting, so a write fails inside SQLite rather than
relying on the code to behave. `gc_plan` also reads through its own private
`query_only` connection when called on an ordinary writable handle.
All reads share one snapshot, so a concurrent writer cannot make the numbers
inconsistent. The plan holds that snapshot while it runs; a very long plan keeps
SQLite from resetting the write-ahead log until it finishes.

### What it computes

Roots are every branch head. A commit keeps its ancestry parents reachable, and also
the source and mainline named by a recovery (revert or cherry-pick) commit, because
those are signed provenance references that transfer must be able to resolve. An
object is reachable when a reachable commit's file state graph references it. The
same bytes stored under another document are a separate object, exactly as in the
capacity accounting.

Everything retained but not reachable is reported in three categories:

- **staged**: stored objects that no stored commit references. These are the result
  of `store-state`, an interrupted transfer, objects imported before their commit,
  or a merge/recovery state whose commit was never imported. **Only this category
  is ever collected.**
- **superseded**: unreachable commits whose ancestry or provenance joins retained
  history, with the objects only they reach. Typical causes are a branch moved away
  from a line (a reset), or a commit that was imported and never published.
- **orphaned**: unreachable commits with no link at all to retained history, with
  the objects only they reach.

An object shared by a reachable and an unreachable commit is reachable. When an
object is reached by several unreachable commits it is counted once, in the
strongest category (superseded before orphaned).

The result also lists why reachable data stays pinned, largest first (at most
`--top`, default 10; the count of smaller reasons is reported): `branch-head:NAME`
for the head commit's own files, `branch-history:NAME` for ancestors, and
`recovery-source` for commits kept only by a recovery reference. Each object and
commit is attributed once, to the first reason in that order and, within a reason
type, to the first branch name in sorted order. `--list` (default 10) adds examples:
unreachable commits and staged objects in address order.

Counts are encoded payload bytes, the same quantity the capacity limits charge:
the canonical encoded length of each object, which compression at rest does not
change, so the plan and its hash are the same before and after `repository ...
compact`. The plan's `physical` section reports what the rows actually hold
(`stored_object_bytes`, `stored_commit_bytes`, `compressed_objects` and the logical
bytes they stand for) and the collector report gives `deleted.stored_bytes` and the
`stored_bytes` counter before and after, beside the file sizes `--vacuum` reports.
`reclaimable_bytes_estimate` is the unreachable object and commit bytes. It is an
estimate of logical payload only: SQLite file size, indexes, WAL, free pages and the
separate receipt journal are not included. **`gc` reclaims only the staged part of
that estimate**; superseded and orphaned commits and their objects stay. The plan
also cross-checks scanned totals against the persistent capacity counters and
reports whether they agree.

The JSON has no timestamps or host-specific values and uses ordered collections,
so the same repository state always gives identical bytes. `--format text` prints
a summary of the same facts.

### The plan hash

`plan_hash` is a SHA-256 under the domain `dynodoc.local-retention-plan-hash\0v1\0`
over: project, every branch name, head address and generation (sorted by name),
every unreachable commit (address, category, encoded size, sorted),
every superseded and orphaned object (document, address, size, sorted), every
staged object (document, address, size, in key order) and the category totals.
It depends on neither `--top` nor `--list`, nor on the schema number (a plan taken
on a schema 1 database stays valid across the upgrade `gc` performs when it opens
it). Anything that changes what could be
deleted, or why something is kept, changes the hash: moving a branch (the
generation also changes on a move back), importing a commit, adding or removing an
object.

## The collector

`gc` runs one `BEGIN IMMEDIATE` transaction, so SQLite's writer lock is held before
any read:

1. It refuses a read-only handle and anything other than local schema 3 (opening
   a schema 1 or 2 database with a writable handle upgrades it first, exactly as
   for every other write; a missing database is an error, never created).
2. It recomputes the plan from the rows it is about to modify and refuses when the
   capacity counters disagree with a fresh count of the stored rows.
3. It compares the recomputed hash with `--plan-hash`. A mismatch is a stale
   plan and nothing changes.
4. It deletes each stored object that no stored commit's file state graph reaches.
   The object table's immutability trigger is dropped and recreated inside this
   same transaction, so no other connection ever observes it missing, and a
   rollback restores it. The stored definition is compared afterwards; a missing
   or altered guard fails the run. Commit and ref tables are never written.
5. It lowers the persistent counters by exactly the deleted rows and bytes (the
   logical bytes the limits charge, and the physical bytes the rows held), then
   checks that they equal a fresh recomputation (objects, bytes, stored bytes,
   commits, branches) and that the surviving object count equals the number of
   objects some commit reaches.
6. It appends one row to `local_retention_log` and commits.

Any error, including a failure at each of those steps, rolls the transaction back:
no row is removed, the guard and counters are untouched, and the log row is absent.
A second connection that holds the writer lock makes `gc` wait up to SQLite's busy
timeout (5 seconds) and then fail with a database error; it never reads a mixed
state, and a writer that commits first changes the plan so `gc` refuses as stale.

### Idempotency and the log

Repeating a hash that was already applied returns `already_applied: true` and
changes nothing, even though the repository's recomputed hash is now different.
The record is the append-only table `local_retention_log` (sequence, plan hash,
deleted objects and bytes, remaining objects and bytes, wall-clock milliseconds).
It is created inside the first deleting run's transaction, its rows are protected
by update/delete triggers, and the local schema number, the capacity counters and
earlier clients do not depend on it. A run that has nothing to delete (a matching hash with
no staged objects) writes nothing. Owner-level edits to the SQLite file remain
outside any trigger's protection.

### `--vacuum`

Deleted rows free SQLite pages, not file size. `--vacuum` runs after the deletion has
committed, on its own connection: `PRAGMA wal_checkpoint(TRUNCATE)`, `VACUUM`, then
a second checkpoint. It needs free disk space about the size of the database file
while it runs, and it fails if another connection holds an open transaction. The
report gives the database file plus write-ahead log size before and after. A
vacuum failure never undoes the deletion; it is reported in `vacuum.error`.

## Cost and limits

With C commits, O objects and F file entries across all commits: one pass over the
commit table (one signature check per commit), graph work proportional to commits
and parent/source edges, one read per distinct reachable object, and at most F set
lookups for file roots, plus one key-ordered scan of the object table (identifiers
and sizes only) whenever anything is staged, because the plan hash binds every
staged key. Shared subtrees are read once, so a long history of small
edits costs roughly the number of distinct objects, not history length times file
size. File states are never reconstructed; one encoded object (at most 1 MiB) and
one page of commit bodies are resident at a time. Memory is proportional to
commits plus objects (about 100 bytes per commit and 64 per object, plus about 56
bytes per superseded or orphaned object for the hash, so under about 650 MB at the
4 million object / 1 million commit tracking limits, far above the default
1,000,000 / 100,000 capacity limits). A repository above the tracking limits is
refused with an explicit limit error rather than planned partially. A missing,
corrupt or tampered object or commit fails the plan and the collection closed; it
is never planned around.

`gc` holds the writer lock while it recomputes: other writers wait up to the busy
timeout, so run it when nothing else is writing. Measured once (debug build,
Windows, synthetic): a repository with 32 commits and about 20,000 staged
objects of 2.7 MB planned in 3.1 s and was collected, with `--vacuum`, in
6.7 s. That is a smoke test of the bounded design, not a large-document or
multi-gigabyte measurement.

## What is deleted, what is never deleted

Deleted: stored objects outside every stored commit's file state graph.

Never deleted: commits and their signatures (including unreachable and
orphaned ones, and the ones a receipt or recovery reference may name); branch
refs and generations; every object any stored commit reaches, even when only an
unreachable commit or a recovery source reaches it; the receipt journal and
other trust data (not in this database); the capacity limits; the shared
PostgreSQL objects, which stay immutable except through audited erasure.

## Grace periods and in-progress work

The schema records no age for staged objects, so an object written seconds ago for a
transfer or for a commit you are preparing is indistinguishable from garbage.
The hash makes the deletion deliberate, not safe against intent: if you run
`store-state` or import objects after reviewing, the hash is stale and `gc` refuses;
if you review while a transfer is half-finished, `gc` will delete its already
received objects. The sender can negotiate again and send them (`missing` is
computed from what is stored), and a commit prepared from a deleted state fails
with a missing-content error and must be prepared again, but nothing retained is
damaged. Do not run `gc` during a transfer.

## What remains

- **No commit pruning.** Superseded and orphaned commits, and every object they
  reach, are retained. Removing them needs a policy decision about signed-commit
  evidence, publication receipts that name a commit, and independently retained
  heads or witnesses (detecting loss of the newest history already needs one).
- **No pack files or compression.** Objects are single rows; space returns only
  through `--vacuum`.
- **No minimum age or grace period** in the schema, and no coordination with an
  active importer beyond the writer lock and the plan hash.
- **No remote retention.** Deleting local staged objects does not change what a
  protected remote ref or another replica holds, and shared PostgreSQL content
  objects are immutable except through audited erasure.
- **No scheduled or automatic collection.** The engine keeps retaining everything
  unless an operator runs `gc`; use `storage` and `storage-limits` to inspect and
  explicitly raise capacity.
