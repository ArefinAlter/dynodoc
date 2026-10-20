# Remaining engine designs: index, dependencies, order and effort

> **Status: design, not implemented.** 14 October 2026. This index and the six
> documents it lists are design documentation only. They change no code, migration,
> hash, API or deployed behaviour, and nothing in them is a statement that a
> capability exists. Planned work must never be labelled shipped
> ([ENGINE-EVOLUTION-PLAN.md](ENGINE-EVOLUTION-PLAN.md), "Handoff contract").

## 1. Purpose and how to read this set

The status ledgers list a small set of engine and acceptance items as open after
the schema-40 release: commit pruning, applying merges across documents, resolving
real node conflicts automatically, external witnesses, large-file acceptance and
installed-host acceptance
([PROJECT-STATUS.md](PROJECT-STATUS.md) "Not done";
[ROADMAP-ACCEPTANCE.md](ROADMAP-ACCEPTANCE.md)). Each now has a design an engineer
can implement from and a product manager can reason about.

Every document in the set has the same shape:

| Section | Content |
|---|---|
| Problem, goals, non-goals, personas, success metrics | why and for whom |
| Current state against target | each capability tagged EXISTS / PARTIAL / MISSING with a repository path |
| Architecture, formats and invariants | ASCII diagrams, field-level specifications labelled "proposed" |
| API and CLI surface | commands, routes, scopes, error codes |
| Security and privacy | threat model, controls, residual risk, what is not claimed |
| Operations | deployment, upgrade, rollback, observability, failure modes |
| Delivery plan | phases, dependencies, effort in person-weeks with assumptions and exclusions, acceptance, test strategy, rollout, migration |
| Risks and numbered open questions | each with a recommended default and who decides |
| Appendices | glossary, references with dates; unverified facts say so |

Status vocabulary used throughout:

| Tag | Meaning |
|---|---|
| EXISTS | implemented in source at the cited path |
| PARTIAL | part exists; the gap is stated |
| MISSING | not implemented anywhere |
| Proposed | appears only in a design document |

## 2. Overview

| ID | Item | Document | Capability today | Effort (person-weeks) | Hard dependencies | Biggest open decision |
|---|---|---|---|---:|---|---|
| E-PRUNE | Commit pruning: signed history horizons, skeletons, policy language | [DESIGN-COMMIT-PRUNING.md](DESIGN-COMMIT-PRUNING.md) | PARTIAL: local staged-object collector; a read-only horizon planner exists (20 Oct 2026); no commit deletion | 25 - 33 | none locally; receipt authority installed for shared use | whether shared horizons are allowed before the object collector exists (CP-Q10) |
| E-XDOC | Cross-document and project-level merge: relocation lineage, rename-follow, split/join, constraint validation | [DESIGN-CROSS-DOCUMENT-MERGE.md](DESIGN-CROSS-DOCUMENT-MERGE.md) | PARTIAL: path planner, same-file convergence with v3 records; rename+edit not mergeable | 26 - 34 | E-NODERES phases N1-N2 (for P3) | relocation identifier scheme and audience labels (XD-Q1, XD-Q4) |
| E-NODERES | Automatic resolution of real node/field conflicts: strategies, invariants, policy as data, loser preservation | [DESIGN-NODE-CONFLICT-RESOLUTION.md](DESIGN-NODE-CONFLICT-RESOLUTION.md) | PARTIAL: equal/one-sided and non-overlapping rich-text merges; everything else conflicts | 27 - 37 | none (nested identifiers for N4b come from evolution part 4) | default gate for text (NC-Q1); shift inference versus stable column identifiers (NC-Q5) |
| E-WITNESS | Independent witnesses and a transparency log for signed heads and event-chain heads | [DESIGN-WITNESSES-AND-TRANSPARENCY.md](DESIGN-WITNESSES-AND-TRANSPARENCY.md) | PARTIAL: receipts and client pins; a pure Merkle log, checkpoint and verifier library exists (18 Oct 2026, unused); no anchoring or service | 19.5 - 26 | receipt authority installed for receipt leaves (event-chain anchoring has none) | log key custody and public versus private log (WT-Q1, WT-Q3) |
| E-LARGE | Large-file and service-scale acceptance: corpus, methodology, hypotheses, streaming ingest/egress, chunking decision | [DESIGN-LARGE-FILE-ACCEPTANCE.md](DESIGN-LARGE-FILE-ACCEPTANCE.md) | PARTIAL: streaming byte tree and bounded zip in the browser; 3,000,000-byte cap; full-state publication | 37 - 52 (of which 7 - 11 changed-node publication shared with the storage roadmap) | changed-node publication and windowed reads for H5-H8 | first rung of the limit ladder and where originals live (L-Q1, L-Q7) |
| E-HOSTS | Installed-host acceptance run plan (owner-run) | [DESIGN-INSTALLED-HOST-ACCEPTANCE.md](DESIGN-INSTALLED-HOST-ACCEPTANCE.md) | PARTIAL: worksheet, manifests validated, simulated bridges; **no real-host result** | owner 14 - 25 working days first pass (+ 12 - 20 reruns); engineering 9 - 13 automation + 4 - 8 defect placeholder | a PowerPoint-capable build and Mac for broad claims | iPad: test or exclude (IH-Q1) |

