# Local retention plan

13 October 2026. A local project repository can now report what a future retention
policy could affect. The command is **read-only groundwork**. There is still no
garbage collector, no pruning, and no way to delete a retained object or commit.
SQLite schema 2 and the PostgreSQL service schema are unchanged.

```text
engine-cli repository DATABASE PROJECT_UUID gc-plan [--top N] [--list N] [--format json|text]
```

The library entry point is `LocalRepository::gc_plan`, beside `inventory`. The
database is opened with `LocalRepository::open_read_only`: it never creates a
missing file, never upgrades a schema 1 database, and every connection is opened
with SQLite's `query_only` setting, so a write fails inside SQLite rather than
relying on the code to behave. `gc_plan` also reads through its own private
`query_only` connection when called on an ordinary writable handle.
All reads share one snapshot, so a concurrent writer cannot make the numbers
inconsistent. The plan holds that snapshot while it runs; a very long plan keeps
SQLite from resetting the write-ahead log until it finishes.

## What it computes

Roots are every branch head. A commit keeps its ancestry parents reachable, and also
the source and mainline named by a recovery (revert or cherry-pick) commit, because
those are signed provenance references that transfer must be able to resolve. An
object is reachable when a reachable commit's file state graph references it. The
same bytes stored under another document are a separate object, exactly as in the
capacity accounting.

Everything retained but not reachable is reported in three categories:

- **staged**: stored objects that no stored commit references. These are the result
  of `store-state`, an interrupted transfer, or objects imported before their commit.
  Deleting them could break a resumable transfer.
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

Counts are encoded payload bytes, the same quantity the capacity limits charge.
`reclaimable_bytes_estimate` is the unreachable object and commit bytes. It is an
estimate of logical payload only: SQLite file size, indexes, WAL, free pages and the
separate receipt journal are not included, and SQLite would need a vacuum to return
space even after a delete. The plan also cross-checks scanned totals against the
persistent capacity counters and reports whether they agree.

The JSON has no timestamps or host-specific values and uses ordered collections,
so the same repository state always gives identical bytes. `--format text` prints
a summary of the same facts.

## Cost and limits

With C commits, O objects and F file entries across all commits: one pass over the
commit table (one signature check per commit), graph work proportional to commits
and parent/source edges, one read per distinct reachable object, and at most F set
lookups for file roots. Shared subtrees are read once, so a long history of small
edits costs roughly the number of distinct objects, not history length times file
size. File states are never reconstructed; one encoded object (at most 1 MiB) and
one page of commit bodies are resident at a time. Memory is proportional to
commits plus objects (about 100 bytes per commit and 64 per object, so under
about 400 MB at the 4 million object / 1 million commit tracking limits, far above
the default 1,000,000 / 100,000 capacity limits). A repository above the tracking
limits is refused with an explicit limit error rather than planned partially. A
missing, corrupt or tampered object or commit fails the plan closed; it is never
planned around. This bounded in-memory design has been exercised with a synthetic
history only; it is not a large-document or multi-gigabyte measurement.

## What deletion would additionally require (not implemented)

Producing a plan is the easy half. Before anything is removed the owner must decide:

- **Signed-commit retention.** Commits carry signatures that other replicas, receipts
  and recovery references may depend on. Is an unreachable signed commit evidence to
  be kept for a minimum period, or only until its objects are unreferenced?
- **Publication receipts.** A server receipt names a commit and branch position. A
  local commit that was once published must remain resolvable for as long as that
  receipt can be presented, and the receipt journal is separate from this database.
- **Audit expectations.** Removing history changes what can be independently shown
  later. Detecting loss of the newest history already needs an independently retained
  head or witness; deletion needs an agreed record of what was removed and why.
- **Grace periods and concurrency.** Staged objects of an interrupted transfer look
  identical to garbage. A collector needs a minimum age (the schema has none today)
  and a way to exclude concurrent importers, and must keep the capacity counters
  correct.
- **Immutability guards.** The object and commit tables reject `delete` through
  triggers by design. A collector would need an explicit, audited path that is
  separate from ordinary writes.
- **Remote interaction.** Deletion must not remove content a protected remote ref or
  another replica still needs to fetch, and does not change server-side retention.

Until those decisions are made, use `storage` and `storage-limits` to inspect and
explicitly raise capacity; the engine continues to retain everything.
