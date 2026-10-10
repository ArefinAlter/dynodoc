# Dynodoc product specification

Owner direction: 10 October 2026. This is the current product specification.
Decision [012](decisions/012-local-first-document-version-control.md) supersedes
the research-instrument product scope. Questionnaire support remains an adapter;
public consultation, civic identity and AI assessment are separate proposals.

## The user journey

1. An owner creates a private project/repository and connects a document on their
   device or in an existing editor. Dynodoc scans its structure, content, formatting
   and assets, assigns persistent identities and creates an initial commit.
2. Invited collaborators connect their own copies. Embedded identity and verified
   ancestry provide strong matching; name/content similarity suggests candidates.
   Access and an explicit, verified binding are required before attaching history.
3. People edit in Word/Docs, Excel/Sheets, PowerPoint/Slides or supported local
   formats. A connector records supported changes durably on the device, including
   offline work. Unobserved intervals must be disclosed when recovered by a scan.
4. Commit records a local version. Push transfers missing commits and changed
   objects. A change request selects work for review. Pull retrieves missing objects
   and previews a checked application to the local document. A merge records its
   parents, resolutions and resulting state without discarding either contributor.
5. Reviewers see content, formatting and structural differences, attribution and
   explicit conflicts. They can restore earlier content through a new commit.
   Interrupted transfer/application can be retried or recovered without losing work.

The web product provides repositories, branches, requests, comparisons, history,
permissions and synchronization. Existing basic editors remain available.
Native editor parity is not the product roadmap.

## Identity, formatting and format fidelity

The graph has stable logical nodes (paragraphs, table cells, worksheet rows/cells,
slides/shapes and asset references). Text within a node is represented as maximal
adjacent runs with equal normalized formatting. Equal-looking runs in unrelated
places do not become one node. A formatting edit may split/coalesce runs without
changing logical identity. Explicit formatting, inherited styles and host-specific
properties must be distinguished; visually similar rendering is not byte equality.

Run paths/offsets are valid only against their checked base. Durable nested identity,
split/join ancestry and move semantics need a versioned contract. File-name/content
matching never establishes authorship or authorizes access. Ambiguous bindings or
unsupported structures require review and retain the source for recovery.

DOCX, XLSX and PPTX plus the six native Microsoft/Google editors are primary.
RTF is an explicit next format adapter, not currently certified. Further formats
require a capability matrix and round-trip corpus. Byte-exact opaque preservation
and semantic cross-format equivalence are different guarantees. Unknown properties,
assets, macros and embedded objects must survive unchanged where promised; an
unsupported semantic edit must fail explicitly rather than silently lose content.
Preserving a formula is not a guarantee that two native calculation engines agree.

## Storage and transfer contract

The target is one optional original binary plus immutable content-addressed objects,
assets, commit manifests and change records. A commit references unchanged objects;
it does not create a complete document copy. Changed chunks are new objects. Large
assets are independently addressed and transferred only when missing. A typed hash
domain and canonical serialization version are part of every object contract.

The normalized initial graph contains document content. A server that reconstructs
and reviews versions therefore stores information sufficient to reconstruct them;
"only changes" does not mean "no document content on the server." An optional
client-encrypted repository mode would need a separate search/review/key-sharing
design. It is not implied by compact storage. Tenant isolation applies to object
existence checks, deduplication, authorization and eventual garbage collection.

Logical versions must exist. The requirement is to avoid redundant full-file
versions, not to discard history. Storage grows with unique changed content and
metadata, retained assets and any optional original; highly rewritten content still
costs space. Retention and explicit audited erasure must cover all reachable copies.

## Correctness contract

- The same verified commit, schema version and reachable objects materialize the
  same state. Preserve existing canonical events and their readers during migration.
- Push is resumable and idempotent. Publishing a commit/ref happens only after all
  required objects are durable and validated. Ref updates use compare-and-swap.
- Local work survives restart/offline periods. Pull detects local edits made after
  preview and preserves recovery state before changing a native file.
- Automatic merges require compatible read/write/constraint footprints and format
  validation. Independent target IDs alone are insufficient. Ambiguous edits remain
  explicit conflicts; resolutions are durable operations/commits.
- Offline replicas converge on the same accepted commit graph and resolutions.
  This does not promise intention-preserving automatic merge of every operation.
  A realtime CRDT is an optional implementation component with its own proof/test
  obligations, not a substitute for review or branch ancestry.
- Hash integrity, authenticated upload identity, device signatures, observed editor
  activity and independently witnessed timestamps are separate evidence. No claim
  of complete human/AI authorship from polling or a server hash chain alone.

## Size and scale objectives

Hundreds-of-MB files, ten collaborators per large-document acceptance fixture and
hundreds of thousands of registered users are target workloads, not measured
capacity today. Test 100/250/500 MB compressed files with separately reported
expanded XML/text/media sizes, zip expansion limits and realistic edits.

Initial ingest must inspect relevant source bytes: at least O(input size) work.
Memory should depend on bounded parser buffers, active objects and indexes, not
the complete expanded file. Subsequent observed edits should cost proportional to
changed content and affected index/graph paths, with disclosed rescan fallbacks.
Chunking/batching limits must prevent a single document from exhausting a worker.

Capacity reports must state concurrent active users, documents, edits/second,
object sizes, history length, cache state, p50/p95/p99 latency, peak memory and
storage/network amplification. A registration count or paragraph microbenchmark
does not demonstrate service capacity. See [the staged plan](ENGINE-EVOLUTION-PLAN.md).

## Current boundary

Today: centralized event history, checked rich-text patches, drafts/review,
portable proposal bundles and development connectors for existing text/cell fields.
Full JSON snapshots, native acceptance gaps and pane-dependent capture remain.
The target above becomes implemented only with the evidence in
[project status](PROJECT-STATUS.md) and [the current handoff](ENGINE-EVOLUTION-HANDOFF.md).