Totals if everything is built: roughly **150 - 205 person-weeks of engineering**
plus the owner's acceptance days. These are planning estimates with stated
assumptions in each document; they exclude user-interface authoring, security
review, legal review, operations procurement and defect fixing beyond the
placeholders named in the documents.

## 3. Dependency graph

### 3.1 Hard and soft dependencies

```
                    (owner decision: install receipt authority)
                                   |
        +--------------------------+------------------------------+
        |                                                         |
        v                                                         v
  E-PRUNE P5/P5b (shared service)                          E-WITNESS W1 ref-receipt leaves
        ^                                                         ^
        | soft: horizon records are leaves                        |
        +-------------------------- E-WITNESS ----- event-head anchoring (W5, no authority needed)

  E-NODERES N0-N1 --(strategy registry, evidence, v4 records)--> E-XDOC P0-P2
  E-NODERES N2-N3 (text, marks, lists, sheets, slides) -------> E-XDOC P3 (edit-follows-move)
  E-NODERES N5 (resolution-set hash, policy API) -------------> E-XDOC P6 (service)

  storage roadmap: changed-node publication, windowed reads --> E-LARGE L6 --> H5..H8
  E-LARGE L5 (server ingest) --> H9 egress --> soft --> E-HOSTS (opens in real hosts)

  E-HOSTS ................ independent of all engine items (connectors do not change)
  E-LARGE L2 (baseline) .. independent; informs every item that reads whole states
```

| Edge | Type | Why |
|---|---|---|
| Receipt authority -> E-PRUNE shared | hard | horizon records need a signer clients pin ([RECEIPT-AUTHORITY-DECISION.md](RECEIPT-AUTHORITY-DECISION.md)) |
| Receipt authority -> E-WITNESS receipt leaves | hard for that leaf type only | event-chain heads can be anchored without receipts |
| E-WITNESS -> E-PRUNE | soft | log makes out-of-policy truncation visible |
| E-NODERES -> E-XDOC | hard from XDOC P3 | edit-follows-move invokes the per-document resolvers |
| E-NODERES / E-XDOC / E-PRUNE share commit format v4 | coordination | one version bump, not three |
| Storage roadmap -> E-LARGE L6 | hard | steady-state cost must be independent of document size |
| E-LARGE -> E-XDOC, E-NODERES | soft | three resident states per document is a stated assumption of both |
| E-HOSTS -> E-LARGE H9 | soft | produced files must open in real hosts |

### 3.2 What can start now, independently

