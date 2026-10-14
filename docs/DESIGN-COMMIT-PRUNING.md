# Design: commit pruning with signed history horizons

> **Status: design, not implemented.** 14 October 2026. Nothing in this document
> exists in source. It changes no commit format, migration, hash, API or CLI in the
> current repository. Part of the [remaining-engine-designs index](ENGINE-REMAINING-DESIGNS.md).
> Item: `E-PRUNE`. Depends on: nothing mandatory; strongly benefits from
> [witnesses and transparency](DESIGN-WITNESSES-AND-TRANSPARENCY.md) (publish horizon
> records) and from the receipt authority being installed
> ([decision](RECEIPT-AUTHORITY-DECISION.md)) before any shared use.

## 0. Summary

Today a local project repository keeps every signed commit forever. The only
deleting operation is the staged-object collector, which removes objects no commit
references and never touches a commit, signature or ref
([LOCAL-RETENTION-PLAN.md](LOCAL-RETENTION-PLAN.md)). The shared PostgreSQL service
deletes published history only through audited whole-document erasure
(`migrations/0036_shared_project_history.sql`). A team that edits for years
therefore accumulates metadata and file-state objects that nobody will read again,
and the only relief is raising capacity limits.

This design adds **history horizons**: signed, append-only records that say "on this
date, under this policy, these commits and file states were released; this is the
frontier that remains". Everything left behind stays verifiable. The key ideas:

1. **Do not rewrite commits.** A retained commit keeps its exact canonical bytes,
   address and signature even when its parents are gone. A horizon is *metadata
   beside* the commit graph, not a new commit version.
2. **Two tiers.** A *content horizon* releases object bytes of old file states but
   keeps every commit body (signatures and receipts keep verifying). A *history
   horizon* additionally releases commit bodies and keeps a compact **skeleton**
   (address, parent addresses, height) so ancestry, fast-forward and merge-base
   questions keep working.
3. **Evidence classes are pinned by policy**: branch heads, merge bases of live
   branches, open review candidates, recovery sources, merge inputs, legal holds.
4. **Authority-signed, two-person, plan-hash-bound.** A horizon is computed as a
   deterministic plan, reviewed, then applied only with the exact plan hash and the
   required signatures. The plan binds the branch heads it was computed against.
5. **Optional cold archive.** The planner can write the released commits and objects
   into a hash-committed archive bundle recorded in the horizon, so release is
   reversible by re-hydration.

What this does **not** do: it is not erasure, not redaction, and not a way to
rewrite or correct history. Commit messages and paths stay in retained commits.
Audited erasure ([decision 005](decisions/005-product-operations-and-recovery.md))
remains a separate operation.

### 0.1 Order and dependencies

| Needs first | Why |
|---|---|
| Nothing for local-only tier 1 and tier 2 | The local repository already has the plan/hash/apply pattern (`gc-plan`, `gc`). |
| Receipt authority installed and server rotation story | A shared horizon needs a signer whose key clients pin. Production has no authority today. |
| Optional: transparency log | Lets third parties see that a horizon happened and detect truncation outside policy. |

