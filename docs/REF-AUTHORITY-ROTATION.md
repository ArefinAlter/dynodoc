# Receipt-authority rotation and compromise recovery

14 October 2026. Design and source contract for schema 41. It replaces the
"installed once, immutable, cannot be replaced" rule of
[the authority decision](RECEIPT-AUTHORITY-DECISION.md) with an append-only
**epoch chain**. Production has no receipt authority installed; this work exists
so that installing one is a recoverable decision. Nothing here is deployed yet.

## Vocabulary

| Term | Meaning |
|---|---|
| Authority | The installation UUID (`device_authority.id`). Stable for the life of the database. Receipts already carry it. |
| Epoch | A numbered, consecutive period in which exactly one Ed25519 key signs receipts. Epoch 1 is the key installed with `install-ref-authority`. |
| Statement | A signed record that epoch N is followed by epoch N+1. Two kinds: `rotation` (planned, dual-signed) and `recovery` (a recorded break). |
| Seal | A global marker in every statement: how many receipts existed, and a digest over their addresses, when the old epoch ended. |
| Revocation marker | An operator-recorded, unsigned note that an epoch's key must no longer be trusted, with a reason and an effective time. |

A key is used in exactly one epoch. Receipts keep the existing version-1 encoding
and hash; they identify their epoch through their `issuer_key`, never through a new
field. The existing receipt and signature vectors are unchanged.

## What is stored (migration 0041)

| Table | Rows | Rules |
|---|---|---|
| `project_ref_authority` (existing) | Zero or one. | Unchanged and immutable. An insert creates epoch 1; an existing row is backfilled to epoch 1 with the same key. |
| `project_ref_authority_epoch` | One per epoch, at most 1024. | `epoch` is the primary key, `public_key` is unique, `predecessor = epoch - 1` and is unique (no fork, no gap). The signed statement is stored verbatim with its address and seal. Never updated or deleted; the runtime login has SELECT only. |
| `project_ref_authority_revocation` | At most one per epoch. | Immutable. `effective_at` cannot be later than the time it is recorded. The runtime login has SELECT only. |

The active epoch is the highest one. A guard trigger refuses an epoch row unless its
stored statement names the same authority, predecessor key, successor key, kind and
seal as the row's columns, the seal count equals the stored receipt count, a
`rotation` does not follow a revoked epoch and a `recovery` does follow one. A second
trigger on `project_ref_event` refuses to store any receipt whose `issuer_key` is not
the active epoch's key, or whose active epoch is revoked, even if API code is
bypassed. Signature checks stay in the operator CLI and in clients: the database
cannot verify Ed25519, so it checks consistency, not authenticity.

Operator commands lock `project_ref_event` in SHARE ROW EXCLUSIVE mode while they
read the seal and write the epoch, so the seal cannot go stale: receipt writers wait,
and two operators serialise. Runtime logins never write these tables; installation
uses the migration login through `engine-cli`, like the original authority.

## Statement format

Canonical sorted-key JSON, hashed as `SHA-256("dynodoc.ref-authority-epoch\0v1\0" ||
canonical)` and signed over `"dynodoc.ref-authority-epoch-signature\0v1\0" ||
raw 32-byte address`. These domains differ from the receipt and the per-branch
rotation domains, so a signature cannot be replayed across kinds.

| Field | Rule |
|---|---|
| `version` | 1 |
| `kind` | `rotation` or `recovery` |
| `authority` | Installation UUID |
| `from_epoch`, `to_epoch` | `to_epoch = from_epoch + 1`, at most 1024 |
| `old_key`, `new_key` | 32-byte lowercase hex, different |
| `sealed` | `{receipts, digest}`; see below |
| `reason` | 0 to 200 bytes (required for `recovery`), no control characters |
| `declared_at_ms` | Operator clock; not trusted time |

The signed wrapper holds `transition`, `old_signature` (required for a `rotation`,
forbidden for a `recovery`) and `new_signature`. A `rotation` needs both keys. A
`recovery` carries only the new key's signature: it proves the new key exists and
that the operator chose this succession, and says nothing about the old key.

Seal digest: SHA-256 of `"dynodoc.ref-authority-seal\0v1\0"` followed by the raw
32-byte receipt addresses of every stored ref event, ordered by project UUID bytes,
branch UTF-8 bytes, then generation. It is cumulative across epochs and names no
scope, so it is an audit commitment rather than something a client can check per
branch (see the limits below).

