# Design: physical pack files for the local repository

14 October 2026. **Design only: nothing in this document is built.** It records
why the local SQLite repository got per-object compression first (local schema 3),
what multi-object pack files would add on top, what that is worth against what it
costs, using the measurements taken for schema 3, and when it is worth building.
The measurements are in
[local-compression-20261014.json](benchmarks/local-compression-20261014.json);
how to reproduce them is in
[LOCAL-STORAGE-BUDGETS.md](LOCAL-STORAGE-BUDGETS.md).

## 1. Where things stand

Local schema 3 stores each object's canonical encoded bytes in its own row, raw
DEFLATE compressed when that saves at least a tenth and the object is at least 128
bytes. Addresses, quotas (`bytes_used` is logical), transfer packets, the retention
plan hash and every verification are about the canonical bytes and did not change.
A separate counter, `stored_bytes`, reports the physical payload. `repository ...
compact` re-encodes older rows in place.

What a pack file would change is not *whether* objects are compressed but *how
many objects share one compression context and one physical container*. The
measurements below show that the payload is not where most of the space goes.

## 2. What was measured

All figures are this repository's synthetic fixtures (one document of paragraph
nodes, 33 revisions each changing one paragraph, as in the earlier growth
benchmarks) on one Windows 11 machine. `repetitive` is the earlier benchmark's
padded label and flatters compression; `docs` uses real English paragraphs from
this repository's own documentation. Neither has formatting runs, assets, comments
or concurrent branches.

| Fixture | Objects | Logical bytes | Stored bytes | Stored / logical | SQLite files, identity rows | After `compact --vacuum` | Written compressed, same graph |
|---|---:|---:|---:|---:|---:|---:|---:|
| repetitive, 128 paragraphs | 1,075 | 273,586 | 170,872 | 0.625 | 602,112 | 483,328 | 491,520 |
| repetitive, 512 paragraphs | 3,494 | 736,893 | 462,058 | 0.627 | 1,708,032 | 1,347,584 | 1,396,736 |
| docs, 128 paragraphs | 1,075 | 292,468 | 205,682 | 0.703 | 630,784 | 516,096 | 532,480 |
| docs, 512 paragraphs | 3,494 | 786,027 | 570,958 | 0.726 | 1,757,184 | 1,454,080 | 1,507,328 |

Logical bytes include the signed commit bodies (about 19 KB for 33 commits), which
are not compressed. Payload shrinks by 27 to 37 percent; the SQLite files, which are
what a user's disk and backups see, shrink by only 17 to 21 percent.

Object sizes are small. In the 512-paragraph repetitive fixture the median object
is 145 bytes, 47 percent of objects are under 128 bytes and 63 percent under 192
(typed JSON nodes and map index nodes with hex addresses). Text chunks are the
large objects (up to 64 KiB in the format; a few hundred bytes here).

**The file is much larger than the payload, and compression barely changes
that.** For the 512-paragraph fixtures the database files are 1.35 to 1.45 MB for
0.46 to 0.57 MB of stored payload. The cause is the key layout: `objects` is a rowid
table with a `(document text, address text)` primary key, so each object's
36-character document and 64-character address are stored twice, in the row and in
the primary-key index. Re-creating the same rows (same payload, VACUUMed, objects
table only) in other layouts:

| Layout | repetitive-512 file bytes | per object | docs-512 file bytes | per object |
|---|---:|---:|---:|---:|
| as shipped (rowid, text keys) | 1,282,048 | 240 | 1,396,736 | 242 |
| `WITHOUT ROWID`, text keys | 962,560 | 149 | 1,081,344 | 152 |
| `WITHOUT ROWID`, 16-byte document and 32-byte address blobs | 761,856 | 91 | 876,544 | 93 |

"Per object" is file bytes minus stored payload, divided by object count: roughly
240 bytes of keys, headers and page slack against a median object of 145 bytes.

Codec choice (release build, one thread, best of three; small objects are the
objects of the fixtures above at least 128 bytes long, stored only when they save
10 percent):