E-HOSTS (owner dry run on the web), E-LARGE L0-L2 (baseline measurement of the
current system), E-NODERES N0-N1 (no-op framework), E-PRUNE P0-P1 (specification
and read-only local planner), E-WITNESS W0 (specification and vectors).

## 4. Ordering

Waves group work by dependency and risk. Within a wave, items are independent.

| Wave | Contents | Why here | Evidence produced |
|---|---|---|---|
| 0 | E-HOSTS H0-H1 (web dry run); E-LARGE L0-L2 (baseline report); E-NODERES N0-N1; E-PRUNE P0-P1; E-WITNESS W0 | cheap; produce specs, vectors and measurements; nothing ships behind a format change | first real-host results; first baseline report; frozen vectors |
| 1 | E-NODERES N2-N3; E-PRUNE P2-P3 (local apply, archive); E-WITNESS W1-W2 (log core, client verifier); E-LARGE L3-L4 (chunk experiment, resumable upload); E-HOSTS H2-H5 | delivers user-visible value locally; independent of the receipt-authority decision | strategy laws and corpus results; crash-injection results; chunking decision |
| 2 | E-XDOC P1-P3 (plan_v2, relocations, edit-follows-move); E-NODERES N4-N5; E-PRUNE P4 (sync); E-WITNESS W3-W5; E-LARGE L5-L6 (server ingest, changed-node publication) | needs wave-1 foundations; first format v4 writers | model-based merge corpus; witness interoperability; H1-H5 results |
| 3 | E-PRUNE P5/P5b (shared service, object collector); E-XDOC P4-P7; E-WITNESS W6-W7; E-LARGE L7-L9 (egress, service-scale, ladder); E-HOSTS H6-H7 (freeze, submission) | service-side and release hardening; needs the authority decision and a production-copy rehearsal | gate reports; frozen host evidence |

Rules for sequencing decisions:

1. **No writer ships before every reader.** Format changes (v4 commits, schema 4
   local repositories, journal v2) first ship readers that fail closed, then
   writers, as v2 and v3 commits did.
2. **Measure before limits move.** E-LARGE's baseline precedes any upload-limit
   change; every rung needs its report.
3. **Receipt authority decision gates shared pruning and receipt leaves**, not
   local work or event-chain anchoring.
4. **Host acceptance gates claims, not code.** Listing text equals the passed set.
5. **One format version bump.** E-XDOC owns the v4 specification; E-NODERES and
   E-PRUNE conform to it (section 5).

### 4.1 Critical path

For a single engineer the longest chain is
E-NODERES (N0-N3) -> E-XDOC (P1-P3) -> E-XDOC P6 service, about 37 - 48 person-weeks,
with E-LARGE L6 in parallel for a second engineer. With two engineers all but
E-LARGE L5-L9 and E-PRUNE P5b can be done in roughly 12 - 16 months; with three,
about 9 - 12 months. These are not commitments (confidence is stated per phase in
each document and is mostly "medium" or "medium-low").

## 5. Cross-cutting concerns

### 5.1 Commit format versions

| Version | Status | Added by |
|---|---|---|
| v1 | EXISTS | plain project commit |
| v2 | EXISTS | recovery source reference |
| v3 | EXISTS | `same_file_resolutions` (strategy `dynodoc.same-file.node-field.v1`) |
| v4 | Proposed | `project_resolutions`, `relocations`, embedded merge policy (E-XDOC P0 owns; E-NODERES strategy ids and policy hash; E-PRUNE adds pin rules, no new fields) |

Importers reject unknown strategy ids today (`convergence::STRATEGY` equality in
`Commit::validate`), so any new strategy id needs a version gate. v1-v3 addresses,
signatures and vectors never change.

### 5.2 Storage and schema

