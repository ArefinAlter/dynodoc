# Project-scoped device sessions

12 October 2026, revised 13 and 14 October 2026. Device sessions are schema 38
(released with schema 39). The per-project key grants and the separate
`history:review` scope described below are schema 40: source only until a release
records them. Encrypted key files (14 October) are client-side only and change
nothing in this protocol: see [KEY-FILE-FORMAT.md](KEY-FILE-FORMAT.md). Executed
checks are listed under Verification; the handoff records exact commands.

This contract lets a command-line or native client that holds an **enrolled
Ed25519 device key** sign in to exactly one project, use only the project-history
routes, and renew without an email link, a browser cookie or a stored account
token. It replaces the interim use of an ordinary account access token by
`engine-cli sync`. It is not marketplace/installed-host acceptance, a hardware
token, an external witness or an end-user UI. The CLI can hold the key in a
passphrase-encrypted file or bind it to an operating-system keystore, which
protects the file at rest and is described under "CLI" and in the format document.

## Decisions

| Question | Decision |
|---|---|
| Long-lived credential | The device private key file only (unencrypted, or a `dynodoc-key-v1` encrypted file that the CLI unlocks with a passphrase or an OS keystore). The server stores the public key binding it already had (migration 33). No renewable secret, refresh token or token hash is stored for a device session. |
| Short-lived credential | A Paseto v4.local access token, 1 hour, one project, a subset of four scopes (`project:read`, `history:read`, `history:write`, `history:review`). Not written to disk by the CLI. A key enrolled for an AI agent (schema 42) instead gets the one scope `content:read`, accepted only by the agent routes; see [the grants contract](DEVICE-KEY-GRANTS-API.md) and [AI agent connectors](AI-AGENT-CONNECTORS.md). |
| Renewal | Sign a new one-use challenge. Possession of the key is the only renewal factor, so key revocation ends renewal immediately. |
| Who authorises a key for a project? | **The account holder, per project and scope, from an authenticated browser session** (schema 40). A key in `granted` mode (the default for every key enrolled from schema 40 on) can only be issued a session for a project with an active grant, and only for the granted scopes; the key holder no longer picks the reach. A key enrolled before schema 40 is `all` until its owner changes it: it keeps its old reach (every project the account can currently access). See "Per-project key grants". |
| Token confusion | A distinct Paseto **implicit assertion** and footer, distinct claim names and a `typ` claim. Every existing validator passes no implicit assertion and so fails closed at the authentication tag. |
| Where accepted | Only handlers that declare a `ProjectAuth<Scope>` extractor (explicit allow-list below). All other routes keep the account extractor, which cannot decrypt the new token. |
| Re-checks | Every request: key not revoked, account active and same session generation, token project equals the route project, scope, **the key's mode or active grant for this project and scope**, then the unchanged project/file role, policy, review and CAS checks. Membership removal, demotion, grant revocation or scope narrowing applies on the next request; nothing but the 1 hour TTL is cached. |
| Audit | One `device.session_issued` operation-audit row per issuance (key, session id, project, scopes, expiry). No row per request. Grant and mode changes have their own rows (`device.grant.set`, `device.grant.revoked`, `device.access.changed`; see the grants contract). |
| Migrations | `0038_device_sessions.sql`: one small mutable table, `device_session`, the one-use replay ledger and issuance-rate window. `0040_device_key_project_grants.sql`: `device_key_access` (one mode row per key), `device_key_project_grant` (revoke-only grant rows) and the widened scope check on `device_session`. |

## Protocol

All four steps use the existing key and a server-supplied challenge, but the
challenge is **stateless** so an unauthenticated caller can never cause a database
write or learn whether a public key is enrolled.

1. `POST /projects/{id}/device-sessions/challenge` with
   `{public_key, scopes}` (no bearer). The server returns
   `{challenge, mac}`. `challenge` is
   `{version:1, id, authority, public_key, project, scopes[], nonce, issued_at_ms,
   expires_at_ms, token_ttl_seconds}`; `mac` is an HMAC-SHA-256 under a key derived
   from the service's Paseto key and a domain label. The response is the same for an
   unknown, revoked or enrolled key and for a project that does not exist.
   Challenges live 120 seconds. Scopes are sorted/deduplicated; unknown scopes are
   rejected.