| Codec | Small objects, repetitive-512: stored / logical | docs-512 | 64 KiB text chunks: stored / logical | compress MB/s | decompress MB/s |
|---|---:|---:|---:|---:|---:|
| deflate level 1 (`miniz_oxide`) | 0.617 | 0.722 | 0.510 | 83 | 145 |
| **deflate level 6 (chosen)** | **0.617** | **0.720** | **0.381** | **20** | **192** |
| zstd level 3 | 0.640 | 0.753 | 0.397 | 73 | 528 |
| zstd level 9 | 0.640 | 0.752 | 0.374 | 26 | 582 |
| zstd level 19 | 0.622 | 0.732 | 0.362 | 2.7 | 201 |
| lz4 block | 0.758 | 0.930 | 0.599 | 242 | 1,910 |

Speeds are for the 64 KiB chunks. For the small objects compression runs at 5 to 14
MB/s and decompression at 20 to 50 MB/s for deflate, because per-call set-up
dominates; the exact figures vary with machine load (it was shared) and are in the
JSON. Findings: deflate matches or beats zstd on small objects and is within 5
percent of the best zstd level on large text; lz4 is not worth its poor ratio here;
zstd's advantage is speed on large chunks, not size. The minimum-size sweep (48, 96, 128, 192, 256
bytes) gave 0.617 / 0.720 at 128 or less, 0.635 / 0.739 at 192 and 0.683 / 0.793 at
256, so 128 was chosen.

Compression cost per small object is dominated by compressor set-up, not data:
building a new `miniz_oxide` compressor per object took about 30 microseconds at
level 6 and reusing one per thread about 17, which is what the store does (the
level makes no difference below a few KiB).

## 3. Why per-object compression first

1. **No format break and no new failure class.** The row stays the unit of
   atomicity, immutability, accounting, deletion and read isolation; the existing triggers, `BEGIN IMMEDIATE` collector, WAL recovery and
   backup tooling all keep working. A pack file adds a second durable artifact whose
   crash consistency with the catalog must be designed and tested.
2. **It moves the right number for the common case at low risk.** About 27 to 37
   percent of the logical payload disappears on these fixtures at roughly 15 to 20
   microseconds per compressed object (release build) and no change to what any
   consumer sees. In-process (release), writing the 33 revisions took 12 to 50
   percent longer than with identity rows across two runs on a loaded machine: one
   compression per visited object, including objects that already exist, and one
   inflation for each duplicate comparison. Reads were unchanged or faster.
3. **Random access stays trivial.** One row, one inflate of at most 1 MiB, with the
   declared size enforced; decompression bombs and corrupted rows are rejected by
   the same bounds as before (see LOCAL-STORAGE-BUDGETS.md).
4. **It preserves every guarantee the retention planner relies on.** Reachability,
   the plan hash and the collector's exact accounting are about logical objects.
5. **It leaves the door open.** The `codec` column accepts any positive number, so
   a later codec (zstd for large chunks, a dictionary codec) needs no table
   rebuild. A build that predates a codec refuses such rows on read, so introducing
   one should still raise the schema number.

## 4. What multi-object packs would add

### 4.1 Shared dictionaries

Per-object compression cannot exploit what *all* objects share: field names,
envelope text, the `DYNOTYP` prefix, record shapes. A trained dictionary (zstd
format) held per repository or per pack lets small objects compress against that
vocabulary. Measured on the corpora above (dictionary of 16 KiB trained on half of
a fixture's objects and applied to the other half, all objects counted, kept
whenever smaller):

| Corpus (test half) | Logical bytes | deflate 6 per object | zstd 3 per object | zstd 3 + 16 KiB shared dictionary |
|---|---:|---:|---:|---:|
| repetitive-512 | 349,373 | 210,717 (60.3%) | 219,922 (62.9%) | 116,857 (33.4%) |
| docs-512 | 378,645 | 268,718 (71.0%) | 282,277 (74.6%) | 178,266 (47.1%) |

