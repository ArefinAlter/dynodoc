# Decision 003 — Formatting-aware rich-text version control

Date: 21 September 2026. Status: implemented first increment; see the research and
conformance documents for the remaining work. This explicitly extends the original
questionnaire PoC in response to the product owner's rich-text versioning request.

Use stable structural nodes plus adjacent formatting runs. A run is a maximal
contiguous sequence with identical stored text metadata **within one container**.
Style equality does not establish document identity. Paragraphs in different
locations never become one block just because they share a font.

Keep the existing append-only event chain and PostgreSQL authority. Add versioned,
base-checked `RichTextPatched` events with retain/delete/insert operations and a
per-event style dictionary. Use them only when smaller than the full-field event
and exact replay reproduces the requested editor JSON. Legacy events remain valid;
do not rewrite old hashes or migrate the historical event payloads.

The workspace merger may combine disjoint text edits and independent formatting
properties in a paragraph, heading or code block. Ambiguous boundaries, competing
values, deletion versus formatting, and unsupported structural overlaps require
review. Do not claim CRDT convergence from this three-way merge implementation.

Represent document structure and provenance as a permissioned, derived view of
nodes and events. No separate graph database, embedding model or language model
is required to classify formatting runs. Rich-text offset units are Unicode scalar
values; editor selection offsets remain UTF-16 and must be converted at adapters.

Retain ProseMirror JSON as the materialized/editor representation. This increment
reduces history payloads; it does not yet deduplicate complete snapshots or transmit
browser edits as compact patches. Those are separate, measurable follow-on changes.

Research, implementation map, performance evidence and release requirements:
[rich-text versioning research](../RICH-TEXT-VERSIONING-RESEARCH.md).
