# Project-scoped device sessions

12 October 2026. Source schema 38; not deployed to the schema-29 pilot. Executed
checks are listed under Verification; the handoff records exact commands.

This contract lets a command-line or native client that holds an **enrolled
Ed25519 device key** sign in to exactly one project, use only the project-history
routes, and renew without an email link, a browser cookie or a stored account
token. It replaces the interim use of an ordinary account access token by
`engine-cli sync`. It is not marketplace/installed-host acceptance, a hardware or
OS keystore, an external witness or an end-user UI.

## Decisions

| Question | Decision |
|---|---|
| Long-lived credential | The device private key file only. The server stores the public key binding it already had (migration 33). No renewable secret, refresh token or token hash is stored for a device session. |
| Short-lived credential | A Paseto v4.local access token, 1 hour, one project, a subset of three scopes. Not written to disk by the CLI. |
| Renewal | Sign a new one-use challenge. Possession of the key is the only renewal factor, so key revocation ends renewal immediately. |
| Who authorises a key for a project? | **Chosen at request time by the key holder, limited by current membership** (no per-project key-grant table). The account holder already authorised the key account-wide by enrolling it in an interactive session; project access is whatever the account's role is *at each request*. See "Why no per-project key grant". |
| Token confusion | A distinct Paseto **implicit assertion** and footer, distinct claim names and a `typ` claim. Every existing validator passes no implicit assertion and so fails closed at the authentication tag. |
| Where accepted | Only handlers that declare a `ProjectAuth<Scope>` extractor (explicit allow-list below). All other routes keep the account extractor, which cannot decrypt the new token. |
| Re-checks | Every request: key not revoked, account active and same session generation, token project equals the route project, scope, then the unchanged project/file role, policy, review and CAS checks. Membership removal or demotion applies on the next request; nothing but the 1 hour TTL is cached. |
| Audit | One `device.session_issued` operation-audit row per issuance (key, session id, project, scopes, expiry). No row per request. |
| Migration | `0038_device_sessions.sql`: one small mutable table, `device_session`, the one-use replay ledger and issuance-rate window. |

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
   current project role (and a contributing role when `history:write` is
   requested), per-key issuance budget, **insertion of the challenge id into
   `device_session`** (a conflict is a replay), one audit row. Only then it mints
   the token. Every failure before the transaction and most inside it return the
   same `401`; role problems are `403`; budget exhaustion is `429`.
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
| `history:write` | everything under `/staging/*`, `POST /history/commits`, `POST /history/commits/{address}/acknowledge`, `POST /history/refs`, `POST /history/reviews` |

Scopes are independent: `history:write` does not imply a read. `engine-cli sync`
asks for `history:read` for plan/fetch/pull and `history:read history:write` for
push. A token is **never** accepted by: account/profile/device-enrollment routes,
documents/ops/drafts/change requests, project detail/settings/members/drafts,
administration, connectors/provenance, `auth/*` (including refresh and logout),
SSE streams or the public API. It confers no administrator, owner or member
management right, and it cannot enroll, list or revoke keys.

A browser logout does **not** end device sessions: the key, not a login, is their
credential. Ending them is a key revocation (own Profile → Signing keys), account
disablement/erasure (revokes every key), or the administrator's *revoke sessions*
control, which now also revokes the account's device keys because a key can mint
a session without any login. Re-enroll a fresh key afterwards.

Review decisions and protected-`main` advancement are included in `history:write`
and use the identical checks as a browser session: current project role, file
floors, required independent approvals, publisher cannot self-approve, fast-forward
CAS, receipt signing. A device session is the account acting; it has no extra
authority and no exemption.

## Why no per-project key grant

A per-project grant table would let a leaked key reach only the projects it was
granted. It would also need an interactive grant/revoke UI and API before the CLI
could be used at all, and every key already exists with the account's authority
(it can sign commits for any project the account contributes to). We therefore
kept the smaller model and made its blast radius explicit:

- A leaked key file can mint sessions for **every project the account can
  currently access**, until the key is revoked (Profile → Signing keys). Treat the
  key file like an SSH private key; enroll one key per device (limit 20).
- Role demotion/removal or account disablement stops the next request; key
  revocation stops the next request *and* all renewals; sessions are not used as a
  separate revocation unit.
- Every issuance is audited with key, project and scopes, so administrators and
  the account holder can see which projects a key opened.

A per-key project allow-list (append-only `device_key_project` rows) can be added
later without changing token or protocol format: the issue step would add one
check. It is deliberately not part of this increment.

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

`engine-cli device keygen --out FILE` creates a new unencrypted DER PKCS#8 Ed25519
key (`create_new`, mode `0600` on Unix, fsynced; never overwrites; removes a
partial file on error) and prints the public key. Enroll it in Profile → Signing
keys as before.

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

Known unrelated failure on this Windows host: `sse_delivers_a_new_event_within_100ms`
(a 100 ms wall-clock deadline) fails with or without this change.

## Security caveats and not done

- The key file is unencrypted; there is no OS keystore, TPM or hardware token
  integration. On Windows the file inherits its folder's ACL; store it in a private
  directory.
- A server impersonator that the user configured in `remote.json` can relay a
  challenge from the real server and obtain a token. The remote URL, TLS
  certificate and the independently verified authority are the trust anchor; the
  CLI checks authority, key, project and scopes before signing.
- No per-project key grant, no per-session revocation, no session listing UI, no
  interactive-only restriction for review approvals, no connector (Office/Google)
  adoption, and no installed-editor or marketplace acceptance.
- The sessions share the server's global Paseto key; rotating it ends all sessions.
- Rate budgets are process-local admission plus a persistent per-key window
  (exact under concurrency through a per-key advisory lock), not a fleet-wide quota.
