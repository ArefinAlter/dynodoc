# 023 — Node identities for linked document copies

Date: 2026-10-29. Accepted for the desktop 0.1 alpha.

The engine reserves node IDs globally within an engine, rather than within a document. The
original desktop synchronizer copied those IDs unchanged. Linking the same source to a second
project on the same server therefore failed with an ID collision.

Each new document pair now saves a bijection from local node IDs to remote node IDs. A new copy
gets fresh ULIDs; structural parent references use the same mapping. Content fields are left
unchanged. Existing nodes and their append-only histories are never renamed. Old links without
a mapping retain identical IDs for nodes already present on both sides.

Comparisons and conflict choices use the local namespace. Outbound operations translate back to
the destination IDs. New nodes extend the saved mapping before any content batch is sent. This
keeps independent links independent while preserving field merges, review requests and each
engine's own revision chain.

Creation saves the destination document ID and mapping before the first content batch. A restart
can finish an acknowledged or unacknowledged partial copy in that same document. Already written
nodes must still match the expected initial copy; otherwise the file is held for review. A failed
checkpoint prevents content writes. Document allocation itself is not transactional with local
state: a process failure between allocation and checkpoint can leave an empty unlinked document.

No engine schema or database migration is required. The additional link fields are stored in the
existing workspace state and included in its backup.

Validation covers two projects on one engine, multiple local copies, later edits, conflicts,
legacy mappings, lost acknowledgements, checkpoint failure and changed partial copies. The fake
engine enforces the engine-global uniqueness rule rather than accepting duplicate node IDs.