| Item | Change | Notes |
|---|---|---|
| Local SQLite | schema 4 (horizon, skeleton, released-state tables) | readers first; schema-3-only clients refuse schema 4 |
| PostgreSQL | migrations for horizons/skeleton/guards, transparency outbox/checkpoints, upload sessions; number assigned at implementation (0041 is the next free number at the time of writing) | applied migrations are never edited |
| Device-key scopes | `history:retention` (and possibly `history:prune-approve`) widen the check in `migrations/0040_device_key_project_grants.sql` | same pattern as `history:review` |
| Receipt journal | version 2 adds checkpoints, inclusions, policies, horizons | additive |
| Policy files | `dynodoc.retention-policy.v1`, `dynodoc.merge-policy.v1`, witness policy JSON | canonical JSON, hashed, versioned |

### 5.3 Keys and custody

| Key | Used for | Where defined | Custody guidance |
|---|---|---|---|
| Receipt authority key | branch receipts; countersigns horizons | existing | offline copies, named custodian ([decision](RECEIPT-AUTHORITY-DECISION.md)) |
| Log signing key | transparency checkpoints | E-WITNESS | separate from the authority key; mounted separately |
| Witness keys | cosignatures | E-WITNESS | operated by independent parties |
| Pruner (owner) device keys | horizon signatures | E-PRUNE | existing device key model with a new scope |
| `k_project` | blinds transparency leaf tags | E-WITNESS | derived server-side; delivered to `history:read` principals |
| CDC seed | per-document chunk boundaries | E-LARGE | stored with the document |

### 5.4 Testing infrastructure shared by the set

| Asset | Used by |
|---|---|
| Independent implementations for vectors (Python/OpenSSL style as `project-merge-v3.json`) | all format additions |
| Property-test harness with law checks (`proptest` is a workspace dependency) | E-NODERES, E-XDOC, E-PRUNE |
| Synthetic corpus generator and host-mimic mode | E-LARGE, E-XDOC F6, E-NODERES |
| Crash-injection/failpoint hook | E-PRUNE, E-WITNESS, E-LARGE |
| Real-router tests under the restricted runtime login | every service-side phase |
| Evidence recorder and result schema | E-HOSTS, E-LARGE reports |

### 5.5 Interactions between items

| A | B | Interaction |
|---|---|---|
| E-PRUNE | E-XDOC | v4 relocation and resolution records reference state roots; closure rule pins them (`merge_inputs`) |
| E-PRUNE | E-WITNESS | horizon records as leaves; client journal remembers latest horizon |
| E-NODERES | E-XDOC | resolver framework is called per document; policy hash and strategy ids recorded in v4 |
| E-NODERES | E-LARGE | strategies must be bounded; large documents need streaming merges |
| E-XDOC | E-LARGE | project-level constraint checks need node-identifier indexes at scale |
| E-LARGE | E-PRUNE | content horizons reclaim bulky assets; chunk sharing defines what is releasable |
| E-HOSTS | all | connectors capture existing paragraph, cell and shape text only; none of these designs changes the connector contract |

### 5.6 The three ideas documented separately (not built)

The self-hosted edition, AI-agent connectors and end-to-end encryption are
documented in their own design documents on the same date. Where they constrain
this set the constraint is recorded in the relevant section: end-to-end encryption
means server-side planning, merge and ingest would have to run on clients or inside
a trust boundary (see section 7.8 of the pruning design and the large-file design's
ingest options); self-hosted installations are first-class witness and log
operators; AI agents acting through connectors must go through the same
review-gated merge paths and policy floors (E-NODERES gate levels apply equally to
agent-authored changes).

## 6. Owner decisions needed

