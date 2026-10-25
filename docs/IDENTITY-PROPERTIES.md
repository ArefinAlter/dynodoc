# Engine node identity properties

Two reproducible properties run 1,000 generated examples each with the fixed
`IDENTITY` seed. They use the read-only structural diagnostic checker
`integrity::check_invariants` (`crates/engine-core/src/integrity/invariants.rs`).

Independent generated draft histories fork a generated base. Both merge
orientations must keep map keys equal to node IDs, retain base identities and
invent no identity absent from the inputs. Nodes unchanged on both sides keep
their fields. Conflict-free plans pass structural checks; replay preserves every live or previously canonical identity and key/ID equality.
A private create/delete pair that was never shared intentionally needs no
canonical event; a named regression preserves this limit. This identity assertion does not replace
the existing full-state merge replay properties.

The split/join model appends delete plus fresh-create operations. It tombstones
the original paragraph, creates two fresh identities, then tombstones those and
creates a fresh joined paragraph. Bengali text survives exactly, unaffected
neighbors remain identical, and every tombstone keeps its original identity.
These are engine operations, not native editor split/join or nested editor IDs.

The tests use bounded in-memory histories and no production documents. They
model concurrent drafts, not thread scheduling, network delivery or database
transaction interleavings. Existing protocol, convergence and replay suites
continue to cover their separate contracts.
