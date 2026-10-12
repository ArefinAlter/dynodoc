# Account-bound device signing keys

12 October 2026. Source schema 33; not yet deployed to the schema-29 pilot.

The portable project commit signature can now be checked against an authenticated
account's active signing key. Enrollment proves possession of an Ed25519 private
key using a five-minute, one-use challenge bound to the account, installation,
public key, random nonce, challenge ID and expiry. This is an account credential
binding, not a project permission, branch update, trusted timestamp or external
audit witness. Remote project exchange and protected publication remain open.

## User workflow

Open your own Profile → Signing keys. Obtain the public key from an existing local
unencrypted DER Ed25519 PKCS#8 key with:

```text
engine-cli device public-key signing-key.der
```

Enter the public key, download the challenge, and run the `device prove` command
shown in your profile. The command requires the expected installation and account
IDs, checks the key matches, and writes a new proof file without overwriting an
existing file. Select that proof JSON in the profile to finish enrollment. The
private key stays local. Do not upload a private key. `engine-cli device keygen
--out FILE` creates a new key (exclusive file, mode 0600 on Unix) and prints its
public key. Secure native-host key storage is not implemented. An enrolled key can
also sign in to one project at a time for short-lived `sync` tokens: see
[project-scoped device sessions](PROJECT-SCOPED-DEVICE-SESSIONS.md).

Revoke keys from the same profile. Revocation is permanent, including after a
lost-response retry or account restoration. Reconnect using a new key. Existing
commit signatures remain independently verifiable; a revoked key cannot satisfy
the service's current-account signing check. The current Office/Google connector
grants still use their existing document-scoped pairing flow.

## HTTP contract

All routes require a current normal account session. Document connector grants
do not authorize them; the restricted connector gateway remains unchanged.

| Route | Behavior |
|---|---|
| `GET /devices` | Own installation/account IDs and at most 128 own public keys with registration/revocation dates. No other-account directory or proof bodies. |
| `POST /devices/challenge` | Accepts `{public_key}`; returns a challenge valid for five minutes. |
| `POST /devices` | Accepts `{challenge, signature}`. Atomically consumes the exact stored challenge, inserts an immutable key binding and writes an audit event. An exact active retry returns the same key. |
| `POST /devices/{id}/revoke` | Permanently revokes an own key with one audit record; repeated requests are idempotent. Another account's ID returns 404. |
| `POST /devices/verify-commit` | Checks a portable commit signature against an active key of the signed-in account. Returns its address and `published:false`. Does not store content, check project permission or publish a branch. |

Enrollment takes at most 2 KiB. Commit verification accepts the existing 1 MiB
canonical commit limit plus a bounded signature envelope. An account can have
five pending challenges, twenty active keys and 128 lifetime registrations.
Challenge requests have a persistent ten-attempt/15-minute budget. Limits survive
router restart. Expired pending challenges are removed before creating another.
Global key uniqueness prevents reassigning the same public key to another account.

Account-row locks serialize enrollment, revocation, logout and account removal.
The service rechecks the exact login after acquiring the lock; the extractor's
earlier authentication is insufficient for this boundary. Database lock and
statement deadlines bound waiting. Account disable/erasure revokes existing keys
and removes pending challenges in the same transaction; restoration does not
undo revocation. Pseudonymous public key bindings/proofs remain with historical
authorship; they contain no email, device label, private key or login secret.

## Signature and storage contract

The signature message is the bytes `dynodoc.device-enrollment\0v1\0` followed by
canonical JSON of the challenge (sorted keys, compact UTF-8). All fields are
covered. Noncanonical hexadecimal, unknown fields and invalid scopes are rejected.
`device-enrollment-v1.json` pins the message and signature using an independent
Node/OpenSSL implementation with a **public synthetic** seed; do not regenerate
it to conceal a protocol mismatch.

Migration 0033 creates a read-only installation UUID, append-only key bindings
and revocations, and mutable bounded pending challenges. Runtime grants are
explicit and still reject DDL/ownership/trigger bypass. Normal owner DML also
cannot edit key history through its immutability triggers. A database owner can
disable those guards: external audit commitments remain a separate requirement.
Installation UUIDs identify a service, not its authenticity; enrollment requires
an authenticated trusted service connection. Database clones must not be treated
as independent authorities without a deliberate new installation identity.

## Verification and next integration

Core tests cover an independent signature vector and mutation of every signed
field. Restricted-role API tests cover cross-account attacks, altered but correctly
signed challenges, response-loss retries, expiry, restart-persistent rate bounds,
irreversible revocation, account restoration, concurrent quota enforcement and
logout while enrollment waits for the account lock. An authenticated browser
case exercises profile enrollment, reload and revocation using an ephemeral key.
The handoff records executed checks separately from merely prepared tests.

Before accepting remote project changes, reuse the account/key check inside the
same transaction as permission/review checks and ref CAS. A prior call to
`verify-commit` is not a grant. Do not expose the local object inventory over HTTP:
it includes private/unreachable staging objects. Remote commit reachability,
document erasure, protected review and atomic signed ref receipts still need their
own coordinated contract and acceptance tests.

That publication check is separate from historical verification: retained commits
must remain readable/verifiable after an author's key is revoked or the reader is
another team member. A future transport must distinguish already-accepted ancestry
from newly submitted author actions, and check the reader's current project/file
permission without demanding that every historical signer still has an active key.