| # | Decision | Needed before | Recommended default | Source |
|---|---|---|---|---|
| 1 | Install the receipt authority in production, with custody and rotation story | shared pruning; receipt leaves | install after rotation/recovery design and rehearsal; start event-chain anchoring first | [RECEIPT-AUTHORITY-DECISION.md](RECEIPT-AUTHORITY-DECISION.md) |
| 2 | Is a "release access, keep bytes" shared horizon acceptable before an object collector exists? | E-PRUNE P5 | yes, labelled | CP-Q10 |
| 3 | Default gate for automatic text merges | E-NODERES N2 | auto-with-review | NC-Q1 |
| 4 | Audience labels (classification) as project policy | E-XDOC P4 | file rules, strengthen-only | XD-Q4 |
| 5 | First upload-limit rung and the original-file store | E-LARGE L4 | 25 MB; content tree via the existing store trait | L-Q1, L-Q7 |
| 6 | Public or private transparency log; one Verne witness plus partners | E-WITNESS W1 | public checkpoint, private tiles | WT-Q1, WT-Q4 |
| 7 | Which platforms to list; iPad stance; minimum builds | E-HOSTS H1 | claim only what passes; decide iPad by manifest mechanism | IH-Q1, IH-Q2 |
| 8 | Whether review decisions may ever be pruned | E-PRUNE P5 | no | CP-Q4 |

## 7. Consolidated open questions (top items)

Full numbered lists, each with a recommended default and decider, are in each
document's section "Risks and open questions".

| Rank | Question | Doc |
|---:|---|---|
| 1 | Shared horizons: authority signer and two-person rule; behaviour before an object collector exists (CP-Q1, CP-Q2, CP-Q10) | pruning |
| 2 | Relocation identifier scheme, deterministic derivation and audience boundary handling (XD-Q1, XD-Q4, XD-Q7) | cross-document merge |
| 3 | How far automatic text merging may go by default, and who may loosen policy (NC-Q1, NC-Q3, NC-Q9) | node conflict resolution |
| 4 | Log key custody, public versus private log, number and independence of witnesses (WT-Q1, WT-Q3, WT-Q4) | witnesses |
| 5 | Limit ladder start, ingest location, original-binary store, CDC adoption (L-Q1, L-Q2, L-Q5, L-Q7) | large files |
| 6 | iPad and minimum-build claims (IH-Q1, IH-Q2) | installed hosts |

## 8. Verification ledger for external facts

External statements in the set were checked on 14 October 2026 against the sources
listed in each document's appendix. Facts that could not be verified are marked
"unverified: check at build time" in place. Aggregate:

| Verified (source read) | Unverified (check at build time) |
|---|---|
| C2SP signed-note, tlog-checkpoint, tlog-cosignature, tlog-tiles formats; tlog-witness protocol (editor's copy) | stable tlog-witness revision text; Sigsum witness details beyond the design page; Go checksum database specifics |
| RFC 9162 status (Experimental) and Merkle consistency proof sections | Git shallow-file mechanics for fetch/push |
| UAX #29 Revision 49 (Unicode 18.0.0), GB9c | ProseMirror mark `inclusive` current wording; Excel reference-rewrite rules at range boundaries |
| FastCDC abstract claims (about 10x vs Rabin-based, about 3x vs Gear/AE-based); restic chunk parameters; prolly-tree description (search summary) | PostgreSQL `bytea` size/TOAST behaviour at hundreds of MB; OOXML XSD availability; zip data-descriptor acceptance in all six hosts |
| tus 1.0.0 protocol | Playwright attach to Office's WebView2 over the DevTools protocol; manifest mechanism to exclude iPad |
| Microsoft requirement-set tables (Word 1.3, Excel 1.13, PowerPoint 1.4), webview table, certification policy 1120.3, testing and debug pages | macOS support policy for Office for Mac; Microsoft 365 developer sandbox availability |
| Peritext, Eg-walker, move-operation papers (existence, venue); diff3 paper | Google add-on availability on mobile editors |

## 9. Maintenance of this set

- When any item starts, record in the handoff what changed in both repositories,
  tests actually run, unresolved failures and the next concrete task
  ([ENGINE-EVOLUTION-HANDOFF.md](ENGINE-EVOLUTION-HANDOFF.md)).
- When a phase completes, change the capability tags in that document's section 2
  and in the overview table above; do not delete the design text.
- A phase's acceptance evidence is a file under `docs/benchmarks/` or
  `docs/operations/`, not a sentence.
- Re-verify external facts before building; dates in each appendix show when they
  were last read.
