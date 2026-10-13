# Local repository capacity

12 October 2026. Portable project repositories now use SQLite schema **2**.
The PostgreSQL service schema remains 34 in source and 29 in production.

Every local project has persistent limits on retained encoded history: **4 GiB**
for object and signed-commit payloads combined, **1,000,000 objects**,
**100,000 commits**, and **4096 branches** by default. These are adjustable local
capacity limits, not a history age cutoff. A revision fifty commits back remains
readable. The owner must explicitly increase capacity to retain additional data;
the engine never silently removes history or trust evidence to make room.

Objects count even before they belong to a commit or branch. Restarting a transfer,
adding a file, reopening the database, or creating another connection does not
reset the counters. A content object is scoped by document: the same bytes under
two document identities count twice. Identical retained objects and commits are
free to reuse at capacity. Moving an existing branch consumes no additional slot.

SQLite AFTER INSERT triggers account for actual new rows in the same transaction.
They check available capacity before updating counters. A failed graph write,
transfer batch, commit import, or branch publication rolls back that operation;
separately committed earlier transfer batches remain retained and charged. Quota
errors are explicit limits, distinct from I/O or integrity failures. Existing
immutable-object and immutable-commit guards remain in force.

`LocalRepository::storage_usage()` reads persisted usage/limits.
`set_storage_limits(StorageLimits)` updates all four limits atomically; negative
values and limits below current usage are rejected. Zero is allowed for an empty
dimension. CLI commands use the same functions:

```text
engine-cli repository DATABASE PROJECT_UUID storage
engine-cli repository DATABASE PROJECT_UUID storage-limits --bytes 8589934592 --objects 2000000 --commits 200000 --branches 8192
```

The second command is an explicit capacity increase. Inspect free disk space and
the usage result first. The CLI does not choose a larger limit automatically.
It prints the persisted result so a connector can expose the same information.
Native connector adoption and a graphical local-capacity flow remain open.

## Compatibility and limits

Opening schema 1 upgrades it transactionally after verifying the project binding.
It counts existing payloads and rows once, without rewriting objects, commits,
signatures or refs. If a dimension already exceeds its default, its initial limit
is that existing usage; subsequent growth needs an explicit increase. Missing
accounting in a schema-2 database fails closed. Unsupported newer schemas are
rejected. Schema-1-only binaries reject schema 2: take a filesystem-consistent
backup before upgrading if you need to return to an older local client. There is
no in-place downgrade or format conversion of signed commits.

This bounds admitted payload bytes and counts, **not physical disk use**. SQLite
pages, indexes, WAL, temporary working space, backups, native documents, credentials
and the separate receipt trust journal are outside this accounting. There is no
whole-device quota, pack compression or commit retention/pruning policy. The
`gc` command removes only staged objects that no stored commit references, after
an exact reviewed plan hash (see LOCAL-RETENTION-PLAN.md). Local filesystem owners remain trusted and can alter a SQLite file;
triggers do not protect against that owner. Remote authorization and independently
retained audit evidence are separate requirements.

The regression suite covers real schema-1 upgrade with retained graphs/signatures
and above-default branch counts, independent byte/count exhaustion, rollback after
partial insertion, exact-capacity reuse, explicit increases, reopen persistence,
concurrent connections, unchanged branch CAS behavior and missing accounting.
Actual commands and results are recorded in the handoff.

## Measured small-history growth

The [reproducible CLI experiment](benchmarks/local-storage-growth.mjs) stores an
initial synthetic paragraph graph and 32 commits, each changing one paragraph.
It verifies exact retained-byte accounting, the unchanged first historical root,
and a final retry adding zero bytes/objects. These are real local CLI calls on
engine `5822e57`, Windows x64, i7-1255U, debug build, with process startup and
database reopen included. Native files, formatting/assets, peak memory, HTTP and
PostgreSQL are not measured. The labels are repetitive ASCII; the comparison is
uncompressed full-state JSON and excludes the copies' own commit metadata.

| Paragraphs | Sum of full JSON states (bytes) | Retained objects + commits (bytes) | SQLite files (bytes) | Median state write (ms) | Unchanged retry (ms) |
|---|---:|---:|---:|---:|---:|
| 128 | 1,896,632 | 273,586 | 589,824 | 280 | 277 |
| 512 | 7,599,032 | 736,893 | 1,695,744 | 1168 | 1523 |

Reports retain every per-revision measurement:
[128 paragraphs](benchmarks/local-storage-growth-128-20261012.json) and
[512 paragraphs](benchmarks/local-storage-growth-512-20261012.json).
The initial object graph has reference/index overhead and is larger than its
JSON state; reuse across subsequent revisions reduces accumulated payload.
Zero additional bytes on retry does not imply constant-time publication:
the writer still visits the complete state and verifies reconstruction. This
measurement supports prioritizing changed-node publication; it does not establish
large-document throughput, compression comparisons or installed-editor latency.

To reproduce, build the CLI and use a directory that does not yet exist:

```text
cargo build --locked -p engine-cli
node docs/benchmarks/local-storage-growth.mjs target/debug/engine-cli NEW_DIRECTORY 128 32
```

On Windows the executable ends in `.exe`. The script uses and removes a publicly
known synthetic signing key; it retains only synthetic data and the report in the
new directory. Never substitute an existing repository or production key.