The train and test halves are interleaved objects of one synthetic fixture, so this
is an optimistic bound; but the effect is large (about 24 to 27 percentage points
of the logical payload beyond per-object deflate) and **it does not need pack
files**: a dictionary table plus a per-object dictionary id and a zstd codec number
would capture it object by object. The obstacle is the codec, not the container:
`miniz_oxide` has no preset-dictionary support and zstd's encoder is a C library
(`zstd-sys`), which this workspace deliberately does not yet depend on.

This is the main thing per-object deflate cannot do, and it is aimed at the
objects that dominate by count. Its cost is a dictionary lifecycle: training,
versioning (an object must always find the dictionary it was packed with),
storing dictionaries immutably and never garbage-collecting one that any live
object needs.

### 4.2 Delta compression

Successive revisions of one paragraph produce text chunks that differ by a few
words. A delta against the previous version stores almost nothing. A rough
model on this repository's paragraphs (text chunks of at least 400 bytes with a
12-byte in-place edit and a short appended phrase; zstd with the previous version
as a raw dictionary stands in for a delta, against deflate on the successor
alone):

174 chunks (97,826 bytes) compressed to 58,700 bytes (60 percent) with deflate
alone and to 5,878 bytes (6 percent) against the previous version.

The synthetic fixtures above replace whole paragraphs and therefore show *no*
delta benefit in the stored graph; real editing sessions would. The benefit is
proportional to paragraph length and edit locality, which the current fixtures do
not model.

Costs: a base selection policy (which previous object is the base), bounded delta
chains so a read never walks more than a few steps, and a rule that a base is never
removed while a dependent exists, which the retention collector would have to know.
Content addressing is unaffected (the address is of the logical bytes), but
`gc` accounting, `verify` and any repair must be delta-aware.

### 4.3 Append-only pack plus index

Git's design: immutable pack files that are appended or rewritten whole, with a
sorted index. For this repository it would mean:

- **Fewer, larger writes.** One sequential append per transaction instead of
  random B-tree page updates; WAL traffic shrinks.
- **Cheaper per-object metadata.** An index entry is address (32 bytes), pack id,
  offset and length instead of two text keys in a rowid table and its primary-key
  index (see the overhead measurement above, which shows how much of the file that
  is).
- **Cheap whole-pack transfer and backup.** A pack is already a byte stream.

Costs: pack and catalog must publish atomically (write temp pack, fsync, insert
catalog rows, rename, fsync directory; and the reverse on delete), packs must be
repaired or ignored when a crash leaves one without catalog rows, concurrent
writers need pack-level locking, and a reader needs a bounded read path for
partially written trailing packs. SQLite's own atomic commit currently provides all
of this for free.

### 4.4 Reachability-aware repack

Retention's `gc` already deletes exactly the objects no stored commit reaches, and
it never deletes commits (pruning history is a separate, explicit policy: see
[DESIGN-COMMIT-PRUNING.md](DESIGN-COMMIT-PRUNING.md)). A repack would therefore not
remove more *logical* data than `gc`. What it adds is physical: write a new pack of
the objects the planner's reachable set names, grouped by commit generation or
document for locality, with deltas re-based against nearby versions, publish it and
drop the old packs, discarding dead space without a whole-file `VACUUM`. It would
reuse the planner's reachability rather than invent a second notion.

Cost: the repack must hold the same writer-lock discipline as the collector or be
snapshot-based with a verified switch-over, must never run while a transfer holds
staged objects it has not yet bound to a commit, and needs the same exact
accounting equalities as `gc` (this design would reuse the plan hash).

## 5. Costs and risks, together

| Area | Pack files add |
|---|---|
| Correctness | A second durable store with its own crash states; delta chains and dictionaries are new ways to lose data if a base or dictionary is dropped early |
| Security | Decompression and delta-expansion bombs (bounded sizes and chain depth must be enforced before and during expansion, as for deflate rows today); pack parsing is a new parser for hostile input when packs are transferred |
| Performance | Random reads inflate more data when objects share a stream or chain; compaction/repack are heavy and need I/O budgets |
| Operations | Backup is no longer one SQLite file; free-space accounting for repack (about the size of the live set); a new class of "orphan pack" repair |
| Compatibility | A new local schema; schema-3 clients refuse it; an exact downgrade story (export to canonical packets) must exist |
| Testing | Crash injection at every publish and repack step; mixed-format reads; hostile packs |

