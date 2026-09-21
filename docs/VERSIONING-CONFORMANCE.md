# Versioning implementation and theory

Reviewed 21 September 2026 against `02-versioning-design-vol1.md`,
`03-versioning-mathematics-vol2.md`, the PoC decision and the hosted-workspace extension.
The implemented product is centralized semantic version control with human resolution
of overlapping edits. It is not the complete distributed protocol in the mathematics document.

## What matches the design

| Principle | Implementation and evidence |
|---|---|
| Identity independent of content and position | ULIDs identify nodes. Tiptap block IDs survive ordinary edits; spreadsheet cells are fields of stable row nodes. `changes.test.ts` checks unchanged IDs, insertion order and restoration. |
| Append-only source of truth | PostgreSQL event UPDATE/DELETE triggers; `log::append_in_tx` locks the document row before allocating sequence numbers. Materialized state is replayed from events. DB-backed concurrent append and rollback tests cover the write boundary. |
| Hash-linked semantic history | Length-framed canonical event content and actor IDs feed SHA-256; each chain hash includes its predecessor. Verification catches changed payloads, actor/target edits and reordering. The genesis predecessor is 32 zero bytes, as specified by the build-stage contract. |
| Derived snapshots and Merkle commitments | Snapshots store replayed state, an event-chain head and a Merkle root over ordered node hashes (including tombstones), with separate leaf/internal prefixes. Property tests compare full replay with snapshot-plus-tail replay. |
| Independent edits combine | Three-way merge compares the ancestor, current team version and draft by stable node ID and field. Independent fields and cells combine; diverging same-field values require a choice. Delete/edit and divergent locations are conflicts in the workspace merge implementation. |
| Compact rich-text history | `RichTextPatched` interns style metadata and retains unchanged text; exact base/result hashes guard replay. Full-field events remain the fallback. Shared Rust/browser fixtures and the DB-backed API test exercise the contract. |
| Restore without erasing history | Restoration emits new operations, including `NodeRestored`; it never resets canonical history. Named checkpoints are immutable. |
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
- Selected blocks can be held back from sharing. Persistent exclusion rules, offline
  local repositories, portable clone/fetch protocols and arbitrary remotes are not
  implemented. Browser drafts are server-backed personal workspaces.
- Membership/lifecycle changes now have a separate append-only operations audit. Document metadata and operational samples do not all use the semantic
  document event log. The cryptographic claim is specifically about recorded content
  operations, not every administrative action in the installation.
- Formula/expression support is a documented subset. Neither successful parsing nor
  a native preview proves arbitrary REDCap, SurveyCTO, CSPro or XLSForm runtime parity.

## What the researcher workflow means

| Git term | Dynodoc action | Scope |
|---|---|---|
| init / clone | New document / Make a copy | Copies start their own history. |
| local / remote | Personal draft / Team version | Both live on the server. |
| branch / switch | Create draft / Open draft | Author's own draft workspace. |
| stage / ignore | Include in sharing / Keep for later | Block selection; no persistent ignore patterns. |
| commit | Save version | Named immutable snapshot of recorded work. |
| push / pull | Share changes / Get latest | Three-way comparison with the stored base. |
| merge / conflict | Combine changes / Review overlaps | Explicit choices before conflicting edits land. |
| checkout / revert | View version / Restore as new changes | Previous records remain intact. |
| blame / grep | Who changed this? / Find in workspace | Membership-scoped attribution and content search. |

These are evidence-backed mappings for a research editor, not a claim to implement
Git's wire protocol or every feature of the full theoretical roadmap.
