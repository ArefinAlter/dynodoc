# Versioning implementation and theory

## Current audit boundary - 11 October 2026

Schema 0028 adds immutable legacy snapshots/receipts and checked serving paths.
Old snapshots require explicit replay backfill; API/CLI audits independently
compare all legacy snapshots with canonical replay, including non-node state.
See [the snapshot contract](VERIFIED-SNAPSHOTS.md) and [validation status](ENGINE-EVOLUTION-HANDOFF.md).
`log::verify_chain` alone remains an event-hash check. Receipts are internal
attestations, not signatures or independent witnesses. The runtime still owns its
schema and applies migrations: privilege separation is the next urgent gate.

## Evolution part 2a - 10 October 2026

[Shared checkpoints v1](SHARED-CHECKPOINTS.md) add an opt-in content-addressed
state graph covering nodes/tombstones, comments, suggestions and removed choices.
Stable-key radix indexes reuse unchanged objects and avoid array-suffix rewrites
on insertion. Canonical vectors, full-state round trips/property cases, malformed
graph rejection, legacy replay equivalence, database rollback/concurrent retry and
erasure/isolation tests exercise this contract. The normal read/snapshot path is
unchanged; a hash-valid root is not an externally witnessed proof of event replay.

The storage benchmark shows unique-content growth on a synthetic 8,000-block trace;
it does not establish native-file throughput or database/service capacity. Creation
still scans/encodes the full state, map construction sorts entries, reconstruction
hydrates full state, and objects over 1 MiB fail explicitly. Portable commit/DAG
exchange, default-path migration, large text/assets and stronger audit remain open.

## Evolution part 1 - 10 October 2026

[Decision 012](decisions/012-local-first-document-version-control.md) and
[PRODUCT-SPEC](PRODUCT-SPEC.md) define the current target. D.2 now states a conditional
read/write/validation-footprint theorem; different target IDs alone are insufficient.
Regression examples cover unique-name collisions and crossed structural moves.

Historical batches/draft bases and review/provenance checkpoints share the core
`read_state_at` reader: nearest snapshot at/before the exact revision, streamed
suffix, revision/gap rejection and replay accounting. Tests compare every revision
across checkpoint boundaries with full replay, including zero/future revisions and
another document's checkpoints. No preceding checkpoint still means genesis replay.
Chain verification streams events; periodic cadence checks query only sequence
metadata. Full current-state hydration, JSON snapshots and draft bases remain.

Browser block alignment now caps extra Myers search work and trace cells and uses
linear duplicate counting followed by unique-anchor/LIS fallback. Exceeding the
budget can retain fewer matches and require more review; it never establishes
identity from ambiguous repeated content. Other shape matching, large-file ingest
and service concurrency still need profiling and resource bounds.

See [the handoff](ENGINE-EVOLUTION-HANDOFF.md) for exact verification/publication
state. The following September assessment is retained as a dated baseline; its
portable-sync limits must be read alongside subsequent proposal/recovery releases.

Reviewed 21 September 2026 against `02-versioning-design-vol1.md`,
`03-versioning-mathematics-vol2.md`, the PoC decision and the hosted-workspace extension.
The implemented product is centralized semantic version control with human resolution
of overlapping edits. It is not the complete distributed protocol in the mathematics document.

## What matches the design

| Principle | Implementation and evidence |
|---|---|
| Identity independent of content and position | ULIDs identify nodes. Tiptap block IDs survive ordinary edits; spreadsheet cells are fields of stable row nodes. `changes.test.ts` checks unchanged IDs, insertion order and restoration. |
| Append-only source of truth | PostgreSQL event UPDATE/DELETE guards (whole-document erasure is the explicitly audited exception in decision 005); `log::append_in_tx` locks the document row before allocating sequence numbers. Materialized state is replayed from events. DB-backed concurrent append and rollback tests cover the write boundary. |
| Hash-linked semantic history | Length-framed canonical event content and actor IDs feed SHA-256; each chain hash includes its predecessor. Verification catches changed payloads, actor/target edits and reordering. The genesis predecessor is 32 zero bytes, as specified by the build-stage contract. |
| Derived snapshots and Merkle commitments | Snapshots store replayed state, an event-chain head and a Merkle root over ordered node hashes (including tombstones), with separate leaf/internal prefixes. Property tests compare full replay with snapshot-plus-tail replay. |
| Independent edits combine | Three-way merge compares the ancestor, current team version and draft by stable node ID and field. Independent fields and cells combine; diverging same-field values require a choice. Delete/edit and divergent locations are conflicts in the workspace merge implementation. |
| Compact rich-text history | `RichTextPatched` interns style metadata and retains unchanged text; exact base/result hashes guard replay. Full-field events remain the fallback. Shared Rust/browser fixtures and the DB-backed API test exercise the contract. |
| Restore without erasing history | Restoration emits new operations, including `NodeRestored`; it never resets canonical history. Named-version metadata and legacy snapshots have immutable guards; backing snapshots require replay-verification receipts before serving. |
| Review before canonical acceptance | Suggestions remain separate until an authorized author accepts. Acceptance and the wrapped content edit occur in the same transaction. |
| Semantic integrity before deployment | Duplicate names, missing references, dependency cycles, incompatible numeric operands and undefined choices block deployment. This audit fixed malformed syntax passing unchecked and included the newer editor's `question_type` in type validation. Conditional-required expressions are now also inspected. |
| Understandable editing language | Personal draft, team version, save version, share changes, get latest, compare, restore and who-changed-this expose familiar actions through the GUI. |