## How receipts and old history stay verifiable

1. Every receipt is signed by the key of the epoch in which it was issued. Rotation
   never re-signs or rewrites a receipt; an old receipt stays individually valid.
2. The first receipt of a branch after a rotation has `previous` set to the last
   receipt of the old epoch, so the per-branch chain is unbroken. The API refuses to
   serve a branch whose stored receipts move back to an older epoch.
3. A client holding a pin at key K asks for the chain, verifies every statement
   signature, the authority UUID, epoch numbers and linkage locally, and then lets
   the existing pin logic accept the next receipt under the successor key. Receipts,
   statements and the pin change in one client transaction; an exact retry is a no-op.
4. A client that pinned the epoch-1 key long ago can still verify a whole history by
   walking the statements. Which key is "current" matters only through signatures
   from keys it already trusts.

## API (no new write endpoint)

All reads require `history:read` like the existing authority and receipt reads.

| Path | Response additions |
|---|---|
| GET `/projects/{id}/history/authority` | `epoch` (active), `revocation` (active epoch's marker or null), `genesis` (epoch 1 with its marker), `rotations[]` (epochs after `from_epoch`, default 1, `limit` 1-64) and `next_epoch`. |
| GET `/projects/{id}/history/authority/rotations` | The same chain as one bounded page: `from_epoch`, `limit`; items hold `epoch`, `kind`, `public_key`, `installed_at_ms`, the signed `statement` and that epoch's `revocation` marker. |
| GET `/projects/{id}/history/receipts` | `epoch` (active), `revocation` and `rotations[]` bridging the epochs seen in the page (optional `from_epoch`; at most 64). Items are unchanged. |
| POST `/projects/{id}/history/refs` | Signs with the key mounted for the active epoch. Refused with 403 when that epoch is revoked, 500 when the mounted key is not the active key. |

Startup: `DYNODOC_REF_SIGNING_KEY_FILE` must equal the active epoch's key or the
process exits. A revoked active epoch still starts (the rest of the service must
keep working) but issues no receipt until the operator installs a recovery epoch.

## Operator procedures

All of these use `MIGRATION_DATABASE_URL`; private keys are never read by the
database or the API container's database login. Key files may be encrypted
containers like the other signing commands.

**Planned rotation** (the old key is held and not suspected):

1. Stop receipt writers (the API). `engine-cli show-ref-authority --out state.json`
   prints the epochs and seal and saves the state that is signed.
2. Generate the new key as in the install procedure. On an offline machine:
   `engine-cli rotate-ref-authority --state state.json --old-key old.der --new-key new.der --out rotation.json`.
   The command refuses a wrong old key, a reused key and an existing output file.
3. `engine-cli install-ref-authority-rotation rotation.json`. An identical retry
   succeeds as a no-op; a stale, replayed or different statement is refused.
4. Mount `new.der`, start the API, run `engine-cli check-runtime`.
5. Publish the new public key and epoch through the channel clients already use for
   the authority key. A client that trusts the old key follows the statement itself.
6. Keep the old private key offline in escrow or destroy it once the clients you
   expect to follow have done so. A retained old key can still be abused.

**Authority loss** (the old private key is gone, nothing leaked):

1. `engine-cli revoke-ref-authority --reason "private key lost"` stops issuance.
2. Generate a new key. Offline:
   `rotate-ref-authority --recovery --state state.json --new-key new.der --reason "..." --out recovery.json`.
   Refreshing `state.json` after the revoke is required.
3. `engine-cli install-ref-authority-recovery recovery.json --confirm-break` records
   a `recovery` epoch. The break is visible to every client and is never converted
   into a rotation.
4. Every client re-pins out of band (below) before it accepts the new key.

**Compromise** (the key may be in someone else's hands):

1. Revoke at once, with the best-known time:
   `revoke-ref-authority --reason "..." --effective-at 2026-10-14T09:30:00Z`.
   The database and the API stop issuing immediately.
2. A dual-signed rotation is **not** available from a revoked epoch: whoever holds
   the key could have signed it. Continue with steps 2 to 4 of authority loss.
3. Tell clients through every other channel. Clients that reach the server see the
   marker, stop trusting that key and report whether their pinned receipt was
   declared at or after the stated time.
4. Rotating later does not undo a marker: it stays on the epoch it names.

**Client re-pin after a break** (explicit, per branch):

1. Obtain the new authority key from a source other than the server.
2. Either run the next sync against the same branch with
   `--accept-authority-break NEW_KEY`, or work offline: `engine-cli sync ... authority-proof --out proof.json`
   then `engine-cli receipt JOURNAL AUTHORITY PROJECT BRANCH accept-break proof.json NEW_KEY --confirm-break`.
3. The recorded statement must follow the pinned key and name `NEW_KEY`, the first
   receipt after the pin must be signed by `NEW_KEY` and extend the pin exactly, and
   later epochs must be unrevoked. The break statement is kept in the journal.
4. A branch with no new receipt keeps its old pin. A fresh journal cannot adopt a
   revoked epoch's head; it bootstraps once a receipt under the new key exists.
5. Update the `remote.json` key afterwards; until then `sync` reports that the
   configured key and the journal are separated by a break.

## Client rules (implemented in `engine-core` and `engine-cli`)

- Statements are verified locally: both signatures for a rotation, the new signature
  for a recovery, authority UUID, consecutive epochs, key linkage, no reused key.
- A trusted key reaches the active key only through rotations, with no recorded
  break and no revocation marker on any epoch from its own upward. Otherwise the
  command refuses and leaves the journal untouched, with one deliberate exception.
- When a marker covers the epoch of the key a journal currently trusts, `sync`
  records the existing local revocation. That pin accepts nothing implicitly again.
  Only the explicit re-pin above (which needs the independent key, the recorded
  statement and an extending receipt) can move it.
- The configured `remote.json` key must be on the served chain and not separated
  from the pin by a break.
- The server cannot make a client accept anything it would not otherwise accept.
  It can make a client refuse, as it always could by withholding data.

## Failure modes

| Situation | Result |
|---|---|
| Mounted key differs from the active epoch | API exits at startup; at runtime a mismatch returns 500 and stores nothing. |
| Rotation signed with the wrong old or new key, or by a different authority | Install refused; nothing stored. |
| Statement replayed after a later epoch exists, reordered, or a different statement for the same epoch | Refused. An exact retry of the active epoch's statement is accepted as a no-op. |
| Seal differs from the database (receipts issued after the state was exported) | Refused; export the state again with writers stopped. |
| Two operators at once | One wins under the table lock; the other sees the new epoch and fails. |
| API restarted with the old key after rotation | Fails at startup. |
| Rotation requested from a revoked epoch | Refused; only recovery. |
| Recovery without a marker | Refused; revoke first so the break has a visible cause. |
| Operator revokes by mistake | A marker cannot be undone; the only path is a recovery epoch and client re-pins. |
| Backup restored from before a rotation | Receipt writes fail by key mismatch; detectable, not silent. Receipts issued after the backup are lost with it. |
| Client offline across several rotations | Walks every statement (at most 1024) in pages of 64. |

## What rotation cannot fix

- A leaked key can mint receipts that verify until each client learns of the
  rotation or marker. There is no witness service, so a thief who also controls a
  client's network path can keep serving forks. Rotation shortens the exposure for
  honest clients; it does not recall forks already accepted.
- The seal is cumulative and global. A client cannot use it to reject an old-key
  receipt for a branch it has never seen. Operators and auditors can recompute it
  from the database and compare it with the signed statement.
- The marker and declared times are unsigned operator statements and declared
  clocks, not trusted timestamping. A marker can also be withheld by a hostile server.
- Rotation does not protect against a hostile database owner or a compromised
  migration login, which can still insert epoch rows. Clients refuse them only if
  signatures fail to verify, so a stolen old key plus owner access is a full bypass
  for clients that have not re-pinned.
- Loss of the new key before clients learn of it is another recovery.
- Devices, accounts and connector tokens are separate trust roots; revoking a
  receipt key does not revoke them.
- Branches with no activity after a break cannot be re-pinned on a receipt; they
  need a new journal once the new key has signed something.

## Evidence

Fixed independent vectors (OpenSSL Ed25519, Python JSON/SHA-256, public synthetic
seeds 7 to 10) are in `crates/engine-core/tests/fixtures/ref-authority-epoch-v1.json`;
existing receipt fixtures are unchanged. The restricted-role API and CLI tests cover
planned rotation with a second client following the chain, wrong keys, replay and
reordering, concurrent installs and an issuance race, startup mismatch through the
real binary, revocation blocking issuance in the API and the database, recovery
producing a break that is refused until re-pinned (through sync and through the
offline bundle), a backfilled pre-epoch installation and retired-key receipts being
refused.
