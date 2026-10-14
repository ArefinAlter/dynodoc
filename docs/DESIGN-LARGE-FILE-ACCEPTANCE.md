# Design: large-file and service-scale acceptance

> **Status: design, not implemented.** 14 October 2026. Every number in this
> document marked *hypothesis* is a target to be falsified by measurement, not a
> measured result or a promise. The pilot upload limit stays **3,000,000 bytes**.
> Part of the [remaining-engine-designs index](ENGINE-REMAINING-DESIGNS.md).
> Item: `E-LARGE`. Implements the acceptance side of part 7 of the
> [evolution plan](ENGINE-EVOLUTION-PLAN.md) and designs the streaming pipeline that
> acceptance needs.

## 0. Summary

The product specification names hundreds-of-MB files, ten collaborators per large
document and hundreds of thousands of registered users as target workloads, and
states that they are "not measured capacity today"
([PRODUCT-SPEC.md](PRODUCT-SPEC.md), "Size and scale objectives"). The plan requires
"streaming/resumable 100/250/500 MB fixtures; per-tenant resource bounds;
storage/transfer amplification; concurrent-workload latency and peak-memory
results; recovery/backup/load-shedding exercise" (part 7).

This document defines **how we will know**:

1. workload models and a deterministic **synthetic corpus** generator;
2. a **measurement methodology** and a fixed **report format**;
3. **hypotheses** with thresholds, falsification criteria and fallbacks;
4. **resource budgets** per request, tenant, document and worker;
5. the **streaming ingest and egress pipeline** and a **resumable upload protocol**
   needed to even attempt the larger sizes (today ingest happens in the browser and
   uploads are base64 JSON in one request body);
6. an **analysis of chunk boundaries**: the current fixed 64 KiB tree versus
   content-defined chunking (FastCDC), including how insertions shift boundaries and
   what that costs, with a decision rule driven by an experiment;
7. a **limit ladder** (3 MB to 500 MB) in which each rung is gated on evidence.

What this document is not: a claim that any size works today. The linear
extrapolations in section 2.2 predict that current full-state publication fails
well below 100 MB of state; that prediction is the motivation, not a result.

### 0.1 Order and dependencies

| Needs first | Why |
|---|---|
| Changed-node publication and windowed reads (evolution plan part 2 remainder) | Without them, steady-state edit cost is O(document), so H5-H8 cannot pass. This design lists them as phase L6 but they are largely owned by the storage work. |
| Optional: [installed-host acceptance](DESIGN-INSTALLED-HOST-ACCEPTANCE.md) | Native-file fidelity for large documents is only meaningful on real hosts; egress acceptance cross-references it. |
| Independent of the merge and witness designs | Merge cost on large documents is a consumer of this work ([cross-document merge](DESIGN-CROSS-DOCUMENT-MERGE.md) lists memory as an assumption). |

## 1. Purpose, goals, non-goals, personas, metrics

### 1.1 Problem statement

- The 3,000,000-byte cap is a *tester cap*, not evidence of bounded memory for 100 MB
  documents ([decision 013](decisions/013-projects-first-test-workspace.md)).
