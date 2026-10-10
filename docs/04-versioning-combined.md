# Dynodoc document versioning, review and synchronization

Current protocol guide, 10 October 2026. This replaces the duplicated historical
volumes formerly in this file. The originals remain in [Volume I](02-versioning-design-vol1.md)
and the corrected [Volume II](03-versioning-mathematics-vol2.md). Their questionnaire
examples are historical; they do not define today's product or prove its code correct.

## Product contract

A team works on its own copies in Word/Docs, Excel/Sheets and PowerPoint/Slides.
Dynodoc connects those copies to a repository, records changes locally, transfers
missing changes/content, supports branches and reviewed merges, and applies checked
updates back into the native editors. An optional basic web editor remains.

See [PRODUCT-SPEC](PRODUCT-SPEC.md) for requirements and [decision 012](decisions/012-local-first-document-version-control.md)
for scope. Hundreds-of-MB files and large service deployments are targets requiring
acceptance evidence, not today's advertised capacity.

## Model: identity, content and history

| Concept | Contract |
|---|---|
| Logical node | Stable identity for paragraph/cell/shape/structural region, independent of text, position and formatting. |
| Formatting run | Adjacent text with equal normalized formatting within a logical node; runs can split/coalesce without replacing node identity. |
| Object address | Typed, versioned hash of canonical content/child references; changes when object bytes change. |
| Commit | Immutable repository identity, parents, root, operations/provenance and declared schema/capture capabilities. Planned beyond today's canonical sequence. |
| Branch/ref | An authorized name pointing at a commit, updated with compare-and-swap. Planned. |
| Binding | Verified connection from a native working copy to repository identity and an exact base; similarity is only a candidate signal. |

Text, formatting, structure and binary assets need separate representations and
capability declarations. Opaque content can be retained without promising semantic
merge. Unknown formatting and host inheritance cannot be discarded during a pull.
RTF and further formats require dedicated adapters and round-trip fixtures.

## Storage and synchronization target

Initial ingest scans relevant source bytes and establishes the graph. Optional
original binaries are stored once. Commit roots reference shared immutable chunks,
nodes, styles and assets; later commits add unique changed content and metadata.
There are many logical versions but no requirement to store many complete files.
The graph still contains document information and permits reconstruction; storing
changes/shared objects is not a claim that the server holds no document content.

Push negotiates missing objects, validates hashes/schema/reachability and atomically
publishes an authorized ref after all dependencies are durable. Pull verifies the
remote ancestry and objects, compares against the local base and current local
changes, previews conflicts, and applies with recovery and a final stale-local check.
Retries preserve commit/receipt identity. These complete semantics remain planned;
today's proposal bundles and local recovery queues provide only part of the contract.

## Merge correctness

Three-way comparison uses an ancestor, current target and contributor state. Stable
identity allows exact correspondence where IDs survive. Imported copies without IDs
need conservative matching and explicit binding. Repeated text and near-identical
files are ambiguous; name similarity never grants access or proves authorship.

Operations may auto-combine when their reads, writes and shared validation constraints
are independent, or when a more specific merge rule is justified. Different target
IDs alone are insufficient: crossed moves can create a cycle and separate nodes can
compete for a unique name. Supported formatting/text alignments must preserve native
invariants. Ambiguous changes remain conflicts with recorded resolutions.

See [D.2](03-versioning-mathematics-vol2.md#d2-operations-dependencies-and-conditional-commutativity)
for the sufficient-condition theorem. It is not a proof of arbitrary associativity,
commutativity, convergence, host fidelity or inferred human intent.

## Offline and provenance guarantees

Offline repositories retain commits and ancestry durably. Replicas converge on the
same accepted history, verified objects and resolutions after communication resumes.
This is distinct from automatic realtime CRDT merging of every native edit. Any CRDT
component needs a scoped operation model, causal/duplicate-delivery rules and tests.

Hashes establish integrity relative to a trusted commitment. Authenticated uploads
identify the uploader; editor observations, local timestamps and inferred changes
must retain their separate evidence status. Device signatures and independently
retained/witnessed heads are planned, with explicit key and trust assumptions.
No polling adapter can attest work performed while it was not observing.

## Performance contract and current implementation

Initial scan has at least linear input work. Incremental capture should avoid
rescanning untouched content when the host exposes changes. Streaming parsers,
chunked assets, persistent indexes, bounded diff/candidate search and backpressure
are necessary before raising file limits. Compare compressed size, expanded content,
unique changed bytes, memory and concurrent workload separately.

The current engine has centralized transactional semantic history, stable IDs,
checked rich-text deltas, three-way merging, review and portable proposal bundles.
Part 1 centralizes checkpoint-aware streamed historical reads and bounds browser
key-matching search/trace. Parts 2a-2c add [shared checkpoints](SHARED-CHECKPOINTS.md)
with full materialized-state roots, stable-key indexes, document-scoped objects,
mixed-format readers and bounded database batches. Shared writes remain opt-in;
publication still scans full state and holds a lock that blocks document appends.
Default JSON snapshots and full draft bases remain; their migration, large-asset
storage, commit DAG exchange, complete offline convergence and stronger independent
audit are pending. Native connectors still need installed-host acceptance and
structural/formatting/background capture.

Use [PROJECT-STATUS](PROJECT-STATUS.md) and [VERSIONING-CONFORMANCE](VERSIONING-CONFORMANCE.md)
for evidence. Use [ENGINE-EVOLUTION-PLAN](ENGINE-EVOLUTION-PLAN.md) for dependencies
and [ENGINE-EVOLUTION-HANDOFF](ENGINE-EVOLUTION-HANDOFF.md) to continue the current part.
