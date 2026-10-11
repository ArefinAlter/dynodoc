# Typed checkpoints v2

12 October 2026. This extends shared checkpoints with typed logical records and
chunked JSON string values. V1 objects, roots and event hashes are unchanged.
Schema 30 permits an explicitly selected v2 manifest. Periodic publication still
defaults to legacy; `DYNODOC_PERIODIC_CHECKPOINT_STORAGE=shared-v2` opts in, and
`engine-cli shared-snapshot DOCUMENT --through-seq N --typed` publishes explicitly.
Normal historical readers select either codec and verify its complete graph.
Publication compares reconstructed state with the pinned event-derived state in
the same transaction. Existing manifests cannot change codec at the same revision.

## Object contract

Typed bytes begin with `DYNOTYP\0` followed by byte `2`, then canonical UTF-8 JSON
(lexically sorted object keys, no insignificant whitespace). The SHA-256 domain
is `dynodoc.typed-checkpoint\0v2\0`, followed by all encoded bytes including the
prefix. Typed objects are limited to 32 KiB; unknown fields, duplicate keys and
noncanonical encodings are rejected. The null scalar vector is
`614afa1a326802d2432a51bb917cbff36c01c440f925efdd479eebec01ce6af0`.

The JSON envelope has `kind` and `value`. Kinds are scalar (null/bool/number),
text, array, dictionary, node, comment, suggestion, removed_choice, state and
asset. Text references contain `root` and exact UTF-8 byte count `bytes`; asset
content uses the same reference with uninterpreted bytes. Content roots use
[content v1](CHUNKED-CONTENT.md), including its fixed boundaries. Arrays and
dictionaries use the unchanged v1 radix map, with exact `count`; array keys are
16-digit lowercase hexadecimal ordinals. A state contains the four existing maps.
Node fields, suggestion details, comment bodies and rejection reasons are separate
references. Node identity remains independent of content address. Tombstones,
comments, suggestions, removed choices and JSON number/string values round-trip.
Standalone opaque asset manifests do not silently add assets to DocumentState;
native adapters still need explicit graph references and fidelity acceptance.

## Bounds and compatibility

One operation shares its object/encoded-byte budget across typed objects, index
objects and repeated content references. Default semantic reconstruction limits
are one million values/records, 256 MiB cumulative string/key/metadata bytes and
64 levels of JSON nesting. Property keys are at most 1024 bytes and record metadata
strings at most 4096 bytes. Checks precede growing semantic output; forged counts,
wrong UTF-8, wrong lengths/types, missing chunks and hash mismatches fail closed.
Temporary map indexes and the completed DocumentState remain resident. These are
finite limits, not constant-space materialization, a native-file size allowance,
or an exact RSS guarantee. Reads remain less efficiently batched than v1 and need
realistic backend profiling before any default rollout. Writes share the existing
64-object/1-MiB batch buffer across values and content chunks.

New metadata bounds can reject old unusual values. V1 remains readable and the
explicit operator command reports failure; the optional periodic writer retains
its existing transactional capacity fallback. Deployment must pair schema 30
with readers built for it; schema-29 binaries intentionally reject the newer
schema. Rollback needs a compatible schema-30 build, not a database downgrade.

Current validation and remaining adoption work are recorded in
[the handoff](ENGINE-EVOLUTION-HANDOFF.md). [Shared draft bases](SHARED-DRAFT-BASES.md)
now use this codec through a separate opt-in at schema 31. Shared named versions,
incremental changed-node publication and physical packing remain separate work.
