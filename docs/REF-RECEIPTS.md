# Signed branch receipts and client pins

12 October 2026. The core provides versioned Ed25519 branch-head receipts,
contiguous proof verification, dual-signed authority rotation, and a separate
SQLite client trust journal. Schema 36 now [issues these receipts atomically with shared branch updates](SHARED-PROJECT-HISTORY.md),
using an operator-installed public binding and separately mounted signing key.
[Selected-history CLI sync](SELECTED-HISTORY-SYNC.md) verifies contiguous proofs
and records remote heads in this separate journal; no key is silently trusted
from the server authority endpoint.
Schema 41 adds server-side [authority rotation and compromise recovery](REF-AUTHORITY-ROTATION.md):
an append-only epoch chain, signed succession statements and revocation markers
that clients follow or refuse. The production schema-29 pilot has not received
this source update, and no authority is installed there. Authority distribution
and external witnesses remain open.

A receipt binds a stable authority UUID, project UUID, branch, commit address,
ref generation, previous receipt address, declared issuance time and signing
public key. SHA-256 addresses hash `dynodoc.ref-receipt\0v1\0` followed by
canonical sorted-key JSON. Ed25519 signs `dynodoc.ref-receipt-signature\0v1\0`
followed by the raw 32-byte receipt address. The canonical envelope limit is
4096 bytes. Generation one has no predecessor; subsequent generations require
one. Generations count accepted ref updates, including a move back to an older
commit; receipt continuity alone does not imply a fast-forward-only branch policy.
The current hosted shared-ref service independently enforces fast-forward updates.

The independent fixture [ref-receipt-v1.json](../crates/engine-core/tests/fixtures/ref-receipt-v1.json)
pins canonical bytes and address
`b644944d962d84af0b90e48111c3fa708897a58198637ef464ad27712eaae57f`.
Its all-zero synthetic public-key field tests encoding, not a signature.
Existing event, checkpoint and project-commit domains/vectors are unchanged.
The separate [signature fixture](../crates/engine-core/tests/fixtures/ref-receipt-signatures-v1.json)
uses independent Python JSON/SHA-256 and OpenSSL Ed25519 to pin both receipt
signatures and both rotation signatures. Its keys use publicly known synthetic
seeds. Never regenerate these fixtures to make an implementation change pass.

The client explicitly chooses an authority key through an independently trusted
channel. Initial bootstrap accepts a checkpoint; it cannot prove unseen earlier
history or discover an already hidden fork. Subsequent advances require an exact
chain from the pin, one generation at a time, with the same scope and trusted key.
Proofs contain at most 256 receipts. A retried proof may include a verified earlier
prefix that connects exactly to the saved pin. Rollback, a different receipt at
the same generation, a skipped/reordered update, an unapproved key or substituted
scope fails without changing trust state.

Rotation binds both public keys, scope, the exact pinned receipt and next
generation under separate rotation hash/signature domains. Both keys sign. The
next receipt must use the new key and extend that pin. Accepted rotations are
stored with receipts in the same transaction. Exact lost-acknowledgement retries
are idempotent. Local revocation persists across restart and rejects every later
advance or rotation, including retries; bootstrap cannot overwrite that pin.
Recovery after revocation requires an explicit, independently trusted new
bootstrap in a new journal, or, when the server recorded a recovery epoch,
`accept-break` with the independently obtained key (the only operation that
works on a revoked pin, because it needs that key, the recorded statement and a
receipt extending the pin). This revokes local receipt trust, not a remote login,
connector token or account's device key.

`ReceiptJournal` uses a separate SQLite database with WAL/FULL durability. It
refuses a project-repository database. A writer reservation precedes reading the
pin; proof/rotation rows and pin changes commit atomically. Receipt and rotation
rows are immutable. Concurrent conflicting advances have one winner. This client
filesystem, its authority-key choice and backups remain trusted: someone who can
rewrite or roll back the client's journal can destroy its remembered evidence.
Verifying a receipt does not download its commit or prove that every referenced
object remains available; verified object/commit import is a separate requirement.