## Exact limits of the claim

- Rich text now supports compact, base-checked formatting-run deltas and disjoint
  wording/formatting merges inside paragraphs, headings and code blocks. Competing
  properties, ambiguous insertion boundaries and overlapping structural edits still
  require review. See [the research and implementation record](RICH-TEXT-VERSIONING-RESEARCH.md)
  for the exact algorithm, benchmarks, import limits and remaining CRDT/storage work.
- The system uses one PostgreSQL authority and a total order per document. There are
  no vector clocks, disconnected replicas, offline synchronization, CRDT move protocol
  or decentralized convergence proof. CRDT stage 20 was explicitly deferred.
- The disjoint-write theorem in the mathematics document applies to independent,
  already-valid field writes. It must not be generalized to structural operations
  that share parents, references, uniqueness constraints or deletion dependencies.
  Combined structural edits still pass validation; the application does not claim
  unrestricted commutativity, associativity or idempotence for every operation.
- The event chain is linear; draft ancestry is represented by stored bases and
  draft edits, not a Git-compatible commit-object DAG. Personal draft edit rows are
  append-only but do not have the canonical event chain's cryptographic guarantee
  until changes are shared. Draft-to-draft collaborative review is still limited.
- Hash verification is tamper evidence against an existing commitment. It does not
  defeat a privileged database operator who rewrites the entire chain and replaces
  its head. Externally signed/anchored heads remain unimplemented. Timestamps and
  transport metadata are not all included in the current content-hash preimage;
  do not interpret verification as an independent timestamping service.
- Selected blocks can be held back from sharing. Per-draft exclusion preferences persist when selected sharing remembers them.
  Opt-in browser recovery preserves a save base and unsaved state; offline local
  repositories, portable clone/fetch protocols and arbitrary remotes are not
  implemented. Drafts remain server-backed personal workspaces.
- Membership/lifecycle changes now have a separate append-only operations audit. Document metadata and operational samples do not all use the semantic
  document event log. The cryptographic claim is specifically about recorded content
  operations, not every administrative action in the installation.
- Formula/expression support is a documented subset. Neither successful parsing nor
  a native preview proves arbitrary REDCap, SurveyCTO, CSPro or XLSForm runtime parity.

## What the document workflow means

| Git term | Dynodoc action | Scope |
|---|---|---|
| init / clone | New document / Make a copy | Copies start their own history. |
| local / remote | Personal draft / Team version | Both live on the server. |
| branch / switch | Create draft / Open draft | Author's own draft workspace. |
| stage / ignore | Include in sharing / Keep for later | Block selection with remembered per-draft exclusions; no path/glob ignore patterns. |
| commit | Save version | Named checkpoint of recorded work; legacy state verification remains an open security gate. |
| push / pull | Share changes / Get latest | Three-way comparison with the stored base. |
| merge / conflict | Combine changes / Review overlaps | Explicit choices before conflicting edits land. |
| checkout / revert | View version / Restore as new changes | Previous records remain intact. |
| blame / grep | Who changed this? / Find in workspace | Membership-scoped attribution and content search. |

These are evidence-backed mappings for a document editor, not a claim to implement
Git's wire protocol or every feature of the full theoretical roadmap.


The office-editor increment keeps these guarantees unchanged. Page settings, shared
font descriptors, spreadsheet layout and inline text-review marks are versioned
fields. Accept/reject operations append content changes. Text-review marks are an
editing affordance, not canonical authorship proof: authenticated event actors are
the attribution authority. Imported Word comment/revision authors are source-file
metadata, not verified Dynodoc identities. Public copies reject unresolved text
revisions and strip embedded private comments, cell notes and private font references.
See [the office capability record](OFFICE-EDITOR-CAPABILITIES.md).

Discussion replies and resolution/reopening are canonical semantic events with
thread revision checks. Discussion state is projected from event IDs, independently
of the structural node projection. Complete document erasure removes the chain;
it cannot be verified after erasure. A minimal administrative receipt survives,
without pretending to be a retained cryptographic proof of deleted content.