2. The device checks the challenge: `authority` is the independently configured
   installation, `public_key` is its key, `project` and `scopes` are what it asked
   for, TTL is not longer than one hour. It signs
   `"dynodoc.device-session\0v1\0" || canonical_json(challenge)` with Ed25519.
   Canonical JSON has sorted keys and compact UTF-8, as in device enrollment. A
   device-enrollment signature is not valid here and vice versa (different domain
   label and different fields).
3. `POST /projects/{id}/device-sessions` with `{challenge, mac, signature}`. The
   server verifies, without touching the database: shape, path project equals
   challenge project, MAC, Ed25519 signature. Then, in one transaction: database
   clock expiry, installation, key binding present and not revoked, account active,
   current project role (a contributing role when `history:write` is requested,
   an approving role when `history:review` is requested), **the key's mode or an
   active grant that lists every requested scope**, per-key issuance budget,
   **insertion of the challenge id into
   `device_session`** (a conflict is a replay), one audit row. Only then it mints
   the token. Every failure before the transaction and most inside it return the
   same `401`; role and grant problems are the same `403` (the caller is not told
which); budget exhaustion is `429`.
4. The device uses `Authorization: Bearer <token>`. Renewal repeats 1–3.

## Token

Paseto `v4.local` using the existing service key, with

- footer `dynodoc.project-device-session.v1`;
- **implicit assertion** `dynodoc.project-device-session.v1/implicit`;
- claims `typ`, `device_identity`, `device_key`, `project`, `scopes`,
  `device_generation`, registered `jti` (session id), `iat`, `nbf`, `exp`.

Paseto v4 authenticates the implicit assertion in the encryption tag. The existing
account validator calls `local::decrypt(..., footer=None, implicit=None)` and
therefore rejects every device token before reading its claims; the new validator
requires both values, `typ`, an `exp - iat` of at most one hour, a non-empty subset
of the three known scopes and a parsable identity/key/project/session, so an
account token, a connector key, a refresh token or a token from another key cannot
become a device token. A footer alone would **not** separate them (pasetors only
compares a footer when the caller supplies one); the implicit assertion is the
separating control and the tests prove the old verifier rejects the new token.

## Allow-list

