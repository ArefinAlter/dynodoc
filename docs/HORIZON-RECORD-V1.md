# Horizon record v1, retention policy v1 and the read-only horizon planner

18 October 2026. Field-level specification for phases P0 and P1 of
[DESIGN-COMMIT-PRUNING.md](DESIGN-COMMIT-PRUNING.md) (item `E-PRUNE`), written from the
code and the independent vectors rather than from the design's sketch. Where this document
and the design differ, this document describes what was built and the differences are listed
in section 11.

## 0. What exists and what does not

**Built** (a library plus one read-only CLI verb; no schema, migration, hash or commit
format changed):

| Piece | Where |
|---|---|
| Record, envelope, address, signature domain, the four Merkle roots, the heads digest, invariants H1 to H10 as checks | `crates/engine-core/src/repository/horizon.rs` |
| Retention policy schema, strict loader, normalization, hash | `.../horizon/policy.rs` |
| Planner (`plan`), `HistoryView`, `HorizonPlan` | `.../horizon/plan.rs` |
| SQLite loader `LocalRepository::horizon_plan` | `.../repository/local/horizon_plan.rs` |
| `engine-cli repository DB PROJECT horizon-plan` | `crates/engine-cli/src/repository.rs` |
| Independent vectors and their generator | `crates/engine-core/tests/fixtures/horizon-v1.json`, `gen_horizon_vectors.py` |

**Not built** (design phases P2 and later): local schema 4 and its tables, the applier, the
skeleton-aware `import_commit` / `merge_bases` / `read_state`, `horizon-apply`,
`horizon-show`, `horizon-verify`, the archive writer and `horizon-rehydrate`, holds storage,
signing flows (nothing here creates a signature), sync, the shared service, the transparency
log leaf, and any deletion. A record produced from a plan has never been applied to a
repository. The checks in section 7 are functions over data a caller supplies; no code reads
them from a repository yet.

## 1. Conventions

* **Hex**: lowercase, no prefix. An *address* is 64 hex digits (32 bytes).
* **Canonical JSON**: UTF-8, object keys sorted by byte order, no whitespace, arrays in the
  stated order, `true`/`false`/`null`, integers in plain decimal. This is the encoding
  `Commit::bytes` uses (`CanonicalValue`). Non-ASCII text is written as UTF-8, not escaped;
  `"` and `\` and control characters escape as JSON requires (this is where two
  implementations most easily differ, so the vectors include a hold reason with Bengali text,
  U+1F512, a quote and a backslash).
* **Integers** in a record are non-negative and at most 2^53 - 1 (`MAX_JSON_INT`), so every
  JSON implementation represents them exactly. A larger value is a format error.
* **UUIDs** are lowercase hyphenated text; the nil UUID is refused for a project.
* A record parsed from bytes must re-encode to exactly those bytes (no extra whitespace, no
  reordered keys, no explicit `null` for an absent `archive`), and every object is closed:
  an unknown field is refused.
* The record is at most 64 KiB (`MAX_RECORD_BYTES`); an envelope at most 64 KiB + 4 KiB.

## 2. The record

```json
{"archive":{"bytes":3200145002,"commits":3,"objects":4,"sha256":"..."},
 "counts":{"commit_bytes_released":700,"commits_released":3,"object_bytes_released":4096,
           "objects_released":4,"states_released":2},
 "created_at_ms":1791086400000,
 "evidence":["holds","merge_bases","merge_inputs:states","recovery_sources"],
 "heads_digest":"...","holds":[{"commit":"...","reason":"legal-hold:LH-17"}],
 "kind":"dynodoc.history-horizon",
 "policy":{"hash":"...","id":"team-default"},
 "previous":"...","project":"5b0f5b77-5a35-47a6-9d61-2ad0a2c9b8d0",
 "roots":{"frontier":"...","released_states":"...","skeleton":"..."},
 "sequence":2,"tier":"history","version":1}