CLI shape (no network or PostgreSQL connection):

```
engine-cli receipt JOURNAL AUTHORITY_UUID PROJECT_UUID BRANCH ACTION
```

Actions:

- `verify RECEIPT TRUSTED_PUBLIC_KEY`: check the explicitly selected scope/key.
- `trust RECEIPT TRUSTED_PUBLIC_KEY`: first checkpoint for this journal scope.
- `advance PROOF_JSON`: atomically accept a contiguous receipt array, or an
  authority proof saved by `sync ... authority-proof` that also carries the epoch
  statements. Dual-signed succession between epochs is followed; a recorded break
  or a revocation marker is refused.
- `accept-break PROOF_JSON NEW_KEY --confirm-break`: explicit re-pin across a
  recorded `recovery` statement, with the new key supplied independently of the
  server. The first receipt by that key must extend the pin; the statement is kept.
- `authority`: list the authority statements (rotations and breaks) this journal
  accepted.
- `show`: display the saved pin and revocation state.
- `revoke`: permanently revoke this journal scope.
- `sign MANIFEST ED25519_PKCS8_KEY`: sign an operator-supplied manifest.
- `sign-rotation OLD_PKCS8_KEY NEW_PKCS8_KEY`: create the dual-signed transition
  from the current pin.
- `rotate TRANSITION NEXT_RECEIPT`: atomically accept a rotation and next head.

## Server authority epochs

A receipt names its signer through `issuer_key`; a key belongs to exactly one
numbered epoch and the receipt encoding is unchanged. A planned rotation is a
statement signed by both the old and the new key over a domain-separated message
holding the authority UUID, both epoch numbers, both keys and a global seal of the
receipts issued so far. A key loss or compromise is a `recovery` statement signed
only by the new key after an operator-recorded revocation marker. Clients keep
verifying old receipts by walking the statements from the key they pinned, accept a
dual-signed rotation inside the same transaction as the next receipt, refuse a
recorded break, and treat a marker on the pinned key's epoch as a local
revocation. The per-branch `rotate` action above remains the offline primitive for
a single scope; server rotations do not use it because one operator signature
cannot cover every branch.

Signing tools never imply remote acceptance or publish a branch. Private keys
are read from caller-selected unencrypted DER Ed25519 PKCS#8 v1 or v2 files,
never printed or stored in the journal. V1 imports derive the public key from
the seed; v2 imports also check that the included public key matches. Standard
OpenSSL v1 imports are pinned against the independent signature fixture.
Inputs are bounded (8192 bytes per signed envelope, 2 MiB per proof file, 4096
bytes per key file). Public keys and signatures are intentionally exportable.

Account-bound device enrollment/revocation exists in source (see
[device signing keys](DEVICE-SIGNING-KEYS.md)) and receipts are issued atomically
with accepted protected-ref updates. Server rotation and revocation now exist (see
[the rotation design](REF-AUTHORITY-ROTATION.md)); clients still obtain the first
authority key independently, since a server-reported key is never auto-trusted.
Independent witness storage/client gossip must detect mutually consistent forks
shown to different clients. A server with
its signing key can still equivocate; isolated clients cannot discover a fork
they have never observed. Declared time is not trusted timestamping. No existing
database-owner rewrite protection or installed-editor acceptance is claimed from
these primitives alone.

The [local synthetic demonstration](benchmarks/receipt-journal-20261012.json)
creates real local project commits and a separate client journal, rejects a
signed conflicting head and rollback, accepts a dual-signed rotation, reopens
the journal and confirms revocation rejects rotation retries. Run it with:

```
cargo run --locked -p engine-core --example receipt_journal_demo -- NEW_DIRECTORY
```

It uses public fixture keys, empty project file maps and local storage. It does
not measure network/service capacity or certify an independent witness.