| Scope | Routes (all under `/projects/{id}`, only for the token's own project) |
|---|---|
| `project:read` | `GET /files`, `GET /activity` |
| `history:read` | `GET /history/commits/{address}`, `.../objects/{document}`, `.../objects/{document}/{object}`, `GET /history/refs`, `GET /history/reviews`, `GET /history/receipts`, `GET /history/authority`, `GET /history/storage` |
| `history:write` | everything under `/staging/*`, `POST /history/commits`, `POST /history/commits/{address}/acknowledge`, `POST /history/refs` |
| `history:review` | `POST /history/reviews` (an approval or a request for changes) |

Scopes are independent: `history:write` does not imply a read and does not allow a
review decision; `history:review` does not allow a push. `engine-cli sync` chooses
its scopes in one place (`device_scopes` in `sync.rs`): `history:read` for
plan/fetch/pull and `history:read history:write` for push. It has no review
command, so it never asks for `history:review`; a command that records a review
decision must ask for exactly that scope. A token is **never** accepted by: account/profile/device-enrollment routes,
documents/ops/drafts/change requests, project detail/settings/members/drafts,
administration, connectors/provenance, `auth/*` (including refresh and logout),
SSE streams or the public API. It confers no administrator, owner or member
management right, and it cannot enroll, list or revoke keys.

A browser logout does **not** end device sessions: the key, not a login, is their
credential. Ending them is a key revocation (own Profile → Signing keys), account
disablement/erasure (revokes every key), or the administrator's *revoke sessions*
control, which now also revokes the account's device keys because a key can mint
a session without any login. Re-enroll a fresh key afterwards.

Review decisions need `history:review` (before schema 40 they rode on
`history:write`: a token issued earlier with only that scope gets `403` on
`POST /history/reviews` until it expires, at most one hour after the upgrade;
push, staging, publication and ref advancement are unaffected). Protected-`main`
advancement stays `history:write`. Both use the identical server checks as a
browser session: current project role, file floors, required independent approvals,
publisher cannot self-approve, fast-forward CAS, receipt signing. A device session is
the account acting; it has no extra authority and no exemption. A `history:review`
session needs an approving rank (owner, manager, editor or approver) at issuance
and again on every decision.

## Per-project key grants

Schema 38 deliberately let the key holder choose the project and scopes at request
time, limited by membership. An independent review found the consequence: an
unencrypted key file was a single factor for account-wide history writes and review
approval, and the holder, not the account owner, decided its reach. Schema 40 moves
that decision to the account holder. Contract and screen behaviour:
[device key grants API](DEVICE-KEY-GRANTS-API.md).

- Each key has a mode, `granted` or `all`. A key enrolled with schema 40 or later is
  `granted` unless the enrolment explicitly says `all`: the safe state is the
  default and the broad one needs a deliberate choice (and a confirmation field to
  switch to it later). Keys that existed at migration time are `all`, so nothing
  stops working; their owners can narrow them. A key with no mode row at all is
  treated as `granted`.
- A `granted` key is issued a session only for a project that has an **active
  grant** (an account-chosen set of scopes) and only for scopes inside it. Changing
  the scopes revokes the old row and inserts a new one; grant rows are never edited
  or deleted (runtime role: insert and update; a trigger allows exactly one
  `revoked_at` change). At most 50 active grants per key.
- Grants are created, narrowed and revoked only through account-session routes. A
  device token on those routes is `401`, and another account's key is `404`. The
  account must be a current member of the project to add or change a grant.
- A grant never exceeds the account: role, file floors, review rules and CAS are
  rechecked on every request exactly as before.
- Grant changes take the same account row lock as key revocation, so an in-flight
  request is either finished before the change or sees it.

What this removes: a copied key file (not in `all` mode) can no longer open projects
or scopes its owner did not grant, and cannot approve unless `history:review` was
granted. What it does not remove is listed under "Security caveats and not done".

## Request-time checks

`ProjectAuth<S>` (extractor) before any handler code: decrypt, footer, implicit
assertion, claims, scope `S`, **route `:id` equals token project** (a route
without an `id` path parameter is refused), and in one query: identity not
disabled/erased and `session_generation` equal to the token's, key row belongs to
the identity and has no revocation row. The existing transaction helpers then
repeat the key check **after the account row lock** (`devices::account_lock`), so
revocation, disable/erase and logout serialize with an in-flight request, and the
project check is repeated in `authorize`. Role, space-ancestry share locks,
document-level roles, policy floors and CAS are the unchanged code paths.

Read-only project pages (`files`, `activity`) have the same non-transactional
extractor check as other pages; they run the same current-role query as for a
browser session.

## Budgets, storage and privacy

- Issuance: 30 per rolling hour and 240 per rolling day per key (persistent, from
  `device_session`), plus the existing `workloads.enter(account, project)` admission
  and rate window. Over-budget returns 429 with `Retry-After`.
- `device_session` rows are deleted when older than one day by the next issuance for
  that key. A challenge lives two minutes, so the ledger always outlives replay
  risk. The permanent record is the audit row.
- Request logs record paths, not bodies, bearer tokens, challenges, MACs or
  signatures. Errors never echo them. The CLI marks bearer headers sensitive and
  prints only status/path.
- The access token is a bearer credential for up to one hour. It cannot be bound to
  the key per request (no proof-of-possession header), so a stolen token works
  until expiry or key revocation.

## CLI

`engine-cli device keygen --out FILE` creates a new Ed25519 key (`create_new`,
fsynced; never overwrites; removes a partial file on error) and prints the public
key. By default it is unencrypted DER PKCS#8; `--encrypt` writes a
passphrase-encrypted `dynodoc-key-v1` file instead and `--encrypt --keystore` binds
a random wrapping secret to the operating system (see below). It is created readable by the current user only: mode
`0600` on Unix; on Windows the file's access list is replaced with a single entry
for the current user (`icacls /inheritance:r /grant:r USER:F`, then read back and
verified). If that cannot be applied or verified the file is removed and the
command fails; `--allow-insecure-key-file` accepts the folder's permissions
instead (with a warning). Token files written by `sync` (the interim account mode)
use the same creation path, including the atomic replacement, so they keep the
restricted access list. Loading a key on Unix refuses a file readable by group or
others unless the same flag is given. Enroll the key in Profile → Signing keys as
before, then grant it the project it should use; the key is `granted` with no
projects until you do.

An encrypted key works in every place a key file is read, with the same
`--device-key FILE`. The passphrase comes from `--passphrase-file FILE` (a global
option), else the environment variable `DYNODOC_KEY_PASSPHRASE_FILE` (a path), else
a hidden terminal prompt; it is never a command-line argument, and with no source
the command fails before any network request. The key is decrypted once, in
memory, for the life of the process, and every renewal signs with it; the short
lived access tokens are still never written to disk. `device protect FILE` converts
an existing key in place (atomic replacement, same owner-only access, same public
key, so it stays enrolled) and `device unprotect FILE --i-understand` reverses it.
A binding to Windows DPAPI, the macOS keychain or the Linux Secret Service
(`--keystore`) removes the passphrase entirely and ties the file to that operating
system account.

```text
engine-cli sync REPO PROJECT --remote remote.json --journal pins.sqlite --branch main \
  --device-key signing-key.der plan COMMIT --out plan.json
```

The CLI obtains and renews the scoped token in memory (also on the first `401`).
Alternatively the interim account mode accepts
`--token-file access.txt --refresh-token-file refresh.txt`: on `401` it calls
`/auth/refresh` and atomically replaces both files (temporary file in the same
directory, fsync, rename; the temporary file is created *before* the server rotates
the refresh token, and a failed replacement never leaves a half file). A lost
response to the rotation still invalidates that refresh token; sign in again.
`--device-key` conflicts with both file options.

## Verification

Source run on 12 October 2026 (Windows, restricted runtime role, PostgreSQL 16):

- `engine-core`: independent Node.js signature/canonical-bytes vector
  (`tests/fixtures/device-session-v1.json`), every-field mutation, scope and TTL
  rules, domain separation from enrollment.
- `engine-api` unit: the account decrypt call fails for a device token and the
  device call fails for an account token **at the cryptographic layer**, before any
  claim; missing implicit assertion, missing footer, forged lifetime, unknown or
  unsorted scopes and wrong type are refused.
- `engine-api/tests/device_sessions_test.rs` (real router, restricted login):
  issue, use on every allowed route class, 403 without the scope, 403 for another
  project, 401 on about 35 other sampled routes across account, profile, document,
  admin, connector, notification, search and `auth/*` classes (the administrator's own
  identity inside the token still gets 401 on `/admin`), one-use challenge
  (sequential and concurrent), challenge for another project, tampered
  scopes/MAC, unknown/revoked key and non-member, expired/foreign-key/forged-claim
  tokens, demotion to viewer, removal and re-add, key revocation, account
  disablement, generation change, administrator *revoke sessions*, uniform
  challenge shape and no writes on request, hourly/daily budgets and pruning, one
  audit row per issuance with no token material.
- `engine-cli` (real HTTP, restricted runtime): `keygen` exclusivity and
  loadability; refresh rotation with expired access token, atomic replacement, no
  temp files, retired secret, invalid secret leaves files intact; device sessions
  for push/fetch with read-only and read/write scopes, contributor unable to move
  protected `main`, required approval and owner approval through a device session,
  key revocation, membership removal.

Encrypted key files (14 October 2026, Windows 11 host; no server schema change):
`engine-cli` unit tests for the container, an independent Python/OpenSSL vector,
tamper and bound checks, and DPAPI through the real Windows API; and
`tests/key_files.rs`, which drives the built binary: encrypted keygen, every
passphrase source and their order, wrong and missing passphrases, argument refusal,
protect and unprotect (in place and to a new file), a signed local commit with an
encrypted key, and `sync --device-key` with an encrypted key reaching a recording
loopback socket with a challenge request for the key's public key. The sync test
does not run a server; the device-session protocol itself is covered by the
tests above with unencrypted keys.

Schema 40 (13 October 2026, Windows, restricted runtime role, PostgreSQL 16):

- `engine-api/tests/device_key_grants_test.rs`: grant/deny matrix (no grant, wrong
  project, scope subset, widening), narrowing and revocation reaching a live token
  on its next request, legacy `all` keys unchanged, `all` needing the explicit
  confirmation and keeping grants, enrolment default and a retry that cannot widen,
  review needing its own scope and an approving rank, push working with write
  alone, owner-only and session-only management (device token and no login are
  `401`, another account's key `404`), non-member and revoked-key cases, the
  50-grant limit, membership removal and re-adding, concurrent issuance against
  revocation and concurrent grants, forged service-key claims, revoke-only grant
  rows and missing runtime DELETE, migration backfill of existing keys, audit rows,
  and `GET /profiles/{id}/starred` visibility, counts and the 100 bound.
- `engine-cli`: the real-HTTP flow with a granted key, the review step using its
  own scope, scope selection per action; `private_file` unit tests for the
  `icacls` arguments and output parsing, the Unix mode rule (compiled and run only
  on Unix), and a real `icacls` run on this host showing a created file and an
  atomically replaced file each carrying exactly one entry for the current user.

Known unrelated failure on this Windows host: `sse_delivers_a_new_event_within_100ms`
(a 100 ms wall-clock deadline) fails with or without this change.

## Security caveats and not done

What remains after schema 40, precisely:

- **An unencrypted key file is the whole credential.** It is the default of `device
  keygen`; `--encrypt` is a choice and existing keys stay as they are until
  `device protect` is run. Owner-only file permissions stop other local accounts,
  not malware or a backup that runs as the user, and they are not checked again
  when a Windows key file is loaded (only files created by `device keygen` carry
  the restricted access list; a key made elsewhere keeps its folder's). On Unix a
  key readable by group or others is refused at load.
- **What the encrypted file does and does not give.** At rest, a stolen or backed
  up file needs an offline guess of its passphrase, slowed by 600,000 PBKDF2
  iterations but not memory-hard. While `engine-cli` runs the decrypted key is in
  its memory (wiped best effort; `ring`'s key object and the OS may keep copies),
  and same-account malware, a keylogger, or a passphrase file kept beside the key
  defeats it. An OS keystore binding (DPAPI, keychain, Secret Service) answers to
  every process of that account. None of this is a TPM, smart card or per-use
  confirmation, and no hardware-backed key type exists yet. A plain conversion
  (`protect`) cannot erase the old plaintext blocks. The macOS and Linux keystore
  paths are implemented but have not been run on those systems.
- **`all` keys keep account-wide reach.** Keys enrolled before schema 40 are `all`
  until their owner switches them; the profile should say so. Anyone with such a
  file can still open any project the account can reach, with any scope the role
  allows, including `history:review`. A key enrolled with `project_access: "all"`
  is the same.
- A `granted` key's holder can use every scope granted for a project the whole
  time the key is valid; grants are not time-limited and a stolen token works until
  it expires (one hour) or the grant/key is revoked.
- Revoking a grant or key is by the account holder, from an authenticated session;
  an administrator can still end an account's sessions, which also revokes its
  keys.
- A server impersonator that the user configured in `remote.json` can relay a
  challenge from the real server and obtain a token. The remote URL, TLS
  certificate and the independently verified authority are the trust anchor; the
  CLI checks authority, key, project and scopes before signing.
- No per-session revocation, no session listing UI, no per-grant expiry, no
  interactive-only restriction for review approvals (a granted `history:review`
  scope can approve; the CLI has no approve command yet), no connector
  (Office/Google) adoption, and no installed-editor or marketplace acceptance.
- The sessions share the server's global Paseto key; rotating it ends all sessions.
- Rate budgets are process-local admission plus a persistent per-key window
  (exact under concurrency through a per-key advisory lock), not a fleet-wide quota.
