# Local repository capacity

12 October 2026, revised 14 October 2026. Portable project repositories use
SQLite schema **3**: schema 2 (12 October) added the persistent capacity limits
described first below, and schema 3 (14 October) added compression at rest, described
under [Compression at rest](#compression-at-rest-schema-3). The PostgreSQL service
schema remains 34 in source and 29 in production.

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
signatures or refs. (Schema 2 to 3 is described under compression at rest.) If a dimension already exceeds its default, its initial limit
is that existing usage; subsequent growth needs an explicit increase. Missing
accounting in a schema-2 database fails closed. Unsupported newer schemas are
rejected. Schema-1-only binaries reject schema 2 and schema-2 binaries reject
schema 3: take a filesystem-consistent backup before upgrading if you need to
return to an older local client. There is no in-place downgrade or format
conversion of signed commits.

This bounds admitted payload bytes and counts, **not physical disk use**. SQLite
pages, indexes, WAL, temporary working space, backups, native documents, credentials
and the separate receipt trust journal are outside this accounting. There is no
whole-device quota or commit retention/pruning policy; per-object compression at
rest is below and multi-object packs are only a [design](DESIGN-PHYSICAL-PACK-FILES.md).
The `gc` command removes only staged objects that no stored commit references, after
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

## Compression at rest (schema 3)

14 October 2026, source only (no release records it). Object rows are stored
compressed when that saves at least a tenth. This changes how bytes sit in the
SQLite row and nothing else: object hashes and addresses, the quota counters and
limits, `export-object` and `import-objects` packets, the retention plan and its
hash, and every verification are about the canonical encoded bytes, as before.

**Row format.** `objects` gains `codec` (0 identity, 1 raw DEFLATE; any other
positive number is reserved) and `size` (the logical length; NULL on rows written
before schema 3, which are identity). An object of at least 128 bytes is stored
compressed only when the result is at most 90 percent of its size, with
`miniz_oxide` at level 6 (pure Rust, already in the dependency graph through
`flate2`). Small objects and incompressible ones stay identity, so a database is a
mix and both kinds read through the same path. The table's constraints refuse a
compressed row whose declared size is above the 1 MiB object limit or not larger
than what is stored.

**Bounded decoding.** A read takes the declared size, checks it against the object
limit before inflating anything, inflates with that size as a hard output limit and
requires exactly that many bytes, then verifies the object against its address as
for any other row. A decompression bomb, a truncated or flipped stream, a wrong
declared length and an unknown codec are errors (tests cover each).

**Counters.** `bytes_used` and the byte limit stay logical, so a repository that
fit before still fits and `gc-plan` totals and hashes are unchanged. A new
persistent counter, `stored_bytes`, is the physical payload: object rows as stored
plus commit bodies (which are not compressed). `storage` prints it beside
`bytes_used`; `gc-plan` prints a `physical` section (stored object and commit
bytes, compressed object count and the logical bytes they stand for); `gc` prints
`deleted.stored_bytes` and the counter before and after; `gc --vacuum` and
`compact --vacuum` report the database plus write-ahead log size. None of these is
SQLite page, index or WAL accounting: the file is larger than the payload (see the
measurements).

**Upgrade.** Opening a schema 2 database with this build upgrades it in the same
initialization transaction as before: two columns, one counter and trigger
replacements, no row is rewritten and nothing is compressed yet. A schema-2 build
rejects schema 3 exactly as schema-1-only builds rejected schema 2 (verified with a
real earlier binary: `local schema/project`). There is no downgrade; keep a
filesystem backup if you may need an older client.

**Compaction.**

```text
engine-cli repository DATABASE PROJECT_UUID compact [--batch N] [--max-objects N] [--after-document UUID --after-address ADDRESS] [--vacuum] [--format json|text]
```

Re-encodes identity rows in bounded `BEGIN IMMEDIATE` batches (at most 1,024 rows
and about 8 MiB read per transaction). Each object is checked against its address
before it is replaced and must inflate back to the identical bytes; each batch
checks that the logical counters are unchanged and the physical counter fell by
exactly what the batch saved. The only way to update an object row is inside such a
transaction (the immutability trigger refuses it otherwise). An interrupted run
keeps every committed batch and a second run finishes the rest; a run that has
nothing left rewrites nothing. `--max-objects` plus the printed `next` cursor
resumes a long run in pieces. It does not shrink the file by itself: `--vacuum`
then checkpoints the write-ahead log and runs `VACUUM`, which needs free disk space
about the size of the database.

**Measured.** The experiment [local-compression.mjs](benchmarks/local-compression.mjs)
builds the same 33-revision graph with a schema-2 CLI and the schema-3 CLI and
compares sizes, inventories and addresses; the full report, including codec and
threshold sweeps, is
[local-compression-20261014.json](benchmarks/local-compression-20261014.json).
Debug CLI builds, i7-1255U, Windows 11. To reproduce, build `engine-cli` from a
revision before schema 3 (the baseline writes identity rows) and from this one, then
run `node docs/benchmarks/local-compression.mjs NEW_CLI OLD_CLI NEW_DIRECTORY 128 32 docs`
(the last argument is `repetitive` or `docs`; the directory must not exist). The
script uses and removes a publicly known synthetic signing key and writes only
synthetic data. The `docs` fixture reads this repository's `docs/*.md`, so its
paragraph pool (size and SHA-256 are in the report) changes as documentation does.

| Fixture | Logical bytes | Stored bytes | Stored / logical | SQLite files before | after `compact --vacuum` |
|---|---:|---:|---:|---:|---:|
| repetitive label, 128 paragraphs | 273,586 | 170,872 | 0.625 | 602,112 | 483,328 |
| repetitive label, 512 paragraphs | 736,893 | 462,058 | 0.627 | 1,708,032 | 1,347,584 |
| real prose (this repository's docs), 128 paragraphs | 292,468 | 205,682 | 0.703 | 630,784 | 516,096 |
| real prose, 512 paragraphs | 786,027 | 570,958 | 0.726 | 1,757,184 | 1,454,080 |

The `repetitive` fixture is the label of the earlier growth benchmark and flatters
compression; `docs` is closer to real text. Payload falls by 27 to 37 percent and
the files by 17 to 21 percent, because most of the file is per-row key and index
overhead (about 240 bytes per object against a 145-byte median object), which
compression does not touch; the report quantifies a narrower key layout.
Writing the same graph directly compressed (no compaction step) gives files within
about 4 percent of the compacted-and-vacuumed ones. The codec comparison, the
in-process write and read cost in release mode (writes took 12 to 50 percent longer
on a loaded machine; reads unchanged) and the 20,000-object compaction test (about
20,000 rows examined in under 10 seconds in a debug build) are in the report and
the tests. What this does not measure: formatting runs, assets, concurrent
branches, peak memory, or any real user repository.

**Commit pruning design.** [DESIGN-COMMIT-PRUNING.md](DESIGN-COMMIT-PRUNING.md) and
[ENGINE-REMAINING-DESIGNS.md](ENGINE-REMAINING-DESIGNS.md) name the local schema
for horizons "schema 3" and say existing databases stay at schema 2. Schema 3 is
now compression at rest, so that design's local schema becomes 4 and its upgrade
starts from 3. Those documents were not edited by this change.