- Raising it without bounded ingest and resource controls would trade a polite
  error message for an outage (decision 012: "not grounds for increasing upload
  limits without bounded ingest, resource controls and measured workloads").
- Every existing benchmark is synthetic and small ([ENGINE-EVOLUTION-HANDOFF.md](ENGINE-EVOLUTION-HANDOFF.md),
  `docs/benchmarks/`). None involves a native file, HTTP, concurrent users or a
  production-like host.

### 1.2 Goals

| ID | Goal |
|---|---|
| G1 | Produce a repeatable, versioned acceptance suite that anyone can run and compare. |
| G2 | State pass/fail thresholds before measuring, so results cannot be rationalised afterwards. |
| G3 | Identify the first bottleneck at each size class and say which component owns it. |
| G4 | Define the pipeline and protocol changes required by streaming. |
| G5 | Decide fixed versus content-defined chunking with evidence, not preference. |
| G6 | Gate every increase of the upload limit on a passing report. |

### 1.3 Non-goals

- Certifying capacity for 100,000+ users. The plan says "no current capacity
  promise" and this design keeps that: it measures *active* workloads.
- Optimising the browser editor for 500 MB documents (the web editors are not the
  product roadmap).
- Native-host fidelity of huge files (installed-host acceptance owns it).
- Replacing PostgreSQL or choosing an object store product here; the plan requires
  "a measured spike" for the production backend and this design supplies the harness.

### 1.4 Personas

| Persona | Need |
|---|---|
| Research lead | Uploads a 180 MB report with embedded figures and expects it to work. |
| Analyst | A workbook with 2,000,000 rows. |
| Platform engineer | Needs to know what to provision and which limit to set. |
| Operator | Needs load shedding that preserves small-document service. |
| Sales/PM | Needs honest, dated numbers to state externally. |

### 1.5 Success metrics

A rung of the limit ladder is *accepted* only if all **gate** hypotheses (section 4)
pass on the reference environment with the recorded report, and a second operator
reproduces the result within 15 % on the same fixtures.

## 2. Current state against target

### 2.1 Capability table

| Capability | Tag | Evidence / path |
|---|---|---|
| Original upload cap 3,000,000 bytes (+ base64 envelope) | EXISTS | `crates/engine-api/src/workspace.rs` (`MAX_UPLOAD_BYTES`, `MAX_UPLOAD_BASE64`); `frontend/researcher/src/lib/workspace/file-limits.ts` |
| Whole-file upload in one base64 JSON body, stored as `bytea` | EXISTS | `workspace.rs` `upload`; table `document_upload` |
| Browser-side OOXML parse with bounded zip (10,000 entries; 32 MiB per entry; 64 MiB expanded) | EXISTS | `frontend/researcher/src/lib/workspace/bounded-zip.ts`, `docx-import.worker.ts` |
| Batch limits: 5,000 changes, 20,000 initial import nodes | EXISTS | `workspace.rs` `batch` |
| Streaming byte-content tree (64 KiB chunks, fan-out 128, height up to 4) with bounded readers | EXISTS | `crates/engine-core/src/shared_checkpoint/content.rs`; [CHUNKED-CONTENT.md](CHUNKED-CONTENT.md) |
| Streaming probe at 100/250/500 MiB (generated repeated bytes, in-memory store) | EXISTS | `docs/benchmarks/content-stream-20261012.json`; example `content_stream_bench` |
| Typed checkpoints reference content roots; asset manifests | EXISTS | `crates/engine-core/src/shared_checkpoint/typed.rs` (`write_asset`, `read_asset`) |
| Fixed chunk boundaries (an insertion shifts later chunks) | EXISTS (documented limitation) | CHUNKED-CONTENT.md "Bounds, sharing and publication" |
| Per-account/document shared-object budgets | EXISTS | `migrations/0034_shared_storage_budgets.sql` |
| Process admission: 64 body responses, 256 streams, 8 expensive reads, per-identity and per-project windows | EXISTS | `crates/engine-api/src/workloads.rs`; [BROWSER-AND-WORKLOAD-SECURITY.md](BROWSER-AND-WORKLOAD-SECURITY.md) |
| Full-state publication, full-state reads, resident `DocumentState` | EXISTS (limitation) | [SHARED-CHECKPOINTS.md](SHARED-CHECKPOINTS.md); benchmarks `shared-checkpoint-pg-*.json` |
| Changed-node (incremental) publication | MISSING | listed as open in [ROADMAP-ACCEPTANCE.md](ROADMAP-ACCEPTANCE.md) |
| Windowed/ranged reads of a large document | MISSING | none |
| Resumable chunked upload of an original native file | MISSING | private transfers cover typed objects (64 objects / 1 MiB per batch), not a native binary |
| Server-side streaming ingest worker | MISSING | none |
| Streaming native export | MISSING | exports are client-side builders (`docx-export.ts`, `xlsx-export.ts`, `pptx-export.ts`) |
| Content-defined chunking | MISSING | none |
| Corpus generator, harness, report format | MISSING | `docs/benchmarks/*` are ad hoc |
| Per-tenant ingest concurrency limits / queue / cancellation | MISSING | process-local admission only |
| Capacity and cost evidence for 100/250/500 MB, ten collaborators | MISSING | [ROADMAP-ACCEPTANCE.md](ROADMAP-ACCEPTANCE.md) "Service scale" |

### 2.2 What existing numbers imply (linear extrapolation, not a result)

Anchors, all from `docs/benchmarks/` on a Windows i7-1255U, release build, local
PostgreSQL 16, single warm sequential runs:

| Quantity | Anchor | Source |
|---|---|---|
| State size | 8,000 blocks, 6.4 MB JSON | `shared-checkpoint-pg-part2c-batched-varied-20261010.json` |
| Shared publication + verification | about 2.7 - 2.9 s per publication, dominated by full scan | same, `shared_write_verify_ms` |
| Batched shared read | 0.8 - 1.4 s (legacy JSON read 0.12 - 0.21 s) | same |
| Writer wait behind publication | 2.1 - 2.9 s append lock wait | same, `append_lock_wait_ms` |
| Peak working set of the benchmark process | about 110 MB (about 17 times the JSON size) | same, `observed_peak_working_set_bytes` |
| Content tree streaming | 100/250/500 MiB write 163/421/850 ms, read 79/171/353 ms, working set about 5.5 - 6 MB | `content-stream-20261012.json` |
| Local SQLite state write, debug build, with process start | 128 paragraphs 280 ms; 512 paragraphs 1,168 ms median | `local-storage-growth-*.json` |

Linear extrapolation of publication cost (about 0.44 s per MB of state JSON) and
working set (about 17 times state size):

| State JSON size | Publication + verify | Working set |
|---:|---:|---:|
| 6.4 MB (measured) | about 2.8 s | about 110 MB |
| 25 MB | about 11 s | about 430 MB |
| 68 MB | about 30 s (equals the documented API request deadline) | about 1.2 GB |
| 250 MB | about 110 s | about 4.3 GB |
| 500 MB | about 220 s | about 8.5 GB |

These show the direction only: the streaming byte tree is not the bottleneck
(hundreds of MB/s, constant memory); full-state typed publication is. They are the
reason phase L6 (changed-node publication) is a prerequisite for H5-H8.

## 3. Workload models

Parameters are explicit so a report can say exactly what was run. All workloads use
synthetic content from the generator (section 5).

| ID | Name | Description | Parameters |
|---|---|---|---|
| W1 | Author session | One user edits one document | size class S; 30 batches/min of 1-20 node edits; 60 min; 70 % text, 20 % format, 10 % structural |
| W2 | Collaborative | Ten users on one document | 6 batches/min each; 10 % of edits overlap another user's node within 60 s; 5 % structural; SSE subscribers 10 |
| W3 | Review cycle | Draft branches compared and merged | 3 branches per week; 1 % and 10 % node diffs; one conflict per 50 nodes changed |
| W4 | Initial import | First ingest of a native file | size class S; cold cache |
| W5 | Re-import | Edited native file pushed | 1 % and 10 % of nodes changed; 0.1 % structural |
| W6 | Export | Native download of current or old revision | whole file; random historical revision |
| W7 | Many teams | Independent teams, small and medium documents | 200 teams x 1 document of 25 MB x 3 active users; 5 further teams with 250 MB documents; Poisson edit arrivals |
| W8 | Reconnect storm | All SSE clients reconnect after a deploy | 10 users per document x 50 hot documents within 10 s |
| W9 | Noisy neighbour | Large ingest during W7 background | one 500 MB W4 concurrent with W7 |
| W10 | Long history | Many revisions, random old read | 10,000 revisions; 100 MB class; reads at random revisions |

### 3.1 Size classes

Compressed native size; expanded and structural sizes are reported separately
(product spec requirement).

| Class | Compressed | Typical composition |
|---|---:|---|
| S0 | 3 MB | the current pilot cap (control) |
| S1 | 25 MB | |
| S2 | 100 MB | |
| S3 | 250 MB | |
| S4 | 500 MB | |

For each class three compositions:

| Composition | Media share | Text/XML share | Example shape |
|---|---:|---:|---|
| M (media heavy) | 95 % | 5 % | report with 2,000 images; 5,000 paragraphs |
| B (balanced) | 50 % | 50 % | 100,000 paragraphs, 500 images |
| T (text/data heavy) | 5 % | 95 % | long document or workbook with 2,000,000 rows x 10 columns |

## 4. Hypotheses and gates

*Every threshold below is a hypothesis.* "Gate" means a rung of the limit ladder
cannot be accepted unless it passes; "Report" means it must be measured and
published whatever the value. The reference environment is defined in 6.1.

| ID | Hypothesis | Threshold (hypothesis) | Class | If falsified |
|---|---|---|---|---|
| H1 | Ingest memory is flat in input size | worker peak RSS at S4 at most 768 MiB and RSS(S4)/RSS(S2) at most 1.25 | Gate | find the resident structure (index, batch, DOM); bound or externalize |
| H2 | Ingest throughput | M: at least 40 MB/s compressed input; T: at least 6 MB/s compressed input (single worker) | Report | profile; parallelize parts; reconsider parser |
| H3 | Ingest wall time | S4/M at most 5 min; S2/T at most 10 min on R1 | Gate | per-part parallelism; larger instance class |
| H4 | Resumability | kill at 20 random points: at most 16 MiB retransmitted; ingest repeats at most one batch | Gate | smaller commit granularity; idempotent batches |
| H5 | Edit cost independent of document size | for m <= 100 changed nodes: server CPU <= 50 ms + 0.5 ms x m; bytes written <= 4x encoded changed bytes + 16 KiB per index level; cost ratio between N = 50,000 and N = 2,000,000 nodes <= 1.5 | Gate | changed-node publication not working; find the full scan |
| H6 | Concurrent writers on one large document | W2 on S3/B: write p95 <= 1.5 s, p99 <= 5 s, error rate <= 0.1 % excluding intended conflicts, zero statement timeouts | Gate | lock scope; batching; per-document queue |
| H7 | Windowed reads | first 200 blocks of S3/B: p95 <= 0.8 s warm, <= 3 s cold; per-request memory <= 64 MiB | Gate | range index; cache |
| H8 | Historical reads | read 200 blocks at random revision in W10: p95 <= 2 s | Report | checkpoint cadence; range checkpoints |
| H9 | Egress streaming | first byte <= 2 s; at least 25 MB/s on loopback; RSS <= 256 MiB; produced file opens in real hosts (cross-link to installed-host acceptance) | Gate | zip data-descriptor mode; part ordering |
| H10 | Storage growth | after 100 revisions each changing 1 % of nodes (text): stored shared object bytes <= 1.2 x (initial + sum of changed encoded bytes), original binary excluded | Gate | packing; object granularity |
| H11 | Transfer amplification | pushing a 1 % change transfers <= 3 % of the initial transfer bytes; negotiation round trips <= 10 + changed objects / 64 | Report | negotiation protocol |
| H12 | WAL amplification | WAL bytes per publication <= 3 x changed object bytes | Report | batching; index churn |
| H13 | Noisy neighbour | W9: p95 write latency of small documents <= 2 x its W7 baseline; excess work answered with 429 + Retry-After, not timeouts | Gate | per-tenant queues; ingest concurrency cap |
| H14 | Many independent teams | W7 on R1: p95 write <= 400 ms; CPU <= 70 %; no growth of RSS over 4 h | Gate | pooling; hot-path allocation |
| H15 | Chunking decision | per-part fixed 64 KiB chunking stores <= 1.2 x per-part FastCDC on the version corpus; whole-file fixed >= 3 x (expected to confirm the per-part design) | Report -> decision | adopt CDC for parts above threshold |
| H16 | Recovery | restore from backup + verify: report RTO/RPO; no gate yet | Report | |
| H17 | Cost | cost per active user-hour on the reference infrastructure, with the formula | Report | |

## 5. Test corpus synthesis

### 5.1 Principles

- **Synthetic only.** No customer or personal content. Customer-provided documents
  are used only with written consent, never committed, and reported by hash.
- **Deterministic.** Seeded; output byte-identical across runs and platforms;
  fixtures are named by `generator version + seed + parameters` and their SHA-256 is
  recorded in the report.
- **Structurally realistic.** Real OOXML packages that real hosts open. The
  generator is validated by independent parsers and by opening samples in installed
  hosts at the small classes.
- **Controlled knobs.** Every property that affects cost is a parameter.

### 5.2 Generator architecture

```
 parameters (YAML) --> [plan] --> part generators --> [OPC packager (streaming zip)] --> file
                         |           |  document.xml  (paragraph/run/table/list distributions)
                         |           |  sheetN.xml    (rows, columns, formulas, shared strings)
                         |           |  slideN.xml    (shapes, text, images)
                         |           |  media         (PRNG bytes: incompressible; or
                         |           |                 procedural image: compressible)
                         |           |  styles, theme, rels, content types
                         +--> manifest.json: node counts, bytes per part, expansion ratio, SHA-256
```

| Knob | Values |
|---|---|
| Composition | M / B / T (section 3.1) and exact media bytes |
| Paragraphs / rows / slides | counts; run density; run length distribution (geometric, mean 12 words) |
| Formatting | marks per run probability; styles count; tracked changes ratio (Word); comments ratio |
| Structure | table density; list nesting depth; sections |
| Text | Latin, Bengali (conjuncts), mixed RTL, emoji ZWJ sequences, long unbroken strings |
| Spreadsheet | rows, columns, formula share, shared-string share, date-only cells, merged ranges |
| Presentation | slides, shapes per slide, image bytes per slide |
| Zip | deflate level, stored media, expansion ratio cap, entry count, data descriptors on/off |
| Edit scripts | generated separately: seeded sequence of operations over the manifest's node index (replace text, insert, delete, move, format, replace media) |

### 5.3 Version corpus (for chunking and storage experiments)

Starting from each fixture, generate 50 successive native versions by applying edit
scripts *and re-serializing through the generator's "host mimic" mode*, which
reproduces the properties that matter for dedup: parts re-ordered or re-compressed,
XML re-serialized with different whitespace, media bytes identical or re-encoded.
A second set uses real Word/Excel saves of small fixtures to calibrate the mimic;
this is the only place installed hosts are needed.

### 5.4 Adversarial fixtures

Zip bomb (nested and flat ratios 1,000:1 and above), 100,000 tiny entries, path
traversal names, duplicate entries, XML entity expansion, deeply nested elements
(depth above 128), extreme attribute counts, 50 MB single paragraph, 10^7
single-character runs, invalid UTF-8, macro-bearing parts. Expected: explicit
bounded rejection with no memory growth.

### 5.5 Generator validation

| Check | Method |
|---|---|
| Package validity | `unzip -t`; OPC part and relationship checks; XSD validation of main parts (schema availability: unverified, check at build time) |
| Opens in host | small classes opened in Word, Excel, PowerPoint (owner-run, part of installed-host acceptance) |
| Statistical realism | compare run-length and style distributions with a handful of real documents (summary statistics only; documents not retained) |
| Determinism | double generation, byte compare |

## 6. Measurement methodology

### 6.1 Environments

| ID | Definition |
|---|---|
| R0 | developer laptop (the existing benchmark host class); for regression trends only |
| R1 | reference service host: 4 vCPU, 16 GB RAM, NVMe SSD, PostgreSQL 16 on the same host, API and worker as separate processes with cgroup limits; exact provider and plan recorded in each report |
| R2 | R1 with 8 vCPU / 32 GB, for scale-up evidence |
| R3 | multi-instance: two API replicas, one worker pool, PostgreSQL on a separate host (when part 7 service design exists) |

The production VPS specification is not recorded in the repository documents; R1 is
chosen to be at most the production class once it is recorded. The report states
the ratio to production.

### 6.2 Run protocol

1. Provision clean environment from an image; record kernel, PostgreSQL settings,
   engine commit, schema, build profile, feature flags.
2. Generate fixtures (cached by hash).
3. Warm-up run discarded; at least 7 measured repetitions per scenario (3 for
   S4 due to time), interleaved to resist drift.
4. Cold cache runs: restart PostgreSQL and drop OS cache (where permitted) before.
5. Workloads W2, W7, W8, W9 use an open-loop load generator (arrival schedule fixed
   in advance) to avoid coordinated omission; closed-loop only for W1.
6. Record raw per-request samples; compute percentiles from raw data.
7. Always publish failures and aborted runs.

### 6.3 Metrics

| Group | Metrics | Collection |
|---|---|---|
| Latency | p50/p95/p99/max per endpoint and per operation; time to first byte | load generator timestamps; server spans |
| Throughput | bytes/s in and out; nodes/s; requests/s | counters |
| Memory | peak RSS per process (API, worker, PostgreSQL), allocator stats where available | cgroup `memory.peak` or equivalent; OS peak working set on Windows; sampling interval documented |
| CPU | CPU seconds per operation; utilization | cgroup; `pidstat`/OS counters |
| Storage | logical object bytes; `pg_total_relation_size` per table; WAL bytes; temp files | PostgreSQL statistics |
| Transfer | bytes and round trips per push/pull | client instrumentation |
| Contention | lock wait time; statement timeouts; 429/503 rates | PostgreSQL logs, `pg_stat_activity` samples |
| Queue | ingest queue depth, wait time | worker metrics |
| Correctness | roundtrip equality: ingest then export then re-parse equals original semantic content; history verification passes | acceptance checks run on every performance run |

### 6.4 Statistical treatment

Report median and p95 over repetitions with min/max; for latency distributions
report the full histogram; no averaging across scenarios; a result is *stable* if
the coefficient of variation across repetitions is below 10 %, else flagged. A
regression is a change above 15 % on any gate metric between two reports on the
same environment and fixtures.

### 6.5 Instrumentation changes required

| Item | Detail |
|---|---|
| Server spans | per-request timing, DB wait, lock wait, bytes in/out, object counts |
| Worker metrics | bytes parsed, nodes emitted, batch commit durations, resident index size |
| Memory | explicit accounting of resident indexes (the engine already counts object visits and bytes against `Limits`) |
| Profiling hooks | feature-gated flamegraph capture for a failing gate run |

## 7. Resource budgets (proposed)

Budgets are enforced limits, not just targets. All values are hypotheses to be
tuned by section 4 evidence.

| Scope | Budget | Enforcement |
|---|---|---|
| Upload session | at most 2 concurrent per account; 500 MB per session; 24 h lifetime; at most 20 sessions per project | session table, quota reservation |
| Ingest worker | memory 768 MiB hard (cgroup), 1 file at a time per worker; per-file wall deadline 30 min | cgroup, scheduler |
| Ingest queue | at most 4 queued per tenant; global cap proportional to workers; overflow returns 429 with `Retry-After` | queue |
| Parser | zip ratio <= 100:1; <= 100,000 entries; part <= 2 GiB expanded; XML depth <= 128; entities rejected; scalar run caps as today | parser |
| Per-request API | body <= 16 MiB (existing outer cap); resumable chunks <= 16 MiB; response windows <= 1,000 blocks or 8 MiB | handlers |
| Document | encoded shared bytes <= 1 GiB and 250,000 objects (existing, raise by evidence) | migration 0034 budgets |
| Tenant | concurrent expensive reads 2 per identity / 4 per project (existing) | `workloads.rs` |
| Database | statement timeout 15 s API default; transfer transactions 10 s / lock 3 s (as documented); ingest batches target <= 1 s each | session settings |
| Egress | at most 2 concurrent exports per account; 25 MB/s per stream soft cap | admission |

## 8. Streaming ingest and egress design

### 8.1 Where ingest happens

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Browser worker (today) | no server CPU; no upload of raw file needed for parse | browser memory (file + fflate buffers + DOM); 100+ MB unreliable; cannot resume; closes with tab | keep for small classes (<= S0/S1) |
| Server ingest worker | bounded resources under our control; resumable; shared by CLI and web | server CPU cost; needs staging store; parser hardening | **primary for >= S1** |
| Local engine (CLI, desktop agent) | no upload of raw bytes needed for parse; full machine resources | not available in browser; install base | supported for CLI users |

### 8.2 Pipeline

```
 client                          API                           staging              worker             store
   | POST /uploads {size,sha256?}  |                               |                    |                  |
   |------------------------------>| reserve quota, create session |                    |                  |
   | PATCH offset, 4-16 MiB chunks |                               |                    |                  |
   |------------------------------>| append, incremental SHA-256   |--spool/blob------->|                  |
   | HEAD -> Upload-Offset (resume)|                               |                    |                  |
   | POST /uploads/{id}/complete   | verify total hash, enqueue    |                    |                  |
   |------------------------------>|------------------------------>| job {id,doc,mode}  |                  |
   |                               |                               |      zip central dir (range reads)    |
   |                               |                               |      per part: SAX stream -> nodes    |
   |                               |                               |      assets -> content trees (64 KiB) |
   |                               |                               |      batch of N nodes --------------->| objects + changed-node
   |                               |                               |                    |    checkpoint    | publication
   | GET /uploads/{id}/status      |<------------------------------|  progress, errors, resumable cursor   |
```

Notes:

- The resumable protocol follows the semantics of tus 1.0.0 (creation, HEAD offset,
  PATCH at offset with 409 on mismatch, optional checksum and termination
  extensions); Dynodoc may adopt the tus wire format or define an equivalent with
  its own authentication and quota headers (decision L-Q2).
- The final native file is stored as an asset content tree (64 KiB chunks)
  referenced by a typed asset manifest (`write_asset`); it is not placed in a
  `bytea` row.
- The worker commits the graph in batches; each batch is idempotent and records a
  cursor so a crash resumes at the next batch.
- Re-import (W5) reconciles with the existing graph using the identity strategy of
  the evolution plan (embedded identity, then verified ancestry, then similarity as
  proposal only). The matching index is the one resident structure; it is stored in
  PostgreSQL (key: content hash and position) and joined in a streaming merge to
  keep RSS flat (H1).

### 8.3 Egress

| Step | Design |
|---|---|
| Plan | resolve the revision to a typed state root and an asset list |
| Stream | iterate nodes in document order with bounded windows; stream zip entries with data descriptors (general-purpose flag bit 3) so CRC and sizes follow the data |
| Assets | read content trees range by range and write directly into the zip entry |
| Backpressure | write to the response socket with bounded buffers; abort on client disconnect |
| Resume | `Range` requests are served only for stored (non-regenerated) originals; regenerated exports are not resumable in v1 |
| Fidelity | export correctness is a host acceptance question; data-descriptor zips are valid OPC but host acceptance must confirm (unverified for all six hosts) |

### 8.4 Limit ladder

| Rung | Original-file limit | Path | Gate |
|---|---|---|---|
| L0 | 3,000,000 bytes (today) | legacy JSON path | none |
| L1 | 10 MB | legacy path or new path | H1-H4 at S0/S1 (control) |
| L2 | 25 MB | new resumable path | all gates at S1 |
| L3 | 100 MB | new path, server ingest | all gates at S2 |
| L4 | 250 MB | per-tenant opt-in | all gates at S3 |
| L5 | 500 MB | per-tenant opt-in | all gates at S4 and H13 at R1 and R2 |

Configuration (proposed): `DYNODOC_MAX_ORIGINAL_BYTES` (global),
`tenant.max_original_bytes` (override), both bounded by the highest accepted rung.
The 3,000,000 constant remains as the fallback for the legacy path
(`workspace.rs`, `file-limits.ts` must be updated together; a CI check compares the
Rust and TypeScript constants).

## 9. Chunk boundaries: fixed versus content-defined

### 9.1 What is chunked today

Content v1 ([CHUNKED-CONTENT.md](CHUNKED-CONTENT.md)): 64 KiB chunks, every child
except the last full to its subtree capacity, fan-out 128, height up to 4
(`CHUNK_BYTES`, `FANOUT`, `MAX_HEIGHT` in `content.rs`). Capacity is
`64 KiB x 128^4` = 16 TiB. The document explicitly says an insertion or deletion can
change all later chunks and that this is not a claim of content-defined chunking.

### 9.2 Insertion shift analysis

Let N be stream length, c the chunk size, p the edit offset, k the inserted bytes,
and `k mod c != 0`.

| Edit | Fixed-size chunks | CDC (average chunk c-bar) |
|---|---|---|
| Overwrite of k bytes in place | `ceil(k/c) + 1` chunks change; optimal | same, plus up to 2 neighbours when cut points move |
| Insert or delete k bytes at p | **all** chunks from p onward change: `ceil((N - p)/c)` chunks, about `N/2` bytes for uniform p | about `ceil(k/c-bar) + 2` chunks; boundaries resynchronize after the edit |
| Append | 1 chunk (+ tail) | 1 chunk (+ tail) |

Index overhead for fixed chunks: a branch node holds up to 128 references of 40
bytes (21-byte header, up to 5,141 bytes). For N = 500 MB and c = 64 KiB there are
about 7,800 chunks and 62 level-1 branch objects. An insertion rewrites those 62
objects (about 320 KB, 0.06 % of N) in addition to the shifted data chunks; the
index is not the problem, the data chunks are. With CDC and positional packing of
variable-length children the same 62 branch objects shift (one extra reference
moves the following children); keeping index nodes content-defined as well (a
"prolly tree" style, boundaries chosen by a hash of child addresses) removes even
that, but is only warranted above roughly 50 GB per stream.

### 9.3 Where insertion shifts matter for Dynodoc

| Stream | Fixed chunking verdict | Reason |
|---|---|---|
| Node text values (typed text references) | fixed is fine | values are small; large single strings are rare; edits are mostly replacement of whole values |
| Embedded media (opaque assets) | fixed is fine | an edited image is a new asset; a re-saved identical image is byte-identical |
| The original native package re-saved by a host | **fixed is poor if the whole package is chunked** | Word/Excel/PowerPoint rewrite every XML part; sizes change; media parts shift offsets inside the zip although their bytes are unchanged |
| Large single parts (a 300 MB `sharedStrings.xml` or `sheet1.xml` kept as an opaque part) | CDC helps | XML re-serialization inserts and deletes bytes |

**Design decision (proposed, subject to H15): package-aware chunking.** Treat each
OPC part as its own content stream (one content tree per part) and key assets by
part. Unchanged media parts then deduplicate exactly regardless of their offset in
the zip, and no CDC is needed for them. CDC is applied only to streams larger than
a threshold (default 8 MiB) whose bytes are not typically replaced wholesale
(opaque XML parts, large CSV-like payloads).

### 9.4 FastCDC parameters and properties

FastCDC (Xia et al., USENIX ATC 2016) builds on Gear-hash CDC with three changes:
a simplified, stronger hash judgment, skipping cut-point checks below the minimum
size, and normalized chunking to keep sizes in a narrow band; the authors report
about 10 times faster than the best open-source Rabin-based CDC and about 3 times
faster than Gear- and AE-based CDC at nearly the same deduplication ratio.

Proposed profile `fastcdc-16k-64k-256k`:

| Parameter | Value |
|---|---|
| Minimum / average / maximum | 16 KiB / 64 KiB / 256 KiB (the maximum fits the 1 MiB object bound with headroom) |
| Normalization level | 2 (stricter mask below average, looser above) |
| Gear table | 256 x u64 derived from a per-document secret seed |
| Hash of chunk | SHA-256 under a new domain `dynodoc.content\0v2\0` |

**Per-document secret seed.** The gear table is derived from a random value stored
with the document, not from a global constant, to resist chunk-boundary
fingerprinting and attacker-chosen worst-case inputs. restic does the same with a
random polynomial stored in its repository config (documentation read 14 October
2026), and its CDC uses 512 KiB minimum, 1 MiB average and 8 MiB maximum with Rabin
fingerprints. Because objects are already scoped per document, cross-document
deduplication is not lost.

### 9.5 Format consequences

| Item | Consequence |
|---|---|
| v1 invariants | v1 requires every child but the last to fill its subtree capacity; CDC chunks have variable length. A **content v2** object kind set is needed: variable-length chunk, branch (children with lengths), root; own magic version byte and hash domain |
| Compatibility | v1 readers reject v2 objects (existing rule for new content kinds). The typed reference gets a `chunker` field: `fixed-64k` or a CDC profile id; readers pick the codec |
| Determinism | CDC output depends on parameters and seed; independent vector fixture required (as for v1) |
| Verification | reading verifies each chunk hash and the length arithmetic as today; seed is not needed to *read*, only to *write* a new version with maximal reuse |
| Transfer | negotiation unchanged: missing objects by address |

### 9.6 Experiment (H15)

| Item | Detail |
|---|---|
| Inputs | version corpus (5.3): 3 compositions x 50 versions, plus real small-file save sequences |
| Variants | whole-file fixed 64 KiB; whole-file FastCDC; per-part fixed 64 KiB; per-part FastCDC; per-part CDC above 8 MiB only |
| Outputs | unique stored bytes per version and cumulative; chunk count; index bytes; chunking CPU seconds per GB; write/read throughput |
| Decision rule | adopt CDC for large parts if per-part fixed stores more than 1.2 times per-part CDC on corpus streams that carry at least 20 % of stored bytes; otherwise keep v1 and package-aware parts |
| Not measured | real-user save behaviour variance; compression of chunks (the design stores uncompressed content objects) |

## 10. Pass/fail criteria and report format

### 10.1 Verdict rules

| Verdict | Meaning |
|---|---|
| PASS | every gate hypothesis for the rung meets its threshold in at least 6 of 7 repetitions (2 of 3 for S4) and the second-operator reproduction is within 15 % |
| FAIL | any gate hypothesis misses its threshold in a majority of repetitions |
| INCONCLUSIVE | instability (CV above 10 %), environment deviation, or incomplete run |
| NOT RUN | explicitly listed |

A rung's acceptance is invalidated by: a change to engine commit, schema, parser,
chunker or limits; a change of environment class; a regression above 15 % on any
gate metric. "Passed" never transfers between rungs or compositions.

### 10.2 Report: machine-readable (proposed `dynodoc.large-file-report.v1`)

```json
{
  "version": 1,
  "date": "2026-11-02T10:12:00Z",
  "engine": { "commit": "abc1234", "schema": 41, "profile": "release", "flags": ["streaming_ingest"] },
  "environment": { "id": "R1", "cpu": "4 vCPU", "ram_gb": 16, "disk": "NVMe", "postgres": "16.x",
                   "provider": "recorded", "production_ratio": 1.0 },
  "fixtures": [ { "id": "docx-M-S3-seed7", "sha256": "…", "compressed_bytes": 262144000,
                  "expanded_bytes": 410000000, "nodes": 41200, "media_bytes": 248000000 } ],
  "scenarios": [
    { "id": "W4/docx-M-S3", "repetitions": 7,
      "metrics": { "wall_s": { "median": 142.1, "p95": 150.7 },
                   "rss_mib": { "worker_peak": 512, "api_peak": 180 },
                   "bytes": { "uploaded": 262144000, "stored_logical": 263100000 } },
      "hypotheses": [ { "id": "H1", "threshold": "<=768 MiB", "value": 512, "verdict": "PASS" } ] }
  ],
  "deviations": [],
  "not_measured": ["native host fidelity", "multi-region", "backup restore"],
  "raw": "raw/*.jsonl"
}
```

### 10.3 Report: human-readable template

1. Headline: rung, verdict, one-sentence summary of the first bottleneck.
2. Environment and engine identification (copy of the JSON header).
3. Hypothesis table with measured values.
4. Graphs: latency histograms, RSS over time per scenario, throughput.
5. Findings: component owning each miss, suggested change.
6. Limitations: the `not_measured` list verbatim, plus the rule that the report is
   not a capacity promise.
7. Reproduction commands and fixture hashes.

## 11. Security and privacy

### 11.1 Threat model

| # | Threat | Control | Residual |
|---|---|---|---|
| T1 | Zip bomb, nested archives, entry flood | ratio and count caps; streaming expansion counting actual output (as `bounded-zip.ts` does client-side); reject nested archives by default | parser bugs |
| T2 | XML bombs, entities, deep nesting | reject DOCTYPE/ENTITY (as `xmlText` does), depth cap 128, attribute and text node caps | |
| T3 | Path traversal and absolute names in zip entries | normalize and reject; never write parts to disk by name | |
| T4 | Resource exhaustion by many sessions or large uploads | per-account sessions, quota reservation at creation, TTL, admission | fleet-wide fairness not modelled |
| T5 | Cost abuse (spin up ingests) | per-tenant concurrency cap; billing hooks later | |
| T6 | Cross-tenant leakage via dedup | content objects stay document-scoped (existing); no cross-tenant existence endpoint | |
| T7 | Malicious macros and embedded objects | preserved as opaque assets, never executed; macro-enabled files rejected as today | |
| T8 | Poisoned CDC boundaries causing degenerate chunking | per-document secret seed; min/max bounds | seed disclosure |
| T9 | Sensitive content in benchmarks and logs | synthetic corpus; logs carry sizes and hashes only | |

### 11.2 Privacy

Customer documents used for calibration need written consent, are never committed
or uploaded to third-party services, and are reported by SHA-256 only. Server logs
for ingest must exclude content and paths inside archives beyond counts.

## 12. Operations

| Topic | Design |
|---|---|
| Deployment | ingest worker as a separate process with cgroup limits; staging on local NVMe with an object-store option later (chosen by the storage spike) |
| Capacity planning | derive from H2/H3: workers = peak concurrent ingests / 1; PostgreSQL IOPS from H12; staging disk = concurrent sessions x size |
| Load shedding | order: refuse new ingests (429), pause low-priority exports, then expensive reads; small-document writes last |
| Observability | queue depth/age, per-tenant concurrency, worker RSS, batch durations, resumable-session counts, ratio of aborted uploads |
| Upgrade/rollback | new endpoints are additive; the limit ladder is configuration; rollback = set the limit back to the previous rung; staged sessions expire |
| Failure modes | worker crash (resume at cursor), staging disk full (reject new sessions, drain), PostgreSQL slow (backpressure through batch size), client disconnect (session resumable until TTL), corrupted chunk (hash mismatch rejects the PATCH) |

## 13. Delivery plan

### 13.1 Phases

| Phase | Deliverable | Depends on | Effort (person-weeks) | Confidence |
|---|---|---|---|---|
| L0 | Methodology doc to code: report schema, harness skeleton, load generator (open loop), metrics collection | none | 2 - 3 | high |
| L1 | Corpus generator (OOXML parts, edit scripts, host-mimic mode, adversarial set), determinism tests | L0 | 3 - 4 | medium |
| L2 | **Baseline measurement of the current system** at S0 to S2 for W1, W2, W4 (legacy path) and publication cost slope; first report | L0, L1 | 1.5 - 2 | high |
| L3 | Chunking experiment (H15) and decision | L1 | 2 - 3 | medium |
| L4 | Resumable upload service, staging, quotas, session lifecycle | L0 | 4 - 5 | medium |
| L5 | Server ingest worker: streaming zip + SAX for OOXML Word first, then Excel, PowerPoint; matching index; batch commit with cursors | L4 | 9 - 13 | low |
| L6 | Changed-node publication and windowed reads (shared with the storage roadmap) | L2 | 7 - 11 | low |
| L7 | Streaming egress | L6 | 3 - 4 | medium |
| L8 | Service-scale runs (W2, W7, W8, W9) on R1/R2; fix cycle | L5, L6 | 3 - 4 (excluding fixes) | medium |
| L9 | Limit ladder, configuration, runbooks, dashboards | L8 | 2 - 3 | medium |
| Total | | | 37 - 52 | |

**Assumptions**: one platform engineer plus part-time reviewer; engine storage
contracts (typed checkpoints, content v1) remain; no new database vendor. L6 is
shared with, and may be accounted for in, the evolution plan's storage work; the
range shows the full cost if it is counted here. **Excludes**: native-host
fidelity of huge files (installed-host acceptance), browser editor performance,
multi-region replication, procurement and operation of R2/R3 hardware, legal and
commercial pricing, GPU or ML components, content compression design, fixes found
by L8 beyond the first fix cycle.

### 13.2 Acceptance by phase

| Phase | Acceptance |
|---|---|
| L0-L1 | two runs of the generator are byte-identical; a harness dry run produces a schema-valid report |
| L2 | a published baseline report with all `not_measured` listed; slope of publication cost vs state size fitted |
| L3 | decision recorded with data; vectors for the chosen chunker |
| L4 | kill/resume matrix passes (H4 upload part); quota reservation tests |
| L5 | H1-H4 pass at S1/S2; adversarial set rejected without memory growth |
| L6 | H5 slope test passes |
| L8 | gates for the target rung pass; second-operator reproduction |

### 13.3 Rollout

1. Publish baseline report (L2) so the gap is visible.
2. Keep L0 limit; ship upload/ingest behind a tenant flag for internal use.
3. Climb the ladder one rung per accepted report; announce each rung with the
   report link and its `not_measured` list.
4. Per-tenant opt-in for L4/L5.

### 13.4 Migration

Existing uploads remain in `document_upload`; they are not migrated. New large
originals live as asset content trees. A background job may later move legacy
`bytea` originals (open question L-Q6).

## 14. Risks and open questions

### 14.1 Risks

| Risk | Mitigation |
|---|---|
| Thresholds set without data are wrong | baseline L2 first; thresholds revised once, in writing, before gating |
| Generator not representative | host-mimic calibration; real-file spot checks |
| Single-engineer bottleneck on L5/L6 | stage Word first; keep Excel/PowerPoint behind separate gates |
| PostgreSQL becomes the bottleneck for object writes | storage-backend spike (plan part 2) uses the same harness |
| Load tests disturb production | never run against production; R1 is isolated |
| "Hundreds of MB" is dominated by media; text-heavy files are the real stress | composition matrix reports each separately |

### 14.2 Open questions

| # | Question | Recommended default | Decided by |
|---|---|---|---|
| L-Q1 | Where is the original binary stored for large files: content tree in PostgreSQL or an object store? | content tree via existing store trait; object store after the spike | Engineering lead |
| L-Q2 | Adopt tus wire format or a Dynodoc-specific resumable protocol? | tus 1.0.0 semantics with Dynodoc auth headers | Engineering lead |
| L-Q3 | Do we keep the original at all after ingest for huge files? | keep (restore fidelity), policy per tenant | Product owner |
| L-Q4 | Server-side parsing of customer files raises the sandboxing bar; run workers in a locked-down container? | yes, no network, read-only root | Security lead |
| L-Q5 | CDC seed scope: per document or per project? | per document | Engineering lead |
| L-Q6 | Migrate legacy `document_upload` rows? | no until needed | Engineering lead |
| L-Q7 | Which rung does the owner want first (25 or 100 MB)? | 25 MB | Product owner |
| L-Q8 | Is compression of stored content objects in scope? | separate design; measure first (A6 in the research register) | Engineering lead |

## Appendix A. Glossary

| Term | Meaning |
|---|---|
| Size class | compressed native file size bucket (S0-S4) |
| Composition | media / balanced / text share of a fixture |
| Gate | hypothesis that must pass for a rung of the limit ladder |
| Open loop | load generation by schedule, independent of response times |
| Part | one file inside an OOXML (OPC) package |
| CDC | content-defined chunking |
| Host mimic | generator mode reproducing save-time byte changes of real hosts |

## Appendix B. References

| Source | Used for | Checked |
|---|---|---|
| Wen Xia et al., "FastCDC: a Fast and Efficient Content-Defined Chunking Approach for Data Deduplication", USENIX ATC 2016, pp. 101-114, https://www.usenix.org/conference/atc16/technical-sessions/presentation/xia | CDC design; claimed speedups (about 10x vs Rabin-based, about 3x vs Gear/AE-based) | 14 October 2026 (abstract) |
| restic design reference, https://restic.readthedocs.io/en/stable/100_references.html | CDC with Rabin fingerprints, 512 KiB to 8 MiB blobs, 1 MiB average, random polynomial per repository | 14 October 2026 |
| tus resumable upload protocol 1.0.0, https://tus.io/protocols/resumable-upload | upload creation, HEAD offset, PATCH at offset, checksum and termination extensions | 14 October 2026 |
| Dolt "Prolly Trees", https://www.dolthub.com/blog/2024-03-03-prolly-trees | content-defined index structures and history independence | 14 October 2026 (search summary) |
| Repository benchmarks under `docs/benchmarks/` | anchors in section 2.2 | in repository |

Not verified in this pass (check at build time): PostgreSQL maximum `bytea` field
size and TOAST behaviour at hundreds of MB; Word/Excel/PowerPoint acceptance of zip
data-descriptor packages; availability of OOXML XSDs for validation; the exact
production VPS specification.