Recommended implementation order across the remaining designs is in the
[index](ENGINE-REMAINING-DESIGNS.md#4-ordering). Pruning is mostly independent of the
merge designs, but any new commit record types they add (for example a v4 project
resolution record) must declare their evidence-pinning rules here before they ship.

## 1. Purpose, goals, non-goals, personas, metrics

### 1.1 Problem statement

- Local SQLite stores grow without bound except for explicit limit increases
  (`crates/engine-core/src/repository/local/storage.rs`; defaults 4 GiB, 1,000,000
  objects, 100,000 commits, 4,096 branches).
- A commit manifest can hold up to 4,096 file entries and 1 MiB
  (`MAX_FILES`, `MAX_COMMIT_BYTES` in `crates/engine-core/src/repository.rs`). A
  project with many files pays that cost for every commit, even a one-file edit,
  because the file map is fully encoded per commit
  ([PORTABLE-PROJECT-HISTORY.md](PORTABLE-PROJECT-HISTORY.md)).
- The shared service charges logical metadata per project (1 GiB, 1,000,000 rows,
  4,096 branches by default, `project_history_storage`) and has no safe object
  collector ([SHARED-STORAGE-BUDGETS.md](SHARED-STORAGE-BUDGETS.md)).
- Customers (enterprise, academia) need a defensible retention statement: "we keep
  N years of full history, then M years of topology and signed receipts", and an
  auditor needs to confirm that nothing was removed outside that statement.

### 1.2 Goals

| ID | Goal |
|---|---|
| G1 | Release storage of commits and file-state objects older than a declared policy, locally and on the shared service. |
| G2 | Everything retained stays verifiable with today's verification code paths (commit signatures, addresses, receipts, client pins). |
| G3 | Release is deterministic, reviewable (dry run), idempotent and crash-safe. |
| G4 | A signed, append-only record of every release, bound to a policy hash and to the heads at that time. |
| G5 | Merge-base, fast-forward and "is this commit part of this project" questions keep answering after release (topology retained). |
| G6 | Reversibility through optional archive bundles. |
| G7 | Retention semantics expressible as data a customer can read, diff and approve (policy language). |

### 1.3 Non-goals

- Rewriting, squashing or re-signing commits. No commit address ever changes.
- Erasing personal data or secrets that were committed. That is audited erasure.
- Pruning the per-document event log or its hash chain. Events stay append-only
  (decision 005); a document-level truncation design would be a separate project.
- Removing receipts. `project_ref_event` rows and client receipt journals are
  evidence and are never released by this design.
- Automatic, unattended pruning in the first release. Every apply is an explicit act.
- Pruning in the browser or add-ins. Authoring remains CLI/engine (decision 014).

### 1.4 Personas and use cases

| Persona | Use case |
|---|---|
| Solo researcher with a local repository | Keep a year of full history, keep named milestones forever, reclaim disk. |
| Team administrator on the shared service | Release history older than 18 months while preserving everything merged through review. |
| Compliance officer | Prove that only policy-selected commits were released, who signed, and that a legal hold was honoured. |
| External auditor / grant funder | Verify retained history and the record of what was released, offline. |
| Offline replica user | Return after four months with local commits based on a now-released commit and still publish. |
| Operator | Plan the storage effect before applying; roll back through the archive. |

### 1.5 Success metrics

| Metric | Target (hypothesis; measure in phase P2) |
|---|---|
| Bytes reclaimed on a synthetic 5-year history under the default team policy | at least 80 % of object bytes for states older than the window |
| Skeleton size | at most 128 bytes per released commit |
| Verification after apply | 100 % of retained commits verify; zero silent holes (`horizon-verify` exit 0) |
| Plan time, local, 100,000 commits / 1,000,000 objects | under 60 s (the existing planner measured 3.1 s for 20,000 staged objects) |
| Apply crash safety | zero partial states across a crash-injection matrix of every statement boundary |
| Support burden | a customer can answer "what did you remove and why" from `horizon-show` alone |

## 2. Current state against target

| Capability | Tag | Evidence / path |
|---|---|---|
| Immutable commits and objects locally (SQLite triggers) | EXISTS | `crates/engine-core/src/repository/local.rs` (`SCHEMA`, `commits_no_delete`, `objects_no_delete`) |
| Read-only retention plan with categories staged / superseded / orphaned and a deterministic plan hash | EXISTS | `crates/engine-core/src/repository/local/retention.rs`, [LOCAL-RETENTION-PLAN.md](LOCAL-RETENTION-PLAN.md) |
| Hash-bound collector for staged objects only; log table `local_retention_log` | EXISTS | `crates/engine-core/src/repository/local/collector.rs` |
| Persistent capacity counters and limits | EXISTS | `crates/engine-core/src/repository/local/storage.rs`, `storage_v2.sql` |
| Recovery sources and mainline treated as roots by the planner | EXISTS | `retention.rs` (`recovery-source` reason) |
| Commit versions v1, v2 (recovery), v3 (same-file resolutions) | EXISTS | `crates/engine-core/src/repository.rs` |
| Merge bases from bounded ancestry walk (cap `MAX_ANCESTORS = 100_000`) | EXISTS | `local.rs` `collect_ancestors`, `best_common` |
| Receipts bind commit address and previous receipt, not ancestry bodies | EXISTS | `crates/engine-core/src/repository/attestation.rs` |
| Client receipt journal (pins, rotation, revocation) | EXISTS | `crates/engine-core/src/repository/receipt_journal.rs` |
| Shared published commits immutable except audited erasure | EXISTS | `migrations/0036_shared_project_history.sql` (`project_history_delete_guard`) |
| Quota release on object delete under erasure | PARTIAL | `migrations/0034_shared_storage_budgets.sql` (`account_shared_object_delete`); only reachable through the erasure guard |
| Safe garbage collection of shared `checkpoint_object` bytes | MISSING | [SHARED-STORAGE-BUDGETS.md](SHARED-STORAGE-BUDGETS.md): "No garbage collector" |
| Deleting a commit locally or on the service | MISSING | no code path; triggers forbid it |
| Horizon record, skeleton rows, tombstone-aware import | MISSING | none |
| Retention policy language | MISSING | none (only CLI flags `--top`, `--list`) |
| Archive bundle and re-hydration | MISSING | none |
| Retention scope on device keys (`history:retention`) | MISSING | scopes are `project:read`, `history:read`, `history:write`, `history:review` (`migrations/0040_device_key_project_grants.sql`) |

## 3. Architecture

### 3.1 Concepts

```
 commit DAG before                         after a history horizon (tier 2)

  c9 (main head)                            c9 (main head)           retained, body kept
   |                                         |
  c8 --- r7 (review candidate, merged)      c8 --- r7                retained
   |      |                                  |      |
  c6      c5                                c6      c5               retained (inside window)
   |       \                                 |       \
  c4        c3   <- fork point of branch X   c4        c3           retained (merge base pin)
   |        |                                |         :
  c2        c1                               [c2]     [c1]           SKELETON only: address,
   |                                          :                        parents, height
  c0                                        [c0]
```

- **Retained set `B`**: commits whose bodies stay.
- **Skeleton set `K`**: released commits whose address, parent addresses and height
  remain. `K` is signed by Merkle root in the horizon record.
- **Frontier**: edges from a commit in `B` to a parent in `K`.
- **State set `S`**: `(document, state_root)` pairs whose object graphs stay. `S`
  is always a subset of the states referenced by `B`.
- **Tier 1 (content)**: `B` = all commits, `S` shrunk. **Tier 2 (history)**: `B`
  shrunk, `K` = the rest, `S` shrunk accordingly.

### 3.2 Components

```
                 policy (data)                     signer(s)
                      |                                |
  repository --> [Planner] --plan(json, plan_hash)--> [Reviewer] --approve--> [Applier]
   (SQLite or        |                                                         |
    PostgreSQL)      |                                               +---------+----------+
                     |                                               |         |          |
                     +--> [Archiver] -> archive bundle (hashed)      v         v          v
                                                              horizon row   skeleton   delete batches
                                                              (append-only)   rows     (commits, objects,
                                                                                         reachability index)
  readers: import_commit / merge_bases / read_state / fetch / push consult the skeleton
  verifier: horizon-verify walks the horizon chain, skeleton closure and counters
```

| Component | Responsibility |
|---|---|
| Policy loader | Parse and canonicalize `dynodoc.retention-policy.v1`; reject unknown fields; compute policy hash. |
| Planner | Read-only. Compute `B`, `K`, `S`, counters, archive size; output deterministic JSON and `plan_hash`. |
| Archiver | Stream released commits/objects to an archive bundle; compute its hash; verify by read-back. |
| Signer | Produce signatures over the horizon record (device key for local owner; authority and owner for shared). |
| Applier | Under the writer lock, recompute the plan, compare hash, write horizon row, skeleton rows, delete in batches, fix counters, verify. |
| Readers | Accept skeleton parents; report `HistoryPruned` / `ContentPruned` as typed errors. |
| Verifier | Re-check a horizon chain and the closure invariants (section 5.4). |

### 3.3 Trust boundaries

| Boundary | Trusted for | Not trusted for |
|---|---|---|
| Local filesystem owner | everything locally (SQLite triggers do not stop the owner) | proving anything to a third party |
| Pruner key (local owner) | declaring a local horizon | being a server-side authority |
| Receipt authority key (shared) | attesting that released commits were valid when admitted and that the horizon follows policy | content correctness of released commits (unverifiable after release; see 8.2) |
| Policy | defining what is eligible | overriding pinned evidence classes the engine hard-codes (heads, verifiability closure) |
| Archive medium | holding bytes | integrity (hash-committed), confidentiality (separate concern) |

## 4. Retention policy language

A policy is data. It is canonical JSON on the wire; YAML is accepted by the CLI and
converted. The policy hash is `SHA-256("dynodoc.retention-policy\0v1\0" || canonical
JSON)`.

### 4.1 Example (proposed)

```yaml
version: 1
id: team-default
description: >
  Keep a year of full history. Keep named versions, merged-review evidence and
  recovery provenance forever. Keep topology for everything else.
tier: history              # content | history
window:
  keep_within_days: 365    # by publication time (shared) or declared authored_at_ms (local)
  keep_last_commits: 500   # per branch, counted along first parents
  min_commits_kept: 50     # floor per branch, never pruned below this
protect:
  branch_heads: true                 # always true; setting false is a validation error
  named_versions: true               # when project-level named versions exist (MISSING today)
  merge_bases:                       # fork points of live branches
    against: main
    branches: ["*"]
  open_review_candidates: true
  reviewed_candidates: body          # none | body | body+states
  recovery_sources: true             # source, mainline and parent of retained v2 commits
  merge_inputs: states               # none | bodies | states  (v3 base/left/right)
  holds: true
  commits: []                        # explicit addresses
topology: keep                       # keep | drop
archive:
  required: true
  destination: "file:///archives/{project}/{sequence}.dynarc"
signatures:
  required: 2                        # distinct signers (shared: authority plus at least one owner)
quarantine_days: 14                  # shared only: soft-release first, hard-delete later
```

### 4.2 Semantics

1. **Eligible** commits are those that match *none* of the keep rules. A commit is
   *recent* if any of `keep_within_days`, `keep_last_commits` or `min_commits_kept`
   keeps it. Time uses `published_at` on the service (a server fact) and
   `authored_at_ms` locally. `authored_at_ms` is declared and untrusted; a local
   policy based on it is an owner convenience, not evidence.
2. **Protect** rules add commits to `B` regardless of age. They compose by union.
3. **Closure** (hard-coded, not configurable). For every commit `c` in `B`:
   - v1: no extra requirement; parents may be in `K`.
   - v2 (recovery): bodies of `recovery.commit`, `recovery.mainline` and `parents[0]`
     are added to `B` (needed to re-run `load_recovery_plan` on import).
   - v3 (same-file resolutions): bodies of both parents and the unique merge base
     are added (needed by `check_same_file_provenance`). With `merge_inputs: states`,
     the three input states of every record are added to `S`; with `bodies` or
     `none`, `verify-merge` returns `inputs-released` for those records.
   Added commits are *evidence commits*. They are imported in *attested mode* on a
   shallow replica (section 5.5) so closure does not recurse.
4. **Heads** are never released. A branch's head commit and its states are in `B`
   and `S` by definition.
5. **Topology**: `keep` writes every released commit into `K`. `drop` omits the
   skeleton; it requires `acknowledge_unmergeable_history: true` and makes
   fast-forward and merge-base questions across the horizon unanswerable. `drop`
   is not offered on the shared service in the first release.
6. **Determinism**: all sets are ordered by address; the plan JSON has no
   timestamps or host-specific values (same convention as `gc-plan`).

### 4.3 Validation errors the loader must produce

| Code | Condition |
|---|---|
| `policy.unknown_field` | any field not in the schema |
| `policy.heads_unprotected` | `protect.branch_heads` false or absent when `tier` is `history` |
| `policy.window_empty` | no `keep_*` rule and no `min_commits_kept` |
| `policy.signatures_below_floor` | shared service: `required` below the service's configured minimum |
| `policy.topology_drop_unacknowledged` | `topology: drop` without acknowledgement |
| `policy.archive_unreachable` | `archive.required` and destination not writable (checked at plan time) |

### 4.4 Defaults by edition (proposed)

| Context | Default policy |
|---|---|
| Local developer | no policy installed; `horizon-plan` requires `--policy` |
| Team on the shared service | `retain-all` until an owner installs a policy; first policy needs two signatures |
| Compliance | `retain-all` plus holds; any shorter policy needs organization approval (open question CP-Q3) |

## 5. Formats and invariants (proposed)

### 5.1 Horizon record v1

Canonical JSON (the same canonicalization as `Commit::bytes`: sorted keys, compact,
UTF-8), at most 64 KiB.

```json
{
  "version": 1,
  "kind": "dynodoc.history-horizon",
  "project": "5b0f5b77-5a35-47a6-9d61-2ad0a2c9b8d0",
  "sequence": 3,
  "previous": "9a0c...e1",
  "created_at_ms": 1791000000000,
  "tier": "history",
  "policy": { "id": "team-default", "hash": "ab12...ff" },
  "heads_digest": "77c1...09",
  "counts": {
    "commits_released": 18234,
    "commit_bytes_released": 21983422,
    "states_released": 40211,
    "objects_released": 912223,
    "object_bytes_released": 3112345678
  },
  "roots": {
    "skeleton": "13f0...aa",
    "frontier": "c4d2...0b",
    "released_states": "5e61...42"
  },
  "archive": { "sha256": "e8f0...01", "bytes": 3200145002, "commits": 18234, "objects": 912223 },
  "holds": [ { "commit": "0d44...9c", "reason": "legal-hold:LH-17" } ],
  "evidence": ["reviewed_candidates:body", "merge_inputs:states", "recovery_sources"]
}
```

| Field | Rule |
|---|---|
| `previous` | address of sequence `n-1`; `null` iff `sequence == 1`. |
| `heads_digest` | `SHA-256("dynodoc.horizon-heads\0v1\0" || ordered "branch\0commit\0generation\n")`. Binds the world the plan was computed against. |
| `roots.skeleton` | RFC 6962-style Merkle root over sorted leaves `address || count || parents... || height_u64be`, each leaf prefixed `dynodoc.horizon-skeleton-leaf\0v1\0`. |
| `roots.frontier` | same construction over sorted `(child, parent, parent_height)` triples. |
| `roots.released_states` | same construction over sorted `(document_uuid, state_root)` pairs. |
| `archive` | present iff an archive was written. |
| `holds` | holds that were in force; informational and signed. |

**Address**: `SHA-256("dynodoc.history-horizon\0v1\0" || canonical bytes)`.
**Signature input**: `"dynodoc.history-horizon-signature\0v1\0" || 32 raw address bytes`,
Ed25519. Envelope:

```json
{ "horizon": { ... }, "signatures": [
    { "role": "owner",     "key": "<64 hex>", "signature": "<128 hex>" },
    { "role": "authority", "key": "<64 hex>", "signature": "<128 hex>" } ] }
```

### 5.2 Invariants

| ID | Invariant |
|---|---|
| H1 | Sequence numbers per project are contiguous from 1; `previous` chain is unbroken. |
| H2 | Every signature verifies; at least `policy.signatures.required` distinct keys signed; roles are legal (`owner`, `authority`; `device` locally). |
| H3 | For every retained commit `c` and every `p` in `c.parents`: `p` is retained or `p` is in the skeleton of some horizon. No silent holes. |
| H4 | Every skeleton entry's parents are retained or in the skeleton (closure); heights satisfy `height = max(parent heights) + 1` (0 for roots). |
| H5 | The skeleton Merkle root recomputed from stored rows equals `roots.skeleton` of the horizon that introduced the rows. |
| H6 | No current branch head is released (its body and states stay). Receipts for older generations may name skeleton commits; their validity does not depend on the body. |
| H7 | Released and retained sets are disjoint; a commit address appears in `K` at most once across horizons. |
| H8 | A `ContentPruned` state root is referenced only by commits that are retained; its objects are not present, and `released_states` lists it. |
| H9 | Capacity counters equal a fresh recount after apply (local). Project metadata counters equal a recount (shared). |
| H10 | The record never contains paths, messages, author keys or document names. Only addresses, UUIDs and counts. |

### 5.3 Local SQLite schema 4 (additive; sketch)

Local schema 3 is taken by transparent object compression (14 October 2026); pruning therefore
needs schema 4.

```sql
-- schema 3 -> 4, run once in the initialization transaction when a horizon is first written.
create table history_horizon (
    sequence integer primary key,
    address  text not null unique,
    record   blob not null check(length(record) <= 65536),
    signatures blob not null,
    created_wall_ms integer not null
);
create trigger history_horizon_no_update before update on history_horizon
  begin select raise(abort,'immutable horizon'); end;
create trigger history_horizon_no_delete before delete on history_horizon
  begin select raise(abort,'immutable horizon'); end;

create table history_skeleton (
    address text primary key,
    parents blob not null,                    -- concatenated 32-byte addresses, at most 2
    height  integer not null check(height >= 0),
    horizon_sequence integer not null references history_horizon(sequence)
);
create table history_released_state (
    document text not null, state_root text not null,
    horizon_sequence integer not null references history_horizon(sequence),
    primary key(document, state_root)
);
create table history_horizon_progress (      -- the only mutable table
    sequence integer primary key references history_horizon(sequence),
    phase text not null check(phase in ('planned','skeleton','commits','objects','verified'))
);
update local_meta set version=3 where singleton=1;
```

Schema 4 is chosen over a lazily created log table (as `local_retention_log` was)
because *readers* must understand the skeleton. A schema-2 client that opened a
pruned database would report `missing commit`, which fails closed but is
misleading. Schema-3-only binaries reject schema 4 up front, exactly as
schema-1-only binaries reject schema 2 today
([LOCAL-STORAGE-BUDGETS.md](LOCAL-STORAGE-BUDGETS.md)). No in-place downgrade.

### 5.4 Shared PostgreSQL (migration number assigned at implementation; sketch)

```sql
create table project_history_horizon (
    project_id uuid not null references project_settings(space_id),
    sequence bigint not null check(sequence >= 1),
    address text not null check(address ~ '^[0-9a-f]{64}$'),
    record bytea not null check(octet_length(record) between 1 and 65536),
    signatures jsonb not null,
    tier text not null check(tier in ('content','history')),
    actor uuid not null references identity(id),
    transaction_id bigint not null default txid_current(),
    created_at timestamptz not null default now(),
    primary key(project_id, sequence),
    unique(project_id, address)
);
create table project_history_skeleton (
    project_id uuid not null,
    address text not null check(address ~ '^[0-9a-f]{64}$'),
    parents text[] not null check(cardinality(parents) <= 2),
    height bigint not null check(height between 0 and 100000),
    horizon_sequence bigint not null,
    charged_bytes bigint generated always as (64 + 64 * cardinality(parents)) stored,
    primary key(project_id, address),
    foreign key(project_id, horizon_sequence) references project_history_horizon
);
create table project_history_horizon_batch (   -- one row per delete transaction
    project_id uuid not null, sequence bigint not null, batch bigint not null,
    transaction_id bigint not null default txid_current(),
    primary key(project_id, sequence, batch)
);
```

Required changes in the same migration (all by `create or replace` in a *new*
migration; applied migrations are never edited):

1. `project_history_delete_guard()` additionally returns `old` when
   `pg_trigger_depth() > 1` and a `project_history_horizon_batch` row exists with
   `transaction_id = txid_current()` for the row's project. This mirrors the audited
   erasure pattern (`erasure_receipt.transaction_id`).
2. `account_project_history()` already decrements counters on DELETE
   (`-old.charged_bytes`); skeleton rows are charged through the same trigger.
3. Device-key scope check widened to include `history:retention` (as migration 0040
   widened it for `history:review`).
4. `project_history_review` rows for released candidates: kept by default
   (`reviewed_candidates: body` pins the candidate). A policy that releases reviewed
   candidates deletes their review rows and must say so in `evidence`.
5. `project_ref_event` rows are never deleted and keep their `commit_address`.

### 5.5 Import and read rules that change

| Operation | Today | With horizons |
|---|---|---|
| `import_commit` parent lookup (`local.rs`) | missing parent -> `Missing` | parent in skeleton -> accepted; height from skeleton; recovery/v3 re-validation skipped for *evidence commits* imported in attested mode |
| `merge_bases` / `collect_ancestors` | bodies only | walks bodies, then skeleton parents; result may name a skeleton commit (a base with released body) |
| `prepare_merge` | needs base state | `BaseContentPruned` if base state released; explicit resolutions still possible |
| `read_state` of a released root | `Missing object` | `ContentPruned { horizon }` |
| `show COMMIT` of a skeleton commit | `Missing` | `HistoryPruned { horizon, parents, height }` |
| HTTP `GET /commits/{address}` | 404 | 410 `history_pruned` with `horizon` sequence |
| sync `plan` dependency walk | stops at published history | skeleton counts as published; stops |
| sync `pull` fast-forward check | walks ancestry | uses skeleton topology; if local head is below the horizon and not in skeleton -> `AncestryUnknown`, explicit flag required |

**Attested mode**: signature, address, project and scope are verified; plan
recomputation and v3 provenance checks are skipped because a horizon record signed
by the pinned authority (shared) or the local owner key (local) attests that these
commits passed full verification when they were admitted. This is a deliberate
trust delegation, documented in section 8.2.

### 5.6 Archive bundle `dynodoc.history-archive` v1

```
line 1   : {"kind":"dynodoc.history-archive","version":1,"project":"...","sequence":3}\n
repeated : <kind u8: 1=commit,2=object><key_len u16><key bytes><body_len u64><body bytes>
trailer  : SHA-256 over every preceding byte (32 bytes), then the 8-byte magic DYNARCEND
```

Commit entries hold the original signed envelope bytes; object entries use key
`document_uuid:address`. Re-hydration verifies every commit address against the
skeleton and every object hash against its address. The bundle may be encrypted by
the operator outside the engine; the horizon records the hash of the plaintext
stream and the hash of the stored file separately when encryption is used.

## 6. Algorithms

### 6.1 Planner

```
input: repository R, policy P
1  heads    <- all branch heads (name, commit, generation)
2  recent   <- first-parent walk from each head until window rules stop; plus time rules
3  pinned   <- union of protect rules evaluated on R
4  B0       <- heads ∪ recent ∪ pinned
5  B        <- closure(B0)                       # section 4.2 rule 3, fixpoint, depth 1
6  K        <- (all commits) \ B                 # if tier == history
7  S        <- states(B) restricted by tier:
              tier content  : states(B) minus states of commits older than window,
                              plus pinned states
              tier history  : states(B)
8  released_objects <- objects reachable only from released states
9  counts, roots, archive_estimate computed
10 emit plan JSON { policy_hash, heads_digest, B_count, K_count, counters_before/after,
                    roots, top_reasons, examples, plan_hash }
```

Cost, with `C` commits, `O` objects, `F` file entries: one pass over the commit
table with signature checks, `O(C + edges)` graph work, one point read per distinct
reachable object. This is the `gc-plan` cost model (see
[LOCAL-RETENTION-PLAN.md](LOCAL-RETENTION-PLAN.md) "Cost and limits"). Memory is
`~100 B/commit + 64 B/object` and the existing tracking limits (1,000,000 commits /
4,000,000 objects) are reused; larger repositories are refused, not planned
partially.

### 6.2 Applier (local, single database)

```
BEGIN IMMEDIATE
 1  verify schema 4 (or upgrade), writable handle, no open read-only snapshots
 2  recompute plan; require plan_hash and heads_digest equal
 3  verify signatures and signer eligibility; require archive file exists and verifies
 4  insert history_horizon (+ progress 'planned')
 5  insert skeleton rows, released_state rows           -> progress 'skeleton'
 6  drop+recreate commits_no_delete trigger *inside this transaction* (as collector does
    for objects); delete released commit rows           -> progress 'commits'
 7  delete released objects (same trigger technique)    -> progress 'objects'
 8  lower counters by exact deleted rows/bytes; recount; require equality
 9  progress 'verified'; COMMIT
```

For very large releases, steps 6-7 run in bounded batches in separate transactions
(each re-checks the guard and writes progress). An interrupted run resumes from the
recorded phase; the horizon row (step 4) makes the intent durable and the plan
reproducible from the record plus the skeleton rows (the set to delete is "commits
in `history_skeleton` with horizon_sequence = n that still have a body").

### 6.3 Applier (shared service)

Same state machine, with these differences:

- Quarantine: phase `quarantined` makes released commits unreadable through the API
  but keeps bytes for `quarantine_days`. A hard-delete batch is a second explicit
  act (same plan hash) and cannot run before the quarantine ends.
- Project advisory lock: the existing project transaction lock used by publication,
  review and ref updates (`SHARED-PROJECT-HISTORY.md`, "Branches and review")
  serializes the logical release. Hard-delete batches take the lock per batch.
- Object bytes: `project_published_object` reachability rows for released roots are
  deleted first (this alone removes download access and releases the 160-byte
  charges). Physical `checkpoint_object` deletion requires the object collector in
  section 6.4; before that exists, tier 1/2 release is *logical* on the service.

### 6.4 Shared object collector (prerequisite for physical reclaim)

A released `(document, address)` may be physically deleted only if no other root
reaches it. Roots are: `shared_checkpoint.state_root`, draft `base_state_root` /
`merge_base_root` (migration 0031), named-snapshot roots (0032), retained
`project_published_object` rows, `project_staged_object` acknowledgements and
staged commit state roots (0035). Algorithm per document under an advisory lock:
mark the typed graph from every root (bounded reads, `READ_BATCH_OBJECTS = 16`),
sweep the unmarked candidates that appear in the released set, delete in batches
under the horizon-batch guard, let `account_shared_object_delete` release quota.
This is the largest single task in this design (phase P5).

### 6.5 Verification (`horizon-verify`)

Checks H1-H10 in order; prints the first violated invariant with the offending
address. It reads commit bodies and signatures for retained commits (cost like
`gc-plan`), skeleton rows, and recomputes the three Merkle roots. It never needs a
released body.

## 7. Interactions

### 7.1 Receipts, pins and rotation

- Receipts bind `(authority, project, branch, head{commit, generation}, previous
  receipt, issued_at, issuer_key)` and nothing about ancestors
  (`attestation.rs`). Releasing ancestors changes no receipt's validity.
- A client pin stores the latest receipt; `Pin::advance` needs only the receipt
  chain. A client that pinned generation 12 whose commit is now a skeleton entry
  continues to verify receipts 13..n.
- The receipt journal gains a table `horizons(authority, project, sequence, body)`
  so a client remembers the latest horizon it accepted. A server that later serves
  a lower horizon sequence (rollback) or a different record at the same sequence
  (fork) is detected the same way receipts are. Format: same canonical envelope.
- Authority rotation: a horizon signed by the old authority key remains valid
  history; the client accepts it because the dual-signed rotation record chain
  proves key succession. Horizon signatures by a *revoked* owner device key remain
  valid for records created before revocation (consistent with "published history
  remains readable after device revocation", SHARED-PROJECT-HISTORY.md).
- If the receipt authority is never installed (today's production), shared
  horizons are refused. Local horizons do not need it.

### 7.2 Reviews and change requests

- A published candidate that was approved and merged is `reviewed_candidates`
  evidence: body retained by default. The review rows reference it by foreign key
  (`project_history_review.(project_id, candidate)`).
- Open candidates are protected.
- Stale candidates (published, never merged, older than window) are eligible. Their
  review rows go with them only when the policy says `reviewed_candidates: none`
  and the record's `evidence` array says so. Review decisions are audit evidence;
  open question CP-Q4 asks whether they should ever be released.
- Per-file change requests in the centralized workspace (drafts in
  `workspace_draft` and approvals in `change_request_review`,
  `migrations/0021_copies_roles_reviews_notifications.sql`) are outside this
  design: they reference documents and event sequences, not portable commits.

### 7.3 Recovery sources and same-file merges

Covered by the closure rule (section 4.2). A revert of a pruned commit cannot be
planned afterwards (`recovery-plan` needs the source body); the CLI reports
`HistoryPruned`, and `horizon-rehydrate` from an archive restores the ability.

### 7.4 Audited erasure

- Erasure of a document deletes envelopes whose `scope` contains it, transitively,
  including *retained* boundary commits, which may orphan descendants (existing
  behaviour: "An erased branch head is explicitly unavailable (409)").
- Skeleton rows hold only addresses; they are not erased. They are opaque hashes.
- Archive bundles contain content of released commits and objects. Erasure cannot
  reach them. Operators must treat archives as copies subject to the same erasure
  duty as backups ([decision 005](decisions/005-product-operations-and-recovery.md):
  "this release does not promise immediate deletion from backups"). A future
  refinement: per-document archive segments so a document can be dropped from an
  archive by rewriting the segment (open question CP-Q6).
- Erasure after pruning: `project_history_delete_guard` and the erasure receipt are
  unchanged; skeleton and horizon rows survive.

### 7.5 Shared PostgreSQL history and local SQLite

| Aspect | Local SQLite | Shared PostgreSQL |
|---|---|---|
| Storage unit | one file per project | per-project rows, per-document objects |
| Immutability | triggers dropped and recreated inside the apply transaction | guard function with horizon-batch marker |
| Quota | persistent counters decremented explicitly and recounted | `account_project_history` trigger on DELETE |
| Signer | local device key | authority + owner (2 signatures minimum by default) |
| Physical reclaim | immediate (`--vacuum` for file size) | needs object collector (6.4) |
| Reversal | archive re-hydration | archive re-hydration through an operator command |

### 7.6 Quotas

Release lowers `bytes_used`, `objects_used`, `commits_used` (local) and
`project_history_storage.bytes_used / rows_used` (shared). Skeleton rows are
charged (64 + 64 per parent bytes, 1 row). The horizon rows themselves are charged
(record size + 128). A pathological history of one-parent commits costs about 128
bytes per released commit, against a typical 600 to 1,000,000 bytes per body.
Per-document and per-account object budgets (migration 0034) are released only when
the object collector deletes bytes.

### 7.7 Offline replicas and holds

A replica offline for months may hold local commits based on a commit that is now
in `K`. Behaviour:

1. Push: the replica's new commit has a parent in the server skeleton. Accepted;
   height from the skeleton. Its files' objects are uploaded as usual.
2. Merge with main: the merge base is a skeleton commit. If its state is released
   the merge reports `BaseContentPruned`; the user performs a two-way comparison
   with explicit resolutions (like today's disconnected-history path).
3. Mitigation: **holds**. A device key can register `hold(commit, ttl)` for a
   commit it is based on; the planner pins held commits (body and states) until
   expiry. Defaults: 90-day lease renewed on each successful sync, at most 20 holds
   per key per project, counted against project metadata quota. Holds are visible
   in the record. This is optional in phase P6.

### 7.8 Interaction with the other designs

- **Transparency log**: a horizon record is a leaf type (`history-horizon`); the
  log makes out-of-policy truncation detectable by third parties
  ([DESIGN-WITNESSES-AND-TRANSPARENCY.md](DESIGN-WITNESSES-AND-TRANSPARENCY.md)).
- **Cross-document merge**: relocation and project-resolution records reference
  state roots; pinning rules for them are an input to section 4.2
  ([DESIGN-CROSS-DOCUMENT-MERGE.md](DESIGN-CROSS-DOCUMENT-MERGE.md)).
- **End-to-end encryption** (documented separately, not built): a server that
  cannot read content can still hold skeleton and counts; planning that depends on
  reading typed states (`S` closure) would have to move to a client. Keep the
  planner's input interface limited to addresses, parents, versions and sizes so it
  can run client-side.
- **Large files**: content horizons matter most for large assets
  ([DESIGN-LARGE-FILE-ACCEPTANCE.md](DESIGN-LARGE-FILE-ACCEPTANCE.md)); chunk-tree
  sharing means released bytes are only those not referenced by retained states.

## 8. API and CLI surface (proposed)

### 8.1 CLI

```text
engine-cli repository DB PROJECT retention-policy validate POLICY.yaml [--format json|text]
engine-cli repository DB PROJECT horizon-plan  --policy POLICY.yaml [--top N] [--list N]
                                               [--out PLAN.json] [--format json|text]
engine-cli repository DB PROJECT horizon-apply --plan-hash HASH --key KEY.der
                                               [--cosign SIGNATURE.json]... [--archive-dir DIR]
                                               [--vacuum] [--format json|text]
engine-cli repository DB PROJECT horizon-show   [--sequence N] [--skeleton-page AFTER]
engine-cli repository DB PROJECT horizon-verify
engine-cli repository DB PROJECT horizon-rehydrate --archive FILE --key KEY.der
engine-cli repository DB PROJECT hold add COMMIT --reason TEXT [--days N]
engine-cli repository DB PROJECT hold list|remove
```

Exit status is non-zero for a refusal. `horizon-plan` opens the database read-only
exactly as `gc-plan` does (`open_read_only`, `query_only`).

### 8.2 HTTP (shared; all under `/projects/{id}/history`)

| Method and path | Scope | Behaviour |
|---|---|---|
| `GET /horizons?after_sequence=&limit=` | `history:read` | Signed horizon envelopes, ascending, at most 64 per page. |
| `GET /horizons/{sequence}/skeleton?after=&limit=` | `history:read` | Skeleton entries, at most 256 per page, with Merkle audit data for the page. |
| `POST /horizons/plan` | `history:retention` | Body: policy JSON. Returns plan and `plan_hash`; no change. Rate limited as an expensive read. |
| `POST /horizons` | `history:retention` | Body: `{plan_hash, record, signatures}`. Creates the horizon in quarantine. 409 on stale heads. |
| `POST /horizons/{sequence}/release` | `history:retention` | Starts hard-delete batches after quarantine. Idempotent. |
| `GET /horizons/{sequence}/progress` | `history:read` | Phase and counters. |
| `POST /holds`, `DELETE /holds/{id}` | `history:write` | Replica holds (phase P6). |

Errors: 409 stale plan or heads; 410 `history_pruned` / `content_pruned`; 403
missing scope or insufficient signatures; 507 on quota (skeleton/horizon charges).
The HTTP body cap for mutations stays 20 KiB except `POST /horizons`, which is
bounded by the 64 KiB record limit. `POST /horizons` signing is done offline by the
owner using `engine-cli`; the service countersigns with the authority key inside
the same transaction that inserts the horizon row (same pattern as atomic ref
receipts).

## 9. Security and privacy

### 9.1 Assets

Retained commit evidence; receipts and pins; the horizon chain; archives; policy
integrity; pruner and authority keys.

### 9.2 Threat model (STRIDE-style)

| # | Threat | Category | Control | Residual |
|---|---|---|---|---|
| T1 | Compromised owner key prunes evidence to hide an action | Tampering / Repudiation | Two signatures; quarantine; archive required; horizon record logged to transparency log; plan lists examples | Authority + owner colluding or both compromised |
| T2 | Operator with database access deletes commits and fabricates a horizon | Tampering | Horizon needs authority signature the DBA lacks (key mounted separately); client pins detect lower sequence | DBA who also holds the authority key (existing receipt limitation) |
| T3 | Rollback: server serves old database without the horizon | Tampering | Client journal remembers the latest accepted horizon sequence; receipts still advance | First-contact clients (no prior pin) |
| T4 | Forged "pruned" claim to justify an unknown parent in an import | Spoofing | Skeleton rows only from verified horizon; import checks the skeleton table, not the claim | none beyond key compromise |
| T5 | Accidental over-pruning | Denial of history | Dry run, plan hash, heads digest, min commits floor, quarantine, archive | Human error in policy; mitigated by review |
| T6 | Race with a concurrent publish moves a head | Tampering | Plan binds `heads_digest`; apply under the project lock | none |
| T7 | Replay of an old signed plan | Replay | `previous` chain and sequence; heads digest must match now | none |
| T8 | Skeleton leaks topology and timing | Information disclosure | Project-member read only; no paths/messages/keys in the record (H10) | Members learn graph shape of released history |
| T9 | Archive stolen | Information disclosure | Operator-side encryption; archive contains whole released content; treat as a backup | Operator practice |
| T10 | Apply exhausts the service | Denial of service | Batches, per-project lock, expensive-read admission, quarantine delay | Fleet-wide fairness is not modelled |
| T11 | Released evidence needed in a dispute | Repudiation | Archive and re-hydration; holds | Archive loss |

### 9.3 What is explicitly not claimed

- That released commits were authentic. After release, signature verification of
  those commits is possible only with an archive or an unpruned replica. The
  authority attests they verified at admission; that is a trust statement.
- That pruning satisfies a deletion request. It does not (7.4).
- That a witness or log makes pruning reversible. It makes it *visible*.
- Protection against a local filesystem owner editing SQLite directly (existing
  limit).

### 9.4 Privacy review

The horizon record and skeleton contain only addresses, document UUIDs, counts,
policy identifiers and hold reasons (free text, bounded 200 bytes; warn that it is
shown to members). Hold reasons must not contain personal data; the CLI warns.
Skeleton addresses are 64-hex hashes of commits that contained paths and messages;
they are not reversible but are linkable to receipts.

## 10. Operations

### 10.1 Deployment and upgrade

1. Ship readers first (schema 4 local, migration on service) with writers off by
   default. Old binaries refuse schema 4 / unknown migration, failing closed.
2. Enable `horizon-plan` (read-only) everywhere before any apply.
3. Enable apply per project by installing a policy and granting `history:retention`.

### 10.2 Rollback

- Local: restore the pre-apply backup or re-hydrate from the archive. Schema 4 has
  no downgrade; take a filesystem-consistent copy first (same advice as schema 2).
- Service: before quarantine ends, "cancel" restores readability (no bytes were
  deleted). After hard delete, re-hydrate from archive or restore a database dump
  (the schema-40 release routine already uses dump restore as rollback,
  [release record](operations/2026-10-13-schema40-release.md)).

### 10.3 Observability

| Metric | Meaning |
|---|---|
| `horizon_plan_seconds`, `horizon_plan_commits`, `horizon_plan_objects` | planner cost |
| `horizon_apply_phase{phase}` | state machine position per project |
| `horizon_released_commits_total`, `horizon_released_bytes_total` | effect |
| `horizon_verify_failures_total{invariant}` | H-invariant violations |
| `skeleton_rows`, `skeleton_bytes` | overhead |
| `hold_count`, `hold_expired_total` | replica holds |

Audit: every horizon creation, quarantine release and cancel is an operational
audit event (same table family as device-key and metadata audit) without readable
content.

### 10.4 Failure modes

| Failure | Behaviour | Recovery |
|---|---|---|
| Crash between statements (local) | SQLite rolls back the transaction; batch mode resumes from `history_horizon_progress` | rerun `horizon-apply` with the same plan hash (idempotent) |
| Plan stale (branch moved) | refusal; nothing changes | replan |
| Counters disagree after apply | transaction aborted before commit | investigate; counters are authoritative checks, not repaired silently |
| Archive write fails | apply refuses before step 4 | fix destination |
| Signature key revoked between plan and apply | refusal | re-sign with active key |
| Replica pushes history whose parent is unknown and not in skeleton | `Missing` as today | fetch/fast-forward first |
| Skeleton row without matching horizon (corruption) | `horizon-verify` H3/H5 fail; readers fail closed | restore |
| Authority key unavailable | shared apply refused | install/restore key; local unaffected |
| Quota exceeded by skeleton charges | 507, rollback of that batch | raise limit (operator) or choose `topology: drop` locally |
| Clock skew | `keep_within_days` uses server `published_at`; local uses declared time | document |

## 11. Delivery plan

### 11.1 Phases

| Phase | Deliverable | Depends on | Effort (person-weeks) | Confidence |
|---|---|---|---|---|
| P0 | Horizon record v1 spec, Merkle construction, independent Python vector generator, fixture `horizon-v1.json`, policy schema and loader with validation errors | none | 1.5 - 2 | high |
| P1 | Local planner (extends `retention.rs`): closure, tiers, plan JSON, `horizon-plan`, property tests | P0 | 3 - 4 | medium |
| P2 | Local schema 4, applier (batched, resumable), counter recount, tombstone-aware `import_commit`/`merge_bases`/`read_state`, `horizon-verify` | P1 | 4 - 5 | medium |
| P3 | Archive writer/reader, `horizon-rehydrate`, signer roles, two-signature flow | P2 | 2.5 - 3 | medium |
| P4 | Sync: horizon fetch/verify, shallow fetch, journal `horizons` table, pull/push behaviour | P2 | 3 - 4 | medium |
| P5 | Shared service: migration, guard changes, scope `history:retention`, HTTP routes, quarantine, logical release | P0, authority installed | 5 - 6 | medium-low |
| P5b | Shared object collector (mark and sweep, batches, quota release) | P5 | 4 - 6 | low |
| P6 | Holds, Projects UI read-only views of horizons, runbook, observability | P4, P5 | 2 - 3 | medium |
| Total | | | 25 - 33 | |

**Assumptions**: one engineer who already reads the repository crate; CLI-only
surface in P1-P4; reviews and test hardware available. **Excludes**: Projects UI
authoring (browser holds no signing key, decision 014), independent security review
(budget 1 - 2 person-weeks separately), legal sign-off on retention language,
customer-specific policy consultancy, WebAssembly build, data migration of existing
production projects (none hold published history: shared writers are off).

### 11.2 Acceptance criteria per phase

| Phase | Acceptance |
|---|---|
| P0 | Vector reproduced by Python and Rust byte for byte; every H-invariant has a negative test. |
| P1 | On 10 synthetic histories (linear, branchy, criss-cross, v2/v3-heavy) the plan keeps all heads, merge bases and closure; plan hash stable across runs and platforms. |
| P2 | After apply: `merge_bases` equals the unpruned result for every pair whose bases are retained; `read_state` equals for retained roots; crash injection at every statement boundary leaves a repository that verifies or can resume. |
| P3 | Apply -> rehydrate yields a logical dump byte-identical to the original (compare ordered commit/object listings). |
| P4 | A fresh replica fetches a pruned project and verifies the horizon chain; an old replica pushes on a pruned base. |
| P5 | Real-router tests under the restricted runtime login: guard permits only horizon-batch deletes; quota counters recount; erasure still works afterwards. |
| P5b | Collector never deletes an object reachable from checkpoint, draft, named snapshot or retained published roots (property test against a graph oracle). |

### 11.3 Test strategy

1. **Property tests** (random DAG generator with v1/v2/v3 commits, random policies):
   I1 retained commits verify; I2 skeleton closure; I3 merge-base equivalence;
   I4 state equivalence; I5 heads unchanged; I6 counters recount; I7 idempotent
   apply; I8 archive round-trip; I9 plan determinism (shuffle insertion order).
2. **Crash injection**: failpoint hook between each statement and each batch; kill
   and reopen; assert verify-or-resume.
3. **Differential**: run a scripted history through "never prune" and "prune at
   every step" and compare observable CLI outputs for retained commits.
4. **Concurrency**: publish and ref update racing a plan; apply racing `gc`.
5. **Adversarial**: forged skeleton claim, record with wrong `previous`, mismatched
   Merkle root, signature by a revoked key, signature domain confusion with commit
   signatures (domain strings differ by design).
6. **Compatibility**: schema-2 binary against schema-3 file refuses; schema-3 binary
   reads untouched schema-2 and upgrades only on first apply.
7. **Benchmarks**: reuse `local-storage-growth.mjs` style; add plan/apply timing at
   10k, 100k commits and report bytes reclaimed.

### 11.4 Rollout

1. Land P0-P2 behind no flag (read-only paths) in the MIT engine repository.
2. Publish the CLI, run on owner-supplied real repositories with `--dry-run` only.
3. Enable apply locally for volunteers with mandatory archive.
4. Shared service: after authority installation and a production-copy rehearsal
   modelled on the schema-38 rehearsal; enable per project; quarantine of 14 days
   for the first three horizons.
5. Record every apply in the handoff and release notes.

### 11.5 Migration of existing data

None required. Existing local databases keep schema 2 until the first apply. Existing
shared data has no published history in production (shared writers are off), so the
first shared horizons will be on new data.

## 12. Risks and open questions

### 12.1 Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Attested-mode import weakens verification on shallow clones | a malicious authority could attest anything for released history | keep attestation only for evidence commits; transparency log leaf; archive allows later full verification |
| Object collector bug deletes live bytes | data loss | mark-sweep property tests; quarantine; backups required before first physical release |
| Policy language grows into a programming language | complexity | closed set of selectors; versioned schema; no expressions |
| Skeleton makes users think history is fully available | confusion | explicit `HistoryPruned` errors and UI wording |
| Merge fails for long-lived branches | user pain | merge-base pin rule; holds; warnings when a branch's fork point nears the window |
| Local `authored_at_ms` is forgeable | policy bypass | documented as owner convenience; shared uses `published_at` |

### 12.2 Open questions

| # | Question | Recommended default | Decided by |
|---|---|---|---|
| CP-Q1 | Is skeleton retention mandatory on the shared service? | Yes; `topology: drop` local only | Product owner |
| CP-Q2 | Who may sign shared horizons? | Authority plus one project owner; org admin for compliance projects | Product owner and security lead |
| CP-Q3 | Can an organization set a retention floor customers cannot shorten? | Yes, as a policy floor like review floors (`PROJECT-POLICY-AND-METADATA.md`) | Product owner |
| CP-Q4 | May review decisions ever be released? | No; keep the candidate body when a review exists | Product owner / compliance |
| CP-Q5 | Quarantine length | 14 days | Operator |
| CP-Q6 | Per-document archive segments to honor erasure inside archives | Defer; document that archives are backups | Product owner |
| CP-Q7 | Should the event-log chain get a signed truncation anchor? | Out of scope here; separate design | Engineering lead |
| CP-Q8 | Hold lease defaults and abuse limits | 90 days, 20 per key | Engineering lead |
| CP-Q9 | Is the authority key the same key as the transparency-log key? | No; separate keys and custody | Security lead |
| CP-Q10 | Tier 1 on the shared service before the object collector exists | Ship logical release only, label it "access released, bytes retained" | Product owner |

## Appendix A. Glossary

| Term | Meaning |
|---|---|
| Horizon | signed record of a release plus the cut it defines |
| Skeleton | retained address, parents and height of released commits |
| Frontier | edges from retained commits to skeleton parents |
| Evidence commit | commit retained only because a retained commit's verification needs it |
| Attested mode | import mode that skips plan recomputation, relying on a signed horizon |
| Hold | time-limited pin of a commit by a replica or an administrator |
| Quarantine | period during which released data is unreadable but still stored |

## Appendix B. References

Repository documents and code: see the paths cited inline. External:

| Source | Used for | Checked |
|---|---|---|
| Git `git-clone` documentation, https://git-scm.com/docs/git-clone | shallow clones (`--depth`, `--shallow-since`) as prior art for truncated history; partial clone `--filter` | 14 October 2026 |
| RFC 6962 / RFC 9162 (Certificate Transparency), https://www.rfc-editor.org/rfc/rfc9162.html | Merkle tree construction reused for horizon roots; RFC 9162 is Experimental status | 14 October 2026 |

Git's shallow-file mechanics (what `git fetch` does with a shallow boundary) were
not verified in this pass: unverified, check at build time.