## 6. Cheaper steps to try first

The overhead measurement points at something cheaper than packs:

**Narrower keys.** The table above shows that a `WITHOUT ROWID` table with 16-byte
document and 32-byte address blobs cuts the object table's file size by 37 to 41
percent on these fixtures, more than compression did (17 to 21 percent of the whole
file). It needs no new durable artifact: it is a transactional table rebuild
behind a schema number that re-creates the same triggers and keeps accounting and
transfer formats (addresses stay hex text outside SQLite). Canonical lowercase
hex and UUID text sort the same as their raw bytes, so inventory cursors keep their
order. Caveats: the rebuild rewrites the largest table (time and free space about
the file size), and ad hoc SQL inspection gets harder. Test it on real
repositories before committing; it is the cheapest way to attack the dominant
overhead and should come before pack files.

**A shared dictionary without packs.** §4.1's gain comes from a per-repository
dictionary, not from a container; adding a zstd codec with a `dictionary_id` column
needs the C dependency decision and a dictionary lifecycle, but not a pack format.

Other inexpensive options, in order of effort: a documented `compact --vacuum` after
bulk changes (done); a larger SQLite page size for the object table for repositories
dominated by text chunks (measure first); compressing the signed-commit bodies,
which are canonical JSON and currently stored as they are (they are a small share
here, but every commit carries a long hex signature, so the ratio is limited);
and a codec number for zstd on large chunks, whose measured compression speed on
64 KiB text was about five times deflate's at a similar ratio.

## 7. When to build it

Build packs when **measured** evidence, not the synthetic fixtures, shows one of:

1. **Size.** Real repositories whose physical file after `compact --vacuum` and any
   key-layout change is still more than about 2x the stored payload, *and* whose
   absolute size (several GiB, approaching the default 4 GiB limit) makes backup or
   sync time a user-visible problem. Below that, SQLite is adequate.
2. **History depth.** Many revisions of long paragraphs (documents with multi-KiB
   paragraphs edited repeatedly) where §4.2's delta saving exceeds what a second
   codec would give, measured on real documents rather than the padded fixtures.
3. **Object count.** Repositories approaching the default 1,000,000 object limit,
   where per-row overhead and the planner's memory (about 64 bytes per object, 4
   million objects tracked at most) rather than payload are the constraint.
4. **Transport.** A requirement to stream whole history (initial sync, backup
   export) faster than object-by-object batches (64 objects or 1 MiB per batch
   today).

If none of these is observed, do not build it. If (1) or (3) is observed first, do
the key-layout change in §6, which is a transactional table rewrite behind a
schema number; if (2) is observed, add the zstd codec and consider a per-repository
dictionary before a pack file; build pack files only when more than one of the
conditions holds.

## 8. If it is built: sketch

- SQLite stays the **catalog** and the source of truth for quotas, refs, commits
  and reachability. Object rows become `(document, address, location)` where the
  location is either an inline blob (as today) or `(pack_id, offset, length,
  codec, base?)`; inline rows stay valid so migration is incremental.
- Packs are immutable files in a sibling directory named by the SHA-256 of their
  content, written to a temporary name, fsynced, then renamed; a catalog
  transaction that references a pack commits only after the rename is durable.
  Unreferenced packs are ignored and are removable by an explicit repair command.
- Every pack entry is verified on read exactly as an inline row is today: declared
  logical size bound, inflate/delta expansion bounded by it, then the address check.
- Export and import stay canonical, uncompressed packets, so transfer
  compatibility with schema-3 peers is unchanged.
- The retention plan hash and `gc` accounting continue to be about logical
  objects; a repack is a separate, hash-bound command in the style of `gc`.

## 9. Not covered

Server-side (PostgreSQL) object storage, which has different constraints; encryption
at rest of repository contents (the key files are a separate document,
[KEY-FILE-FORMAT.md](KEY-FILE-FORMAT.md)); the commit pruning design, whose local
schema also claims a version number (see the note in LOCAL-STORAGE-BUDGETS.md).
