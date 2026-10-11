# Shared draft bases

12 October 2026. Schema 31 adds document-scoped state-root references for private
drafts, pushed change requests, historical recovery drafts and subsequent merge/
rebase bases. `DYNODOC_SHARED_DRAFT_BASES=shared-v2` opts new writes into this path.
The default remains legacy JSON and all existing bases remain readable.

Each referenced base is a small JSON envelope `{"$dynodoc_state":2,"root":"…"}`
inside the existing base-state column. The root reconstructs a typed v2 state,
including comments, suggestions and tombstones. Creation writes objects, verifies
exact reconstruction and stores the reference in the same authorized transaction.
Identical drafts reuse the graph. Rebase records a new root; initial bases are
immutable. Draft edits and canonical event logs retain their existing contracts.

Generated reference columns and deferred foreign keys enforce document scope and
reachability. Strict envelope constraints reject missing/malformed references.
Existing audited erasure can remove drafts and objects in its established order;
foreign keys are checked at commit. Reference readers check all reachable hashes,
types and semantic budgets and fail closed; corruption never falls back to another
base. The API hydrates draft details to retain the previous response shape and
keeps existing permission, stale-revision, review and selection checks.

This reduces repeated **stored** bases. Current-state reconstruction, diff and
merge still retain full states. No physical packing, garbage collector or hard
total-history quota is added. [Named/deployed snapshots](SHARED-NAMED-SNAPSHOTS.md)
have a separate schema-32 opt-in writer. Readers built before schema 31 are incompatible
with the newer schema, even if this optional writer is disabled. Production is
still schema 29; neither migration nor flag has been deployed by this work.

The same pass replaces repeated parent-chain walks inside diff sorting with an
iterative cached depth index. Its ordering preparation uses O(N log N) time and
O(N) auxiliary metadata with BTree collections; it preserves the old depth cap
for cycles/missing parents. Regression evidence includes 100 synthetic cyclic/
incomplete graphs and a 20,000-level chain. This bound applies to ordering, not
all text diffing, full-state cloning or end-to-end merge latency.