```

(Key order above is the canonical, sorted order.)

| Field | Rule |
|---|---|
| `version` | `1`. |
| `kind` | `"dynodoc.history-horizon"`. |
| `project` | The project UUID; the same on every record of a chain. |
| `sequence` | Integer from 1, contiguous per project (H1). |
| `previous` | Address of sequence `n - 1`; `null` exactly when `sequence == 1` (H1). The key is always present. |
| `created_at_ms` | Integer; declared by the signer, not verified. |
| `tier` | `"content"` or `"history"`. |
| `policy` | `{id, hash}`: the policy identifier (section 8) and its hash. |
| `heads_digest` | Section 5. Binds the branch heads the plan was computed against. |
| `counts` | Five integers. Logical (canonical encoded) bytes, objects counted per `(document, address)`. |
| `roots` | `skeleton`, `frontier`, `released_states`: section 4. |
| `archive` | Present exactly when an archive was written (the key is omitted otherwise). `{sha256, bytes, commits, objects}`; `commits` and `objects` equal `counts.commits_released` and `counts.objects_released`. |
| `holds` | Holds in force, `[{commit, reason}]`, strictly ascending by `(commit, reason)`, at most 128. `reason` is 1 to 200 bytes of UTF-8 with no control character (shown to project members; must not hold personal data). |
| `evidence` | Strictly ascending list from the closed vocabulary below. |

Evidence vocabulary: `holds`, `merge_bases`, `merge_inputs:bodies|none|states`,
`named_versions`, `open_review_candidates`, `recovery_sources`,
`reviewed_candidates:body|body+states|none`, `topology:drop`. It names the rules that
shaped the release; `topology:drop` is how a record says that no skeleton exists (the record
has no `topology` field).

Consistency rules (format errors, checked by `HorizonRecord::validate`):

* A `content` record releases no commit (`commits_released == 0`).
* `commits_released == 0` implies `commit_bytes_released == 0`; `objects_released == 0`
  implies `object_bytes_released == 0`; `states_released == 0` implies `objects_released == 0`.
* `policy.id` is 1 to 64 of `a-z 0-9 . _ -`, starting with a letter or digit.

**Address**: `SHA-256("dynodoc.history-horizon\0v1\0" || canonical record bytes)`, where `\0`
is a NUL byte.

## 3. Signatures

**Signature input** (Ed25519): `"dynodoc.history-horizon-signature\0v1\0" || the 32 raw address
bytes`. The domain differs from the commit signature domains
(`dynodoc.project-commit-signature\0vN\0`), so a commit signature cannot be replayed as a
horizon signature and a horizon signature cannot be a commit signature; the vectors include
both a commit-domain signature and a no-domain signature over the same address, and both are
refused.

**Envelope**: `{"horizon": <record>, "signatures": [{"role", "key", "signature"}, ...]}` in
canonical JSON. `key` is 64 hex digits, `signature` 128. At most 8 signatures, sorted
strictly ascending by `key` (so keys are distinct).

**Roles** are `owner`, `authority`, `device`. Where a horizon is verified decides which are
legal: `Scope::Local` accepts `owner` and `device`, `Scope::Shared` accepts `owner` and
`authority`. The number of distinct keys required comes from the policy (section 8); it is
supplied by the caller (`Envelope::verify(scope, required)`), and `validate_chain` takes a
function from `PolicyRef` to the requirement and treats an unknown policy as an H2 violation.
Producing signatures is out of scope; only the signing input and the verifier exist.

## 4. Merkle roots

Each root is an RFC 6962 section 2.1 tree over leaves sorted by the key shown, with
`leaf_hash = SHA-256(0x00 || leaf_data)`, `node = SHA-256(0x01 || left || right)`, split at the
largest power of two below `n`, and the empty tree hashing the empty string
(`e3b0c442...b855`, the same value for every kind). Duplicate keys are refused. Input order does
not matter. Leaf data begins with a domain string (NUL after the version, like the others):

| Root | Leaf data | Sorted by |
|---|---|---|
| `skeleton` | `dynodoc.horizon-skeleton-leaf\0v1\0` &#124; address &#124; parent count (1 byte, 0 to 2) &#124; parents in commit order (32 bytes each) &#124; height (u64 big-endian) | address |
| `frontier` | `dynodoc.horizon-frontier-leaf\0v1\0` &#124; child &#124; parent &#124; parent height (u64 big-endian) | `(child, parent)` |
| `released_states` | `dynodoc.horizon-state-leaf\0v1\0` &#124; document UUID (16 raw bytes) &#124; state root (32 bytes) | `(document, state root)` |
| *(plan only)* `retained` | `dynodoc.horizon-retained-leaf\0v1\0` &#124; address | address |

The first parent is significant, so parent order is part of the leaf. The *frontier of horizon
`n`* is every edge from a commit retained after `n` to a parent that horizon `n` released;
edges to commits released earlier belong to the earlier horizon's frontier. The *skeleton of
horizon `n`* holds exactly the commits released by `n`.

## 5. Heads digest

`SHA-256("dynodoc.horizon-heads\0v1\0" || lines)`, where `lines` is, for each branch ordered by
branch-name bytes, `name NUL commit-hex NUL generation-decimal LF`. Names are valid branch
names, generations positive, names distinct. No head, no lines (a defined, distinct digest).

## 6. Which commits a horizon can reference

Addresses are opaque 32-byte values, so a record, skeleton or frontier can name commits of
any version that exists today: v1, v2 (recovery) and v3 (same-file resolutions); their
addresses, signatures and bytes are untouched. The version matters only to the planner's
closure (section 9). A future v4 must declare its pin rules before it ships (design section 0.1).

## 7. Invariants

`horizon.rs` implements each as a check; a violation is `Error::Violated(Invariant::Hn, detail)`.
"Needs" lists inputs a repository does not provide yet.

| ID | Rule | Checked by | Needs |
|---|---|---|---|
| H1 | Sequences contiguous from 1, `previous` unbroken, one project | `HorizonRecord::validate`, `validate_chain` | |
| H2 | Signatures verify over the address, keys sorted and distinct, roles legal in the scope, enough distinct keys, policy known | `Envelope::verify`, `validate_chain` | policy requirement |
| H3 | Every parent of a retained commit is retained or in some skeleton | `check_state` | stored commits, skeleton rows |
| H4 | Skeleton parents closed; `height = max(parent heights) + 1` (0 for roots); frontier edges match a body and a skeleton entry | `check_state` | same |
| H5 | The three roots recompute from the rows, and the row counts equal `commits_released` (unless `topology:drop`) and `states_released` | `check_state` | rows |
| H6 | No current head released, in body or in states | `check_state` | branch heads |
| H7 | Retained and released are disjoint; an address is released at most once | `check_state`; the planner refuses a view that violates it | |
| H8 | A released state's objects are absent; every other state of a retained commit is present | `check_state` | the set of present states |
| H9 | Recorded counters equal a fresh recount | `check_counters` (equality only) | local counters (schema 2+) |
| H10 | The record carries no path, message, key or name: closed fields, closed evidence vocabulary, plain policy id, bounded hold reasons | `HorizonRecord::validate` / `from_bytes` | |

H9 compares two values the caller supplies; how skeleton rows are charged to the local
counters is not specified (section 11).

## 8. Retention policy `dynodoc.retention-policy.v1`

A JSON document (YAML is not accepted; the design's example converts one to one). The loader
(`Policy::load(bytes, LoadOptions { min_signatures })`) rejects duplicate keys, unknown fields
at any depth, floats, out-of-range numbers and values outside each field's vocabulary, and
returns **all** problems, each as `{path (JSON pointer), code, message}` sorted by path.

| Field | Values | Default |
|---|---|---|
| `version` | `1` | required |
| `id` | policy identifier (section 2) | required |
| `description` | text up to 1000 bytes, no control character but newline | `""` |
| `tier` | `content`, `history` | required |
| `window.keep_within_days` | 1 to 36500 | absent |
| `window.keep_last_commits` | 1 to 1,000,000 | absent |
| `window.min_commits_kept` | 1 to 1,000,000 | absent |
| `protect.branch_heads` | `true`; required explicitly when `tier` is `history` | `true` for `content` |
| `protect.named_versions`, `protect.open_review_candidates` | boolean | `true` |
| `protect.merge_bases` | `{against: branch, branches: [pattern]}`; pattern is a branch name or a prefix ending in `*` (`*` alone is every branch) | absent |
| `protect.reviewed_candidates` | `none`, `body`, `body+states` | `body` |
| `protect.recovery_sources`, `protect.holds` | `true` only | `true` |
| `protect.merge_inputs` | `none`, `bodies`, `states` | `states` |
| `protect.commits` | up to 4096 addresses | `[]` |
| `topology` | `keep`, `drop` | `keep` |
| `acknowledge_unmergeable_history` | boolean | `false` |
| `archive` | `{required: bool, destination: "file://..."}` using only `{project}` and `{sequence}` placeholders | `{required: false}` |
| `signatures.required` | 0 to 8, but at least `max(1, min_signatures)` | `1` |
| `quarantine_days` | 0 to 365 | `0` |

**Normalization and hash.** The loader writes every defaulted field out, omits unset optional
fields, sorts and de-duplicates `branches` and `commits`, and encodes canonical JSON. The
policy hash is `SHA-256("dynodoc.retention-policy\0v1\0" || that JSON)`. Loading the normalized
bytes gives the same policy and hash. The vectors reproduce the normalized text and hash of
four valid policies, including the design's example.

**Codes.** `policy.unknown_field`, `policy.heads_unprotected` (`branch_heads` false, or absent in
a history policy), `policy.window_empty` (none of the three window keys), `policy.signatures_below_floor`,
`policy.topology_drop_unacknowledged` and `policy.archive_unreachable` are the design's six
(section 4.3). `policy.archive_unreachable` is reserved: it needs a writability test, and the
planner is read-only, so nothing emits it before the archive writer (P3). Added by the loader:
`policy.malformed_json`, `policy.too_large` (over 64 KiB), `policy.duplicate_field`,
`policy.unsupported_version`, `policy.missing_field`, `policy.invalid_value`,
`policy.protection_not_optional` (`recovery_sources` or `holds` false: both are hard-coded
protections) and `policy.archive_destination_missing`. 31 hand-written invalid policies in the
vectors state the exact `(path, code)` set the loader must return.

## 9. The planner

`plan(&HistoryView, &Policy, &PlanOptions) -> HorizonPlan` is pure: it reads a view held in
memory. `LocalRepository::horizon_plan` builds the view from a repository on a private
`query_only` connection in one read snapshot, with the limits of `gc-plan` (1,000,000 commits,
4,000,000 objects; larger repositories are refused, never planned partially), then calls it.

**Inputs.** Branch heads; stored commits (address, version, parents, recovery references,
authored time, stored length, file states, v3 record states); skeleton entries of earlier
horizons; holds; and the object graph of every state (size and children per object). The
planner recomputes every height and compares it with the stored column, resolves every parent
and recovery reference (a missing one is an error), and refuses a head that an earlier horizon
released (H6) or an address that is both stored and released (H7).

**Rules, in order.** A commit is kept for the first reason that applies, in this order:

1. `branch-head`: every head.
2. `hold`, `policy-commit`: commits in the view's holds and in `protect.commits`. A named
   commit that is not stored draws a warning (it cannot be pinned).
3. `merge-base`: with `protect.merge_bases`, every best common ancestor (all of them, for
   criss-cross) of the `against` head and each other matching head. A missing `against` branch
   draws a warning.
4. `recent-count`: the first `max(keep_last_commits, min_commits_kept)` commits along first
   parents from each head (the head counts as the first). `recent-time`: every commit whose
   `authored_at_ms >= as_of_ms - keep_within_days * 86,400,000`. `as_of_ms` is an explicit
   input, required exactly when `keep_within_days` is set, and echoed in the plan; the
   library never reads a clock.
5. Evidence, **one level, not recursive**, from the commits kept so far: for a v2 commit its
   recovery source, mainline and first parent (`evidence-recovery`); for a v3 commit both
   parents and the unique best common ancestor (`evidence-merge`; no unique base is an error).
   An evidence commit that an earlier horizon released draws a warning.

**Tiers.**

* `history`: kept commits keep their bodies; every other stored commit is released into the
  skeleton (address, ordered parents, height), unless `topology: drop`, when the skeleton is
  empty (the plan still counts the released commits and lists `topology:drop` in `evidence`).
  The frontier is every edge from a kept commit to a released parent. The retained states
  `S` are the states of every kept body, evidence included; every other state is released.
* `content`: every body stays and nothing is released from the skeleton or frontier. `S` is
  the states of commits kept for their own sake and of v2 evidence, plus, with
  `merge_inputs: states`, the base, left and right states of the v3 records of those commits.
  Every other state is released.

**Objects.** An object is released when a released state reaches it and no retained state
does. `objects_measured` is true when every involved state has a measured object graph.

**Plan JSON.** `format` `dynodoc.horizon-plan.v1`; `project`; `read_only` (always true);
`plan_hash`; `tier`; `topology`; `as_of_ms` (only with a time rule); `policy {id, hash}`;
`heads_digest`; `heads`; `commits {total, retained, released}`; `counts` (the record's five
numbers); `objects_measured`; `roots {retained, skeleton, frontier, released_states}`;
`reasons [{reason, commits}]` in the fixed order above; `evidence`; `holds`; `warnings` (sorted
text); `examples {released, pinned}` (first N by address). It is canonical JSON with no
timestamp or host value. `plan_hash = SHA-256("dynodoc.horizon-plan-hash\0v1\0" || canonical
JSON without plan_hash and examples)`, so the number of examples never changes it. The full
sets behind the roots (`PlanSets`: retained, released, skeleton, frontier, released states,
reasons per kept commit) are kept in memory for an applier or a test and are not in the JSON.
`HorizonPlan::draft_record(sequence, previous, created_at_ms)` turns a plan into a validated
record; nothing is signed or stored.

**CLI.** `engine-cli repository DB PROJECT horizon-plan --policy FILE [--list N]
[--as-of-ms MS] [--out FILE] [--format json|text]`. It opens the database read-only (it never
creates, upgrades or writes it), prints the canonical plan on stdout and a summary on stderr
(`--format text` prints only the summary), and `--out` writes the canonical plan to a new file
and refuses to overwrite. Without `--as-of-ms` a time rule uses the current time rounded down
to a UTC day. Invalid policies print every problem. The design's `--top` is not offered (there
are at most eight reasons).

**Cost** with `C` commits, `E` edges, `V` v3 commits and `O` objects: `O(C + E)` graph work, one
ancestry walk (capped at 100,000 ancestors, as the importer's) per v3 commit and per matching
branch, so `O(V * C)` worst case, and `O(O)` object accounting. The design's 60 s target at
100,000 commits / 1,000,000 objects has not been met or missed: the ignored test
`planning_a_hundred_thousand_commits_takes_seconds` measured 12.7 s in a debug build for the pure
planner over 100,000 in-memory commits with no object graph (forks every 50, merges every 10);
loading from SQLite, the object walk and a release build were not measured.

## 10. Vectors and what they prove

`tests/fixtures/horizon-v1.json` is written by `tests/fixtures/gen_horizon_vectors.py`, a
standard-library Python implementation of sections 1 to 9 (the Ed25519 block is copied from
`gen_transparency_vectors.py` and checked against RFC 8032 section 7.1 on every run). From the
fixtures directory:

```text
python -I gen_horizon_vectors.py     # rewrites horizon-v1.json; `git diff` stays clean
```

`tests/horizon_vectors_test.rs` reproduces, byte for byte: 36 Merkle roots (four leaf kinds,
nine sizes, fed unsorted), heads digests, three records (canonical text, address, signing
input), eleven envelopes (accepted and refused, including domain confusion), a two-record
chain, four valid and 31 invalid policies, and 10 planner scenarios (history, content, dropped
topology, time window, merge-input modes, floor, released merge commit, warnings, explicit
pins, a second horizon over the first's output) as canonical plan text and hash. Because the
generator and the Rust code were written from the same specification, the vectors catch
implementation bugs and nondeterminism, not a shared misreading. They were generated and
reproduced on Windows only; no other platform has been run. The Merkle and Ed25519 parts
follow RFC 6962 and RFC 8032 but no *published* horizon vector exists to check against.

Property tests (`tests/horizon_plan_props_test.rs`, seed fixed, 300 cases per property by
default, `PROPTEST_CASES` / `PROPTEST_RNG_SEED` override) run random linear, branchy,
criss-cross and v2/v3-heavy histories under random policies of both tiers and check, with
naive recomputation: heads kept and bound by the digest; holds, pins, window and merge-base
pins kept; closure of kept v2/v3 commits; skeleton rows equal the real parents and heights;
`check_state` (H3 to H8) over the plan's own rows; merge bases on the graph built from the
plan's outputs equal the full graph's for **every** pair; released states and objects against
an independent per-state computation; plan bytes and hash identical across insertion orders;
planning the retained set plus skeleton again releases nothing. A coverage test counts that
the generator reaches criss-cross, evidence, holds, content tier, dropped topology and shared
states. `repository/local/horizon_plan_tests.rs` plans real SQLite repositories (including a
real v3 merge and a v2 revert) and compares against the repository's own `merge_bases`, against
re-materializing the states that would stay, and against byte-identical files before and after.

## 11. Decisions where the design was open, and ambiguities

Decisions taken (change them here and in the vectors together):

1. Leaf layouts and domains for `frontier`, `released_states` and the plan-only `retained`
   root, the 1-byte parent count, parents in commit order, the empty-tree root, and sorting by
   the keys in section 4: the design gave only a skeleton prefix and "same construction".
2. The frontier and skeleton of a horizon cover only what *that* horizon released.
3. The record has no `topology` field; `topology:drop` in `evidence` says it, and then the
   skeleton root is the empty root while `commits_released` stays positive.
4. Roles: `Local` = `owner`, `device`; `Shared` = `owner`, `authority`.
5. `recovery_sources` and `holds` may not be switched off (the closure is hard-coded, a hold is
   honoured); `branch_heads: false` is refused in both tiers; `topology: drop` in a content
   policy is refused.
6. `protect.open_review_candidates`, `reviewed_candidates` and `named_versions` are accepted and
   hashed but pin nothing locally: no such objects exist in a local repository. They are not
   listed in `evidence` by the local planner.
7. Closure is one level. In tier `history` the states of evidence commits are retained (`S` is
   the states of all kept bodies); in tier `content` only v2 evidence and, with
   `merge_inputs: states`, v3 record states are.
8. `keep_last_commits` and `min_commits_kept` are the same walk (the larger counts); a time
   rule needs an explicit `as_of_ms`.
9. A record lists every hold in force even when its commit is not stored (the planner warns).
   Holds enter the library through the view; no local storage of holds exists, so the CLI
   plans with none.
10. `archive.commits` and `archive.objects` must equal the released counts.

Ambiguities for the owner:

* **I3 wording.** "Merge bases computed on the retained graph equal the full-graph result"
  holds when the graph includes skeleton parents (what P2 will walk). It does not hold on bodies
  alone, even when the base is retained: `a` still names a released parent `k`, but without `k`'s
  skeleton row nothing says `k` leads on to the retained base `x` (test
  `merge_bases_need_the_skeleton_not_the_bodies_alone`). So `topology: drop` gives up merge-base
  answers across the horizon, as the design says; the tests assert the skeleton-augmented
  statement for every pair of commits, and the pin rule (all bases of live branch pairs are
  retained).
* **`merge_inputs: none` versus `bodies`** behave identically (the closure always retains the
  bodies); the difference exists only as an `evidence` word. `states` matters only in tier
  `content`.
* **Local skeleton charge.** Whether skeleton and horizon rows count in the local capacity
  counters (design 7.6 says they are charged) is not specified for SQLite; H9 is therefore
  equality of recorded and recounted values only.
* **Time base.** `authored_at_ms` is declared and forgeable (the design says so); the plan
  makes the reference instant explicit instead of reading a clock.
* **Hold source.** Where local holds are stored (a schema-4 table, design P6) is undecided.
* **Role vocabulary.** The design says `device` is legal "locally"; whether a local `owner`
  signature should also be legal was assumed yes.
