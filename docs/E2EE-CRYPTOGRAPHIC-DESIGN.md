# End-to-end encryption: cryptographic design

Status: design, not implemented. Date: 14 October 2026. Owner: Verne Ltd. This document is
documentation only; no code, schema or API in the repository changes because of it. Every
construction below is **proposed** and carries no security claim until an independent review
has accepted it (see [the assurance plan](E2EE-THREAT-MODEL-AND-ASSURANCE.md)).

Part of a set. Read the [master design](END-TO-END-ENCRYPTION.md) first. Companions:
[protocol and migration](E2EE-PROTOCOL-AND-MIGRATION.md),
[clients and key management](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md),
[threat model and assurance](E2EE-THREAT-MODEL-AND-ASSURANCE.md). Related, written
concurrently and not authoritative here: [SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md) and
[AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md).

## 1. Purpose, goals, non-goals

### 1.1 Purpose

Specify the cryptographic building blocks for Phase 1 ("encrypted projects") and the key
management objects that Phases 2 and 3 extend: the key hierarchy, algorithms and parameters,
how HPKE is used, key epochs and rotation, keyed content identifiers, authenticated encryption
with associated data bound to project, document, node and revision, the nonce strategy, how
signatures stay valid over ciphertext, recovery and escrow, and a test-vector plan. It also
states what the construction does and does not protect, and why a lighter construction than
Signal, MLS or Matrix was chosen.

### 1.2 Goals

| ID | Goal | Meaning |
|---|---|---|
| G1 | Content confidentiality against the service | A party holding the database, object store, backups and server memory, but no member key, learns no document text, formatting, cell values, comments, titles, file names or commit messages of an encrypted project. |
| G2 | Integrity and attribution survive | Hash chains, commit signatures, ref receipts and review records remain verifiable by anyone, without keys, over ciphertext. Nothing in the existing provenance contract weakens. |
| G3 | Ciphertext cannot be moved | A ciphertext produced for one project, document, node, kind, epoch or revision does not decrypt in another position. |
| G4 | Membership change is cryptographic | Removing a member or device stops that party from decrypting content written after the next epoch, without a server decision being the only barrier. |
| G5 | Durable readability | Authorized members, including members added later, can read the full retained history for years. Forward secrecy of past content is **not** a goal (see 12.3). |
| G6 | Recoverability by design | A lost device does not lose a project. Loss of every key is possible only by an explicit, warned choice. |
| G7 | Delegation | A subset of a project (one file, read-only) can be delegated to a device or a local agent without handing over the project key. |
| G8 | Implementable on every client | Every primitive is available through WebCrypto (browsers, Office task panes, Apps Script sidebars) and through Rust (`engine-cli`, WASM). |
| G9 | Cryptographic agility without ambiguity | A suite identifier is authenticated in every object; a downgrade is detectable. |
| G10 | Small, reviewable surface | Fewer than ten message types, one AEAD construction, one key-wrap mechanism, no custom primitive. |

### 1.3 Non-goals

- Hiding membership, the document tree shape, edit timing, object sizes or event types from the
  service in Phase 1. The metadata inventory is in [the threat model](E2EE-THREAT-MODEL-AND-ASSURANCE.md).
- Forward secrecy and post-compromise security at message granularity (ratchets). Not wanted
  for an archive; see 12.3.
- Protecting against a compromised member device, a compromised host editor or a malicious
  member who copies plaintext. Endpoint compromise is out of scope and stated, not mitigated.
- Anonymity, deniability or unlinkability. Commits are signed and attributable by design.
- Searchable encryption, homomorphic features or server-side computation over ciphertext.
- Post-quantum security in Phase 1. A hybrid KEM migration path is reserved (section 12.4).
- A claim of "zero knowledge". The term is avoided; see the claims policy in the
  [master design](END-TO-END-ENCRYPTION.md#3-claims-policy).

### 1.4 Personas

| Persona | Need from the cryptography |
|---|---|
| Research lab lead | Share a protocol draft with collaborators at several institutions; revoke a departed student without re-sharing every file. |
| Enterprise security architect | Evidence that the SaaS operator cannot read board papers; a recovery path that does not depend on the operator. |
| Compliance officer | Auditable key events (who was added, when keys rotated, any escrow use) that survive in the signed history. |
| Self-hosted operator | A separate story: at-rest protection with customer keys (Phase 0). They may not need Phase 1. |
| Local-agent user | Let a locally running agent read one file without exposing the project key or sending plaintext to Dynodoc. |

### 1.5 Success metrics

| Metric | Target for first release of Phase 1 | How measured |
|---|---|---|
| Seal or open one 4 KiB node object | p95 under 5 ms on a 2022 laptop in a browser worker | Benchmark in CI on reference hardware |
| Open a 100-page document (about 600 nodes, one project epoch) | p95 under 2 s from cached ciphertext | Benchmark fixture |
| Storage overhead beyond plaintext | Under 12 percent average after padding for typical documents (padding bucket design in 8.6) | Corpus measurement |
| Unrecoverable-loss incidents among projects with two recovery paths | 0 | Support and telemetry (counts only) |
| Cross-implementation test vectors | 100 percent agreement between Rust, TypeScript and the WASM build | CI matrix |
| External review findings | All critical and high closed before general availability | Review report |

## 2. Current state versus target

Observation basis: repository HEAD `994c429` plus the uncommitted working tree of 14 October
2026, which another workstream is changing (receipt-authority epochs in migration 0041, key
files, local-repository packing). Rows marked "working tree" can change before they are
committed; re-check before relying on them.

The cryptography in the system today is integrity and authentication only. There is no
confidentiality mechanism for document content.

| Capability | State | Evidence |
|---|---|---|
| SHA-256 hash chain over events (`content_hash`, `chain_hash`) | EXISTS | `crates/engine-core/src/log.rs`: `content_hash`, `chain_hash`, `verify_from` |
| Domain-separated SHA-256 object addresses (`dynodoc.shared-checkpoint\0v1\0`, `dynodoc.typed-checkpoint\0v2\0`, `dynodoc.content\0v1\0`) | EXISTS | `crates/engine-core/src/shared_checkpoint.rs`, `shared_checkpoint/typed.rs`, `shared_checkpoint/content.rs` |
| Ed25519 signatures over project commits (v1, v2, v3 domains) | EXISTS | `crates/engine-core/src/repository.rs`: `SignedCommit::sign`, `verify` |
| Ed25519 account-bound device keys, enrolment proof, project-scoped sessions | EXISTS | `crates/engine-core/src/repository/device.rs`, `device_session.rs`; [DEVICE-SIGNING-KEYS.md](DEVICE-SIGNING-KEYS.md), [PROJECT-SCOPED-DEVICE-SESSIONS.md](PROJECT-SCOPED-DEVICE-SESSIONS.md) |
| Ed25519 signed ref receipts with rotation and client pins | EXISTS (authority not installed in production) | `crates/engine-core/src/repository/attestation.rs`; [RECEIPT-AUTHORITY-DECISION.md](RECEIPT-AUTHORITY-DECISION.md) |
| Per-project signing-key grants and a review scope | EXISTS (schema 40) | `migrations/0040_device_key_project_grants.sql`; [DEVICE-KEY-GRANTS-API.md](DEVICE-KEY-GRANTS-API.md) |
| Paseto v4.local tokens (symmetric AEAD, server-side only) | EXISTS | `crates/engine-api/src/auth/token.rs`, `pasetors` in `Cargo.toml` |
| Canonical JSON with sorted keys and length-framed hashing | EXISTS | `log.rs` `canonical_json`, `feed`; `shared_checkpoint/canonical.rs` |
| Any confidentiality primitive for content (AEAD, KEM, key wrap) | MISSING | No AEAD or key agreement in `engine-core`. `ring` is used for SHA-256 and Ed25519 only |
| Static X25519 keys | MISSING | `ring::agreement` supports ephemeral private keys only; a static device encryption key needs another crate (section 3.3) |
| Keyed content identifiers | MISSING | All addresses are unkeyed SHA-256 of plaintext-derived bytes |
| Key epochs, grants, membership log | MISSING | Proposed in sections 6 and 10 |
| Passphrase- or keystore-encrypted signing key file for the CLI | PARTIAL (working tree, uncommitted on 14 October 2026) | `crates/engine-cli/src/keyfile.rs` (`dynodoc-key-v1`: PBKDF2-HMAC-SHA256 at least 600,000 iterations, ChaCha20-Poly1305 via `ring`), `keystore.rs` (DPAPI, macOS keychain, Linux Secret Service), `passphrase.rs`; [decision 015](decisions/015-customers-formula-engine-and-planned-products.md) item 5. Holds an Ed25519 signing key only: no encryption key, no epoch material. Released CLI keys are unencrypted DER files with owner-only ACLs ([PROJECT-STATUS.md](PROJECT-STATUS.md) "Not done") |
| Encrypted key storage in browsers, add-ins or sidebars | MISSING | No client other than the CLI holds a signing key today (decision 014); add-in connection keys are memory-only ([listings/addin-data-handling.md](listings/addin-data-handling.md)) |
| Backup encryption | MISSING | `infra/demo/backup.sh` writes a plain `pg_dump -Fc` and a SHA-256 file |
| Public statement | Not E2EE, no at-rest claim | `frontend/researcher/src/routes/(site)/security/+page.svelte` ("Not end-to-end encrypted"), `privacy/+page.svelte` |

## 3. Primitives, parameters and library mapping

### 3.1 Suite 0x01 (the only suite in Phase 1)

| Role | Primitive | Parameter | Why |
|---|---|---|---|
| Content AEAD | AES-256-GCM (NIST SP 800-38D) | 256-bit key, 96-bit nonce, 128-bit tag; each key encrypts exactly one message | Only AEAD available in WebCrypto on every target; hardware accelerated; single-use keys remove the nonce-count limit |
| Key derivation | HKDF-SHA-256 (RFC 5869) | Salt and info as specified; output up to 8160 bytes, used up to 76 | In WebCrypto and `ring`; one-way, which makes delegation (G7) possible |
| Message authentication and content identifiers | HMAC-SHA-256 | 256-bit key | In WebCrypto and `ring`; keyed identifier must not be a bare hash (section 9) |
| Hash | SHA-256 | 32-byte output | Matches existing addresses; `ring::digest` |
| Signatures | Ed25519 (RFC 8032) | Existing device signing keys | Already deployed; WebCrypto in Chrome 137+, Firefox 129/130+, Safari 17+ (section 3.4) |
| Key agreement for grants | X25519 (RFC 7748) inside HPKE | Static per-device encryption key | In WebCrypto in Chrome 133+, Firefox 130+, Safari (version to verify) |
| Key encapsulation | HPKE base mode, RFC 9180 | KEM 0x0020 DHKEM(X25519, HKDF-SHA256), KDF 0x0001 HKDF-SHA256, AEAD 0x0002 AES-256-GCM | Standardised, library support in Rust and TypeScript (section 3.5) |
| Passphrase stretching (client key files only) | Argon2id (RFC 9106) or scrypt | Parameters in section 11.4 | Not in WebCrypto; WASM or JS implementation; used only for local key files |
| Randomness | Operating-system CSPRNG | `crypto.getRandomValues`, `getrandom` | No application RNG |

Reserved identifiers, not implemented in Phase 1:

| Suite | Meaning | Condition |
|---|---|---|
| 0x02 | XChaCha20-Poly1305 content AEAD for native clients | Only if WebCrypto gains a stable ChaCha20-Poly1305 and a 192-bit-nonce construction is still wanted. Not needed with single-use keys |
| 0x10 | Hybrid post-quantum grant KEM (X-Wing or ML-KEM-768 with X25519) | Standardised HPKE codepoint and an audited implementation (section 12.4) |

A suite identifier is carried and authenticated in every sealed object (it is inside the AAD,
section 8.3) and in every key grant (it is inside the HPKE info string, section 7). A reader
refuses an unknown suite. There is no negotiation: the epoch record fixes the suite for the
whole epoch, so an attacker cannot select a weaker one.

### 3.2 Parameter summary

| Quantity | Value |
|---|---|
| Epoch root key (`PER`) | 32 random bytes |
| Object salt | 32 random bytes per sealed object |
| Derived AEAD key / nonce / commitment key | 32 / 12 / 32 bytes from one HKDF call (76 bytes) |
| Key commitment tag | First 16 bytes of HMAC-SHA-256 over the AAD |
| Keyed content identifier | 32 bytes HMAC-SHA-256 |
| Key identifier hint | 8 bytes of SHA-256 over a labelled epoch root |
| HPKE encapsulated key | 32 bytes; sealed payload 32 + 16 bytes |
| Padding buckets | 128 B to 64 KiB in powers of two (section 8.6) |
| Maximum sealed object | 1 MiB (the existing `MAX_OBJECT_BYTES`); recommended node-field objects at most 256 KiB |

### 3.3 Where `ring` is and is not enough

The engine depends on `ring = "0.17"` (`Cargo.toml`). Reading its source in the local registry
(`ring-0.17.14`):

- `ring` provides SHA-256, HMAC, HKDF, AES-GCM, ChaCha20-Poly1305 and Ed25519. These cover
  sealing, derivation, identifiers and signatures.
- `ring::agreement` exposes only ephemeral private keys (`EphemeralPrivateKey`,
  `agree_ephemeral`). A long-lived device encryption key, which is what an HPKE recipient is,
  cannot be loaded or stored through `ring`. Phase 1 therefore adds a second crate for X25519
  and HPKE (section 3.5), isolated behind a small trait so it can be swapped.
- `ring` supports `wasm32-unknown-unknown` when the `wasm32_unknown_unknown_js` feature is
  enabled (it routes randomness to `getrandom/js`). It still needs a C toolchain that targets
  wasm (clang) at build time. The WASM assessment is in the
  [clients document](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md#4-the-engine-in-webassembly).

### 3.4 WebCrypto support (verified 14 October 2026, with limits)

| Algorithm | Chrome | Firefox | Safari | Source and date |
|---|---|---|---|---|
| Ed25519 | 137 (May 2025) | 129 or 130 (August or September 2024; sources differ) | 17.0 (September 2023) | [IPFS blog](https://blog.ipfs.tech/2025-08-ed25519/) (13 August 2025); [Igalia](https://blogs.igalia.com/jfernandez/?p=1913) (28 February 2025, updated 31 March 2025); [WICG issue 20](https://github.com/WICG/webcrypto-secure-curves/issues/20) (thread to 30 June 2025) |
| X25519 | 133 (February 2025, enabled by default) | 130 (September 2024) | Enabled in WebKit December 2024; shipped in Safari Technology Preview 211, January 2025; stable Safari version not confirmed | Same sources |
| AES-GCM, HKDF, HMAC, SHA-256 | All | All | All | [MDN SubtleCrypto.generateKey](https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto/generateKey) (fetched 14 October 2026; generateKey is "Baseline widely available", secure contexts only, available in Web Workers) |
| ChaCha20-Poly1305, ML-KEM, ML-DSA, X-Wing | Developer trial planned for Chrome 150, origin trials 151 to 154 (intent posted 17 June 2026) | Neutral ("Firefox is shipping anyway" per the proposal) | Neutral | [blink-dev intent](https://groups.google.com/a/chromium.org/g/blink-dev/c/TF8y175J3YU/m/esAKJTZTAAAJ) |
| Argon2 | Not in browsers | Not in browsers | Not in browsers | Node.js has it; no browser implementation found (search of 14 October 2026) |

Limits of this verification: no caniuse or MDN compatibility table dated 2026 could be fetched
(the MDN page returned no compatibility section). The IPFS post says Ed25519 reaches about 79
percent of web users. Re-verify against MDN browser-compat-data and the Chrome Platform Status
entries at build time, and keep the feature test described in section 3.6.

### 3.5 HPKE and primitive libraries (verified 14 October 2026)

| Library | Language | What it offers | Maturity notes |
|---|---|---|---|
| [`hpke` (rust-hpke)](https://docs.rs/crate/hpke/latest) 0.13 and 0.14 | Rust | RFC 9180 with X25519, X448, P-curves, ML-KEM-768 and 1024 | Pure Rust. Docs state it "has not been formally audited"; Cloudflare performed an internal review of 0.8 |
| [`hpke-rs`](https://docs.rs/hpke-rs) 0.6.0 (Cryspen) | Rust | RFC 9180 with pluggable backends (libcrux, RustCrypto), X-Wing draft-06 | libcrux backend is formally verified for a limited suite; check the suite coverage before choosing it |
| RustCrypto `dhkem` 0.0.1-alpha | Rust | DHKEM adapter only | Early; not a full HPKE |
| [`@hpke/core`, `@hpke/dhkem-x25519`, `hpke-js`](https://github.com/dajiaji/hpke-js) | TypeScript | RFC 9180 over WebCrypto; X25519 in a separate extension package | README: "has not been formally audited"; passes RFC 9180 vectors and Wycheproof ECDH/X25519 vectors; PQ and secp256k1 extensions are marked experimental and not standardised |
| Web platform HPKE | Browsers | None | MDN `generateKey` page does not list HPKE; no browser implementation found |

Consequence: HPKE is available in both runtimes but through unaudited libraries. The design
therefore keeps HPKE usage minimal (base mode, one suite, one short plaintext) so that a
reviewer can check the library against RFC 9180 test vectors and so that replacing it with a
direct "X25519 plus HKDF plus AES-GCM" construction costs little (section 7.4).

### 3.6 Capability detection

A client runs a start-up self-test and refuses to open an encrypted project if any step fails,
with a message that names the missing capability:

1. `crypto.subtle.generateKey({name:"Ed25519"})` and a sign/verify round trip.
2. `crypto.subtle.generateKey({name:"X25519"})` and a `deriveBits` round trip.
3. AES-256-GCM, HKDF-SHA-256 and HMAC-SHA-256 known-answer tests from the shared vectors.
4. `isSecureContext` is true; the page runs in a worker or an isolated frame (clients document).

If native Ed25519 or X25519 is missing, the client may use a vetted pure-JavaScript or WASM
implementation (the clients document names the options), but then non-extractable key storage
is unavailable and the client must say so (clients document, section 3).

## 4. Key hierarchy

```
 USER-HELD ROOTS (never leave the user's devices unwrapped)
 +---------------------------+   +---------------------------+   +---------------------------+
 | Device signing key  DSK   |   | Device encryption key DEK |   | Recovery key  RK          |
 | Ed25519, EXISTS (sch. 33) |   | X25519, PROPOSED          |   | 256-bit secret, optional  |
 | signs commits, sessions,  |   | HPKE recipient for grants |   | derives a recovery X25519 |
 | epoch + membership records|   | one per device            |   | recipient (section 11)    |
 +---------------------------+   +---------------------------+   +---------------------------+
               |                               |                               |
               | binding signature             | HPKE seal (one grant per      |
               v                               v  device, epoch, scope)        v
 PER-PROJECT, PER-EPOCH (random, generated by the member who opens the epoch)
 +--------------------------------------------------------------------------------------+
 | Epoch root  PER_e   32 random bytes, one per project key epoch e                     |
 +--------------------------------------------------------------------------------------+
               | HKDF-SHA256, one-way, label per child
               +--> K_meta_e       project-level text: titles, file names, commit messages,
               |                   issue text, labels, version names, notes
               +--> K_cid_e        keyed content identifiers (HMAC key)
               +--> K_doc_{e,d}    one per document d: HKDF(PER_e, label || project || d)
                                   derivable, so it can be delegated alone (G7)
 EXPLICIT-SCOPE DOCUMENTS (documents whose access list is narrower than the project)
 +--------------------------------------------------------------------------------------+
 | Document root  DR_{e,d}   32 random bytes, NOT derivable from PER_e; granted only to |
 | the document's own recipients. Takes the place of K_doc_{e,d} for that document.     |
 +--------------------------------------------------------------------------------------+
               |
               v  per object, single use
 +--------------------------------------------------------------------------------------+
 | K_obj = HKDF(K_doc or K_meta, salt = 32 random bytes, info = suite label)            |
 |   -> 32-byte AEAD key | 12-byte nonce | 32-byte commitment key                        |
 +--------------------------------------------------------------------------------------+

 ESCROW (Phase 3, optional, visible in the membership log)
 +--------------------------------------------------------------------------------------+
 | Organisation escrow recipient  OER  X25519; private half in a customer HSM/KMS or    |
 | split t-of-n among named custodians. Receives an extra grant for every epoch.        |
 +--------------------------------------------------------------------------------------+
```

### 4.1 Key table

| Key | Type and size | Created by | Stored where | Lifetime | Compromise consequence |
|---|---|---|---|---|---|
| `DSK` | Ed25519 | Device (`engine-cli device keygen`, browser, add-in) | Client key store (clients document) | Until revoked | Forged commits and reviews as that account within granted scopes (existing risk, schema 40 grants limit it) |
| `DEK` | X25519 | Device, at first unlock | Client key store, non-extractable where the platform allows | Until the device is removed | Attacker can open grants addressed to that device: current and earlier epochs it holds |
| `RK` | 256 bits, Crockford base32 in groups | User, shown once | Paper, password manager, or hardware key derived (Phase 3) | Until rotated by the user | Same as all devices of that account |
| `PER_e` | 32 random bytes | The member who opens epoch `e` (owner or manager) | Only inside HPKE grants and client memory/stores | One epoch | Plaintext of everything sealed under epoch `e`, for the holder |
| `K_meta_e`, `K_cid_e`, `K_doc_{e,d}` | 32 bytes each | Derived from `PER_e` | Memory only; recomputed on use | One epoch | `K_doc` exposes one document in one epoch; the others expose their class |
| `DR_{e,d}` | 32 random bytes | Document owner or manager | Only inside grants | One epoch | One document in one epoch |
| `K_obj` | 76 bytes derived | Per sealed object | Memory only; never stored | One message | One object |
| `OER` | X25519 | Organisation administrator (ceremony) | HSM/KMS or Shamir shares | Policy | All projects of the organisation that include the escrow recipient |

### 4.2 Why this shape

- **Per-epoch random roots, not a derivation chain.** Removing a member must leave the removed
  party unable to derive the next epoch. A hash chain from `PER_e` to `PER_{e+1}` would let
  anyone holding `PER_e` compute every later key. Each epoch root is fresh randomness.
- **One-way derivation inside an epoch.** `K_doc_{e,d}` is `HKDF(PER_e, ...)`, so the holder of
  a document key cannot recover `PER_e` or another document's key. This gives delegation to an
  agent or a guest reviewer for one file without a separate key distribution protocol.
- **Explicit document roots for restricted files.** Today a document can have an access list
  narrower than its project (`document_access`, `document_member_role`; see
  `crates/engine-api/src/project_staging.rs` `authorize`). A project member who is not on the
  file's list must not be able to derive its key. Such documents use an independent random
  `DR_{e,d}`, granted only to their recipients. The server already decides who may *fetch*
  such files; the keys make that decision hold even if the server is wrong.
- **Single-use object keys.** A random 32-byte salt per object makes every AEAD key and nonce
  unique. There is no nonce counter to coordinate across devices and no 2^32-message limit.

### 4.3 Domain-separation registry

All labels follow the existing `dynodoc.<name>\0v<N>\0` convention used by commit, receipt and
checkpoint hashes. Labels are byte strings; `\0` is a single zero byte.

| Label | Used for |
|---|---|
| `dynodoc.k.meta.v1` | `K_meta_e` |
| `dynodoc.k.cid.v1` | `K_cid_e` |
| `dynodoc.k.doc.v1` | `K_doc_{e,d}`; info is label then 16-byte project UUID then 16-byte document UUID |
| `dynodoc.seal.v1` | First AAD field of every sealed object |
| `dynodoc.seal.v1/aead+nonce+commit\0` + suite byte | HKDF info for the per-object key block |
| `dynodoc.sealed-object\0v1\0` | Address hash domain for sealed objects |
| `dynodoc.cid.v1\0` | Keyed content identifier |
| `dynodoc.key-id.v1\0` | Key identifier hint |
| `dynodoc.device-encryption-binding\0v1\0` | Signature binding an X25519 key to an Ed25519 key and account |
| `dynodoc.key-grant\0v1\0` | HPKE `info` prefix; `dynodoc.key-grant-signature\0v1\0` signature domain |
| `dynodoc.epoch-record\0v1\0`, `dynodoc.epoch-record-signature\0v1\0` | Epoch record hash and signature |
| `dynodoc.membership-log\0v1\0`, `dynodoc.membership-log-signature\0v1\0` | Membership log entry hash and signature |
| `dynodoc.project-commit\0v4\0`, `dynodoc.project-commit-signature\0v4\0` | Commit format v4 (protocol document) |
| `dynodoc.recovery.v1`, `dynodoc.escrow.v1` | Recovery and escrow key derivations (section 11) |

Rule, taken from the existing code review practice: a label is never reused for a second
purpose, and a new message type gets a new label even if its fields look similar. The
device-session and device-enrolment domains in `repository/device_session.rs` and
`repository/device.rs` already enforce this by test (`assert!(!message.starts_with(...))`).

## 5. Identities and device encryption keys

### 5.1 What exists

A device holds an Ed25519 signing key. Enrolment (`repository/device.rs`) signs a server
challenge `dynodoc.device-enrolment\0v1\0 || canonical_json(challenge)`; the service binds the
public key to the account in `device_signing_key` (migration 0033) and records revocation in
`device_key_revocation`. Schema 40 adds per-project grants of that key (authorization, not
cryptographic access). Commits are signed with the same key (`SignedCommit::sign`).

### 5.2 What is added

Each device also holds an X25519 **device encryption key** (`DEK`). It is bound to the signing
key by a signature, so a recipient set can be built from keys the *account* vouched for:

```
binding_message = "dynodoc.device-encryption-binding\0v1\0"
               || canonical_json({
                    "version": 1,
                    "authority": "<installation uuid>",
                    "identity": "<account uuid>",
                    "signing_key": "<ed25519 public hex>",
                    "encryption_key": "<x25519 public hex>",
                    "created_at_ms": 1791000000000
                  })
binding_signature = Ed25519.sign(DSK, binding_message)
```

The service stores the binding in a new append-only table (protocol document, section 5). It
verifies the signature and that the signing key is an active key of the authenticated
account, exactly like the existing enrolment path. A revoked signing key invalidates its
encryption key for new grants; existing grants stay in history.

### 5.3 What the binding does and does not prove

It proves that whoever controls `DSK` also controls `DEK`. It does not prove the person is who
the directory says: the service chooses which signing key it presents for an account. The
substitution attack and its mitigations (out-of-band fingerprints, organisation-signed
directory, change alerts) are in [the threat model](E2EE-THREAT-MODEL-AND-ASSURANCE.md).

### 5.4 Fingerprints

A device fingerprint is `SHA-256("dynodoc.device-fingerprint\0v1\0" || signing_key ||
encryption_key)`, shown as 60 decimal digits in 12 groups of 5 (Signal-style safety number
presentation) and as a QR payload. Users and organisations can compare fingerprints out of
band. A client records the first fingerprint it sees for each member (trust on first use) and
raises a blocking warning when it changes.

## 6. Key epochs and rotation

### 6.1 Definitions

- An **epoch** is a period during which one set of members and devices holds one `PER_e`.
- The project starts at epoch 1 when encryption is enabled (or at epoch 0 for plaintext
  history that predates encryption, see the protocol document).
- Every sealed object records its epoch (in the header and the AAD). Epochs only increase.
- An **epoch record** is a signed statement that epoch `e` exists, who may read it and why it
  was opened.

### 6.2 Triggers

| Trigger | New epoch? | Notes |
|---|---|---|
| Project created in encrypted mode | Epoch 1 | |
| Member removed | Yes, required | Remaining members' devices get `PER_{e+1}`; the removed member gets nothing |
| Member's role lowered, still a member | No | Confidentiality is unchanged; write authority is enforced by the server and by signature checks (section 10) |
| Device revoked or reported lost | Yes, required | The account is still a member but one holder of `PER_e` must not receive `PER_{e+1}` |
| Member added | No by default | The new member receives grants for the epochs allowed by the history policy. A new epoch is optional hygiene |
| Agent delegation granted or expired | No | An agent holds a `K_doc`, which is bound to epoch `e`; it stops working at the next epoch, which is an acceptable expiry |
| Scheduled rotation | Optional policy (for example 90 or 180 days) | Limits exposure from slow key leakage |
| Suspected compromise | Yes, immediately | Plus device revocation and review of the membership log |
| Escrow recipient changed | Yes | So the former escrow cannot read new content |

### 6.3 Opening an epoch (procedure)

An authorized member (owner or manager of the project, role enforced by the server and checked
by clients against the membership log) performs these steps on a client:

1. Fetch and verify the membership log up to its head. Build the **recipient set**: every
   active, non-revoked device encryption key of every current member, plus the recovery
   recipients members registered, plus the escrow recipient if policy requires one.
2. Draw `PER_{e+1}` from the CSPRNG. For each explicit-scope document `d` with its own access
   list, draw `DR_{e+1,d}`.
3. For each recipient and each scope it is entitled to, create a **key grant** (section 7).
4. Compute `recipients_digest = SHA-256(sorted list of (recipient_key, grant_hash))` and build
   the **epoch record** (section 6.4). Sign it with `DSK`.
5. Submit record and grants in one request. The service stores them atomically, verifies the
   signature and the author's role and rejects an epoch number that is not exactly `e+1`
   (compare-and-swap, the same discipline as ref updates in `project_history.rs::advance`).
6. Clients receiving the new epoch verify the record chain, then decrypt their grant.

Cost: one X25519 operation and one HKDF per recipient. A project with 50 members and 120
devices costs about 120 grants of roughly 0.6 KiB, so about 70 KiB per epoch. This is cheap
enough that rotation on every removal is the default.

### 6.4 Epoch record

```json
{
  "version": 1,
  "project": "7c1e5a52-0b7a-4c2e-9d0a-4e1b0f7b2a10",
  "epoch": 4,
  "previous_epoch_record": "e0f3...9a2c",
  "reason": "member_removed",
  "suite": 1,
  "key_id": "085428cdbf120f89",
  "recipients_digest": "5b1c...77de",
  "membership_head": "9f21...c004",
  "history_policy": "full",
  "created_by_key": "ab12...ef90",
  "created_at_ms": 1791000000000
}
```

The record hash is `SHA-256("dynodoc.epoch-record\0v1\0" || canonical_json(record))`. The
signature is Ed25519 over `"dynodoc.epoch-record-signature\0v1\0" || record_hash`.
`membership_head` binds the record to the membership log position the author saw, so the log
cannot be rewound under an epoch without detection.

### 6.5 History policy

| Policy | Grants a new member receives | Intended use |
|---|---|---|
| `full` | One grant per epoch from 1 to the current one | Collaboration where the whole history matters (default) |
| `from_join` | Only the current epoch and later | Compliance cases where a new member must not read earlier drafts. Requires a **re-seal commit** of the live state under the current epoch first, because a state root references objects sealed in older epochs (see below) |

The policy is per member, recorded in the membership log entry that adds them, and visible to
that member. A `from_join` member can still read earlier content *if another member shares it
with them*; the cryptography limits what the service and the admin tool can hand out, not what
members tell each other.

**Mixed-epoch states.** A state root is a set of object addresses, and an unchanged node keeps
the object (and the epoch) it was sealed under. The live state of a document at epoch 4 can
therefore contain objects from epochs 1 to 4. A reader needs the epoch keys of every epoch
that appears in the state it opens. Consequences: `full` history is the simple default;
`from_join` is only meaningful after the live state has been re-sealed under the current epoch
(one commit that rewrites every live node of the files the member may read). That re-seal is
O(live state), is performed by a manager's client, and is recorded in the membership log
(`reseal_live_state`). Phase 1 ships `full` only; `from_join` is a Phase 1.5 item (OQ-CR-7).

### 6.6 What rotation does not do

- It does not re-encrypt old objects. Removed members and thieves of old backups can still read
  the epochs they held. Re-encryption of history would help only against a removed member who
  did *not* already copy the content, which is an unrealistic assumption. The master design
  states this limitation in the user-facing copy.
- It does not revoke content already decrypted on a device.
- It does not by itself stop a removed member from *writing*: that is the server's role check
  and the signature verification in section 10.

### 6.7 Optional re-encryption

For the narrow case of an organisation that must rid the service of content readable with a
leaked epoch key (for example a leaked recovery key), an administrator can run a **re-seal
job** from an authorized client: read each object, seal under the current epoch, and publish a
migration commit that re-points state roots. The old objects are then removed by the existing
audited erasure path. This is expensive (all retained history) and is specified in the protocol
document as a Phase 3 tool; it is not part of normal rotation.

## 7. HPKE usage: key grants

### 7.1 Grant structure

A key grant delivers one 32-byte secret (`PER_e` or `DR_{e,d}`) to one device encryption key.

```json
{
  "version": 1,
  "project": "7c1e5a52-0b7a-4c2e-9d0a-4e1b0f7b2a10",
  "epoch": 4,
  "scope": { "kind": "project" },
  "recipient_device": "1d4e8f3a-6b2c-4a4d-8c10-3f7b9a2e5d11",
  "recipient_key": "cf8e...19a0",
  "suite": { "kem": 32, "kdf": 1, "aead": 2 },
  "enc": "04d1...e7c3",
  "ciphertext": "9a77...b015",
  "epoch_record": "d3a0...41f7",
  "created_by_key": "ab12...ef90",
  "created_at_ms": 1791000000000,
  "signature": "6e02...c9b1"
}
```

Field notes: `scope` is `{"kind":"project"}` or `{"kind":"document","document":"<uuid>"}`.
`recipient_key` is the 32-byte X25519 public key in lowercase hex. `enc` is the 32-byte HPKE
encapsulated key (hex). `ciphertext` is the 48-byte HPKE sealed payload (hex): 32 bytes of
secret plus a 16-byte tag. Numeric suite codes are the RFC 9180 codepoints: KEM 0x0020, KDF
0x0001, AEAD 0x0002.

### 7.2 HPKE invocation

```
info = "dynodoc.key-grant\0v1\0"
    || project_uuid(16) || u32_be(epoch)
    || scope_tag(1) || (document_uuid(16) if scope = document)
    || recipient_device_uuid(16)

aad  = SHA-256(canonical_json(grant fields except enc, ciphertext, signature))

(enc, ciphertext) = HPKE.SealBase(pkR = recipient_key, info, aad, pt = secret)
signature = Ed25519.sign(DSK_creator,
              "dynodoc.key-grant-signature\0v1\0" || SHA-256(canonical_json(grant incl. enc, ciphertext)))
```

Base mode is used, not Auth mode, because the grant needs a signature that third parties (the
membership log verifiers, auditors) can check later; an HPKE Auth-mode tag would be checkable
only by the recipient. The signature binds the creator, the recipient, the epoch and the
epoch record. The `info` string binds the grant to its project, epoch, scope and recipient
device so that a grant cannot be replayed to a different device or epoch.

### 7.3 Recipient processing

1. Verify the grant signature with the creator's signing key, taken from the **verified**
   membership log, not from the service's response.
2. Check that the creator was authorized to open epochs at the log position named by the epoch
   record.
3. Check that `epoch_record` matches a record in the verified record chain with the same
   `epoch`, `recipients_digest` inclusion and `suite`.
4. `HPKE.OpenBase`; on success check that `SHA-256("dynodoc.key-id.v1\0" || u32_be(epoch) ||
   secret)[0..8]` equals the epoch record's `key_id`. A mismatch means a malformed grant.

### 7.4 Fallback without an HPKE library

If an HPKE library fails review, the same grant can be built directly: ephemeral X25519,
`HKDF-SHA256(shared || enc || recipient_key, info)` to a 32-byte key and a 12-byte nonce, then
AES-256-GCM. This is what HPKE base mode with this suite computes, with RFC 9180's labelled
extract and expand. The design keeps the wire structure identical (`enc`, `ciphertext`), so
swapping implementations is a library change, not a protocol change. Conformance to RFC 9180
test vectors is a release gate either way.

### 7.5 Grant for delegated keys

A delegation grant (agent or guest) carries document keys, not `PER_e`. Because a document's
live state can contain objects from several epochs (6.5), the payload is a list of
`(epoch, K_doc_{epoch,d})` pairs, one per epoch that appears in the document's state, not a
single key; the HPKE plaintext is the canonical JSON encoding of that list (still under 1 KiB
for typical documents). It uses scope `document`, an additional `delegation` object
`{ "expires_at_ms": ..., "purpose": "agent" | "guest" , "read_only": true }`, and the
signature covers it. `read_only` is advisory for
honest clients: a holder of a symmetric document key can also seal objects. Write authority is
enforced by signatures and server role checks (section 10), which is why an agent's output
arrives as a *proposal* signed by the agent's own device key and subject to review.

## 8. Sealing: authenticated encryption with bound context

### 8.1 Container format (proposed, version 1)

A sealed object has a fixed binary header followed by ciphertext and tag. The style matches the
existing content objects (`DYNOCNT\0`, fixed offsets, big-endian integers in
`shared_checkpoint/content.rs`) rather than JSON, because ciphertext in JSON would add base64
overhead and a second canonicalization problem.

| Offset | Size | Field | Notes |
|---:|---:|---|---|
| 0 | 8 | `magic` | ASCII `DYNOSEAL` |
| 8 | 1 | `format_version` | `0x01` |
| 9 | 1 | `suite` | `0x01` (section 3.1) |
| 10 | 1 | `kind` | See 8.2 |
| 11 | 1 | `flags` | bit 0 compressed before encryption; bit 1 reserved zero; all other bits must be zero |
| 12 | 4 | `epoch` | u32 big-endian |
| 16 | 8 | `key_id` | Hint: first 8 bytes of `SHA-256("dynodoc.key-id.v1\0" || u32_be(epoch) || epoch_root)`; selects among stored epoch keys, not authenticated alone |
| 24 | 32 | `salt` | Random per object |
| 56 | 16 | `key_commitment` | See 8.5 |
| 72 | 2 | `ref_count` | u16 big-endian, at most 4096 |
| 74 | 32 x `ref_count` | `refs` | Raw 32-byte addresses of child objects, strictly ascending, unique. In the clear on purpose (8.7) |
| 74 + 32n | P | `ciphertext` | `P` = padded plaintext length |
| 74 + 32n + P | 16 | `tag` | AES-GCM tag |

Total size is `90 + 32n + P` bytes. A leaf object with a 256-byte padded plaintext is 346
bytes. The address of a sealed object is
`SHA-256("dynodoc.sealed-object\0v1\0" || all bytes)`, written as 64 lowercase hexadecimal
characters (the existing `Address` type). A server can verify the address, the header
syntax, size limits and the reference list without any key.

Equivalent deterministic CBOR header (RFC 8949 section 4.2) for implementers who prefer it;
the byte layout above is the proposal of record and the two must not both ship:

```
{ 0: "DYNOSEAL", 1: 1, 2: 1, 3: 1, 4: 0, 5: 4, 6: h'085428cdbf120f89',
  7: h'c0c1..cecf', 8: h'd9d3eefc6923e82b1015ea358c1e2c84', 9: [ h'..32 bytes..' ],
  10: h'..ciphertext||tag..' }
```

### 8.2 Object kinds

| Kind | Name | Plaintext content | Typical size |
|---:|---|---|---|
| 1 | `node_fields` | Canonical JSON of a node's `current_fields` (content, label, expressions, `var_name`) | 0.1 to 32 KiB |
| 2 | `text_chunk` | Up to 64 KiB of one large string, in the existing chunk-tree shape | 64 KiB |
| 3 | `comment` | Body of one comment, reply or review note | up to 10 KB |
| 4 | `suggestion` | `detail` of a suggestion (a wrapped operation) | up to 64 KiB |
| 5 | `meta` | Title, file name, version name, note, label, issue text, commit message, path map (document UUID to path) | up to 512 KiB for a path map |
| 6 | `asset_chunk` | 64 KiB of an original upload or embedded media | 64 KiB |
| 7 | `bundle` | A provenance or change-request bundle body | up to 16 MiB split into chunks |
| 8 | `event` | The sealed part of a centralized event payload | up to 64 KiB |
| 9 | `state_index` | Optional: a sealed index node when structure itself is sealed (Phase 3 option) | up to 32 KiB |

### 8.3 Associated data

The AAD is length-framed in the same way as `log.rs::feed` (a `u64` big-endian length, then the
bytes), which makes the concatenation unambiguous:

```
AAD = LP("dynodoc.seal.v1")
   || LP(project_uuid, 16 bytes)
   || LP(document_uuid, 16 bytes)         // 16 zero bytes for project-level kinds
   || LP(node_id, UTF-8)                  // empty string when the object is not node-bound
   || LP(kind, 1 byte)
   || u32_be(epoch)
   || u64_be(revision)                    // node revision, or event seq, or 0
   || LP(suite, 1 byte)
   || LP(refs_digest, 32 bytes)           // SHA-256 over the concatenated raw refs
```

What each binding buys:

| Bound value | Attack it prevents |
|---|---|
| Project and document UUID | The service (or another project's member) presents project A's ciphertext as project B's. The key differs per epoch root anyway; the binding also covers explicit document roots and agent keys. |
| Node ID | The service swaps two nodes' sealed fields in a response it assembles (for example a materialized view). |
| Kind | A comment ciphertext presented as node fields, or a chunk as a node. |
| Epoch | Replay of an object under a different epoch's key set; downgrade of a recipient set. |
| Revision | A stale version of the same node is re-presented where a newer one is expected in an unsigned context (a draft view, a server-built state). In signed commits the state root already fixes object addresses; the revision makes the check also work for objects fetched individually. |
| Suite | Downgrade to a weaker suite when more than one exists. |
| `refs_digest` | Substitution of the child reference list, which is in the clear and therefore not covered by the ciphertext. |

For centralized events (`kind` 8) the author does not know the final `seq` when sealing: the
service assigns it under the document lock (`log::append_in_tx`). The `node_id` field of the
AAD therefore carries a client-chosen event ULID, the `revision` field carries the author's
`base_seq` (the head the client saw), and the document UUID carries the document. The service
rejects a repeated client event ULID, and clients de-duplicate on it (section 10.4).

### 8.4 Per-object keys and the nonce strategy

```
okm  = HKDF-SHA256( IKM  = K_doc_{e,d}  (or K_meta_e for project-level kinds, DR_{e,d} for
                                          explicit-scope documents),
                    salt = object salt (32 random bytes),
                    info = "dynodoc.seal.v1/aead+nonce+commit\0" || suite,
                    L    = 76 )
key        = okm[0..32]
nonce      = okm[32..44]
commit_key = okm[44..76]
```

**Why a derived, single-use key and nonce.** AES-GCM with a 96-bit random nonce under one key
is limited to about 2^32 messages (NIST SP 800-38D). A project can exceed that over many
years, across many devices that cannot coordinate a counter offline. Deriving a fresh key from
a fresh 256-bit salt gives each object its own key and nonce, so the limit applies to a key
that is used once. The probability that two objects share a salt is negligible (2^-256 per
pair). The cost is one HKDF per object, which is cheap in WebCrypto and `ring`.

**Misuse resistance.** If a client's random number generator is broken and repeats a salt, two
objects encrypted under the same key and nonce leak the XOR of their plaintexts. The design
does not use a nonce-misuse-resistant mode (AES-GCM-SIV is not in WebCrypto). Clients check
`getRandomValues` at start-up (section 3.6) and the object address makes accidental salt reuse
detectable server-side: two different objects with the same salt prefix can be flagged without
decryption (a metric, not a control).

### 8.5 Key commitment

AES-GCM and ChaCha20-Poly1305 are not key-committing: an adversary can craft a ciphertext that
decrypts to different valid plaintexts under two different keys ("invisible salamanders",
Dodis, Grubbs, Ristenpart and Woodage, CRYPTO 2018; "partitioning oracle attacks", Len, Grubbs
and Ristenpart, USENIX Security 2021). This matters where a single ciphertext is read under
more than one key: for example when a member holds several epoch keys and tries them in turn,
or when a delegated `K_doc` and the project key both open a file. The commitment makes the
choice unambiguous:

```
key_commitment = HMAC-SHA-256( commit_key, AAD )[0..16]
```

A reader derives `key`, `nonce` and `commit_key`, recomputes the commitment and compares it in
constant time **before** attempting AES-GCM decryption. A mismatch is a hard failure and is not
retried under another key. This costs one HMAC and 16 bytes per object and closes the
multi-key ambiguity without leaving the WebCrypto primitive set.

### 8.6 Padding

Plaintext is framed as `u32_be(body_length) || body || zero padding` and padded to the next
bucket from {128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536} bytes. Larger
objects are not padded beyond their chunk (they are already 64 KiB). The reader checks the
length prefix and that every padding byte is zero.

| Aspect | Value |
|---|---|
| Overhead for the paragraph in appendix A | 256-byte bucket for 207 bytes of framed body: 49 bytes of padding (about 24 percent of the unpadded plaintext) |
| Expected overhead for prose paragraphs | 10 to 25 percent of node-field bytes; small against `pos`, headers and index objects |
| What it hides | The exact length of short texts (a changed digit, a one-word edit) within a bucket |
| What it does not hide | Rough size class, number of objects, and that a node changed at all |

A stronger option is Padme (Nikitin et al., "Reducing metadata leakage from encrypted files
and communication with PURBs", PoPETs 2019), which bounds overhead at about 12 percent for
large objects. Because object sizes are bounded by 64 KiB chunks, power-of-two buckets are
sufficient for Phase 1.

### 8.7 Child references stay in the clear

The container lists child addresses in the header, outside the ciphertext. This is deliberate:

- The service must walk the object graph for reachability, retention planning, transfer
  negotiation and audited erasure (`shared_checkpoint::references`, `repository/local/retention.rs`,
  the `project_published_object` index in `0036_shared_project_history.sql`).
- `refs_digest` in the AAD prevents the service from editing the list.
- The leak is graph shape (how many children, which objects are shared), which the service
  can already infer from object sizes and upload order.

Where structure itself is sensitive, kind 9 sealed index nodes (a Phase 3 option) hide the
child lists, at the price that the service can no longer plan garbage collection for that
project and erasure becomes whole-project only.

### 8.8 Optional compression

Compression before encryption reveals plaintext length patterns and enables chosen-plaintext
leaks (CRIME and BREACH style) if an attacker can influence part of a compressed message and
observe sizes. Phase 1 therefore **does not compress** node fields or comments. Asset chunks
(already compressed Office archives) are stored as is. Flag bit 0 is reserved for large
`bundle` objects, compressed per object with no attacker-controlled content mixed in.

### 8.9 Open procedure (reader)

1. Parse the header; reject unknown version, suite, kind, flags, `ref_count` over 4096 or total
   size over 1 MiB.
2. Verify the address if the object was fetched by address.
3. Select the key by `epoch` and `key_id`; if the device holds no grant, report "not
   readable on this device" (a normal state, not an error).
4. Derive `okm`; recompute and compare the commitment; then AES-256-GCM decrypt with the AAD
   rebuilt from **the reader's own knowledge** of project, document, node, kind, revision and
   `refs`, never from fields the service supplied separately.
5. Check padding and the plaintext frame; parse canonical JSON; check the inner `cid` against a
   recomputed keyed identifier (section 9).

## 9. Keyed content identifiers

### 9.1 Problem

Today an address is `SHA-256(domain || bytes)` of plaintext-derived bytes. With sealed storage
an unkeyed hash of the plaintext anywhere the service can read becomes a **confirmation
oracle**: anyone who can guess a paragraph can hash it and test for equality. The same holds
for the rich-text patch pre/post fingerprints (`richtext::fingerprint`, `RichTextPatch.before`
and `.after`) and for the copy-detection fingerprints in `migrations/0021_*` if they were
computed over plaintext.

### 9.2 Definition

```
cid = HMAC-SHA-256( K_cid_e,  "dynodoc.cid.v1\0" || kind || canonical_plaintext )
```

- `K_cid_e` is derived from the epoch root (section 4), so only members can compute it.
- `canonical_plaintext` is the canonical JSON bytes of the object's content before framing and
  padding (for kind 1, the `fields` object, as in appendix A).
- By default the `cid` lives **inside** the sealed plaintext: members use it for change
  detection ("is this node's content the same as in the previous revision?"), for local
  deduplication and as the stable fingerprint in rich-text patch preconditions. The service
  never sees it.

### 9.3 Optional exposure of equality

An organisation may choose a *deduplication* mode that places a truncated `cid` (16 bytes) in
the sealed object header so that the service can skip uploading an object whose `cid` it
already holds. This reveals equality of contents among the project's objects to the service
(and nothing else). It is off by default; enabling it is a recorded policy change in the
membership log. Across projects the identifier differs because `K_cid_e` differs, so no
cross-project or cross-tenant oracle exists.

### 9.4 Interaction with the existing object store

`PgStore::put_batch` in `crates/engine-core/src/shared_checkpoint/postgres.rs` compares stored
bytes with incoming bytes for an existing address and returns `HashMismatch` if they differ.
With random salts the same plaintext produces different bytes and therefore a different
address, so the collision case cannot arise for sealed objects; unchanged nodes are reused by
**referencing the old address**, which clients already know. Reuse needs no convergent
encryption. The same plaintext written twice under one epoch becomes two objects, which is the
correct, leak-minimising behaviour.

## 10. Signatures over ciphertext

### 10.1 What is signed today and what changes

| Artifact | Signed or hashed today | Over | In an encrypted project |
|---|---|---|---|
| Event | `content_hash` over type, target node, canonical payload JSON, actor; `chain_hash` links | Payload JSON (plaintext) | The payload JSON is the **sealed form**; hashes cover ciphertext and header. Chain verification is unchanged |
| Typed checkpoint object | Address = SHA-256 over magic and canonical JSON | Plaintext JSON | Structural objects (nodes, indexes) keep this form with new codec v3; payload objects are sealed objects addressed by hash over ciphertext |
| Project commit | Ed25519 over the commit address; the commit lists file state roots and carries a plaintext `message` and plaintext paths as map keys | Canonical commit JSON | Commit v4: map keys become document UUIDs, `message` and the path map are sealed envelopes carried inline; the signature covers the same fields, hence the ciphertext |
| Ref receipt | Ed25519 by the receipt authority over commit address and generation | Addresses only | Unchanged |
| Review | Server row; decision plus candidate address | Addresses only | Unchanged |

### 10.2 Why this works without keys

A hash over a sealed object commits to the ciphertext, the header (so to the epoch, kind and
refs) and the salt. A signature over a state root commits transitively to every sealed object
below it. Any observer can verify authorship, ordering and integrity of the ciphertext graph
using the public key in the commit. Only a keyholder can verify that the plaintext is
well-formed, and the AAD binds the plaintext to its position.

### 10.3 What a signature does not give in this design

- A signature over ciphertext does not prove the signer knew the plaintext. A member could
  sign (and publish) a commit that contains an object they copied from elsewhere. That is
  true of any signed commit and is acceptable: authorship means "this key published this".
- Because AES-GCM is not key-committing, the commitment in 8.5 is what ties a ciphertext to
  one key. Without it a signed commit could point at an object that decrypts differently under
  two members' keys.

### 10.4 Anti-rollback, fork and replay

| Attack | Control | Existing or new |
|---|---|---|
| Rollback of a branch to an older commit | Ref receipts with generation and `previous` link; client pins (`attestation.rs` `Pin::advance`) | Existing (authority not installed in production) |
| Presenting different histories to different members (fork) | Commits embed the membership-log head and epoch; clients exchange heads through receipts and optional witnesses | Existing receipts plus new embedding |
| Replay of an old event into a new chain position | `chain_hash` link (the service assigns position); client event ULID in the AAD and duplicate rejection | Existing chain plus new AAD field |
| Epoch confusion (use an old epoch's key set for new content) | Epoch in header and AAD; epoch record chain verified | New |
| Removed member continues to publish | Server role check (`project_history.rs::authorize`, `devices::current_key`); clients verify the signer was an active member at the commit's `membership_head` | Existing server check plus new client check |
| Server inserts a rogue member key | Membership log is signed by owners; clients verify the log chain, show fingerprints and alert on change | New |

### 10.5 Commit v4 signing

Commit v4 follows the v1 to v3 pattern in `crates/engine-core/src/repository.rs`: a new
address domain `dynodoc.project-commit\0v4\0` and signature domain
`dynodoc.project-commit-signature\0v4\0`, so no v4 signature can be mistaken for an earlier
format and old readers reject it (the same rule used when v3 was added). The new signed
fields are the epoch, the membership-log head and the sealed message and path references.
The byte-level change list is in [the protocol document](E2EE-PROTOCOL-AND-MIGRATION.md).

## 11. Recovery and escrow

### 11.1 Layers

Recovery is a set of independent paths, any of which can restore a user's access. A project
requires at least one path beyond "another device of the same account" before it can be
switched to encrypted mode with a sole owner (section 11.5).

| Path | Who can use it | Server role | Visible to members |
|---|---|---|---|
| 1. Another enrolled device of the same account | The user | Relays grants | No special notice |
| 2. User recovery key `RK` | The user, with the printed or stored secret | Relays grants | Log entry `device_add` signed by the recovery signer |
| 3. Another project member re-grants | Any member with grant authority, after out-of-band identity check | Relays grants | Log entry `device_add` signed by that member |
| 4. Organisation escrow | Organisation administrators, with quorum if configured | Relays grants | Persistent banner "This project can be recovered by your organisation", log entry `escrow_recover` |
| 5. None (explicit choice) | Nobody | None | Warning at project creation |

### 11.2 User recovery key

At setup the client generates `RK`, 32 random bytes, displayed once as 52 Crockford base32
characters in 13 groups of 4 with a 2-character check value (typo detection, not security).

```
seed       = HKDF-SHA256( IKM = RK, salt = "", info = "dynodoc.recovery.v1/seed", L = 32 )
rk_x25519  = X25519 key pair with private key HKDF-Expand(seed, "x25519", 32)  (clamped by X25519)
rk_ed25519 = Ed25519 key pair with seed        HKDF-Expand(seed, "ed25519", 32)
```

The client registers both public keys in the membership log of each project as a **recovery
recipient** and a **recovery signer** (entry type `recovery_set`, signed by the user's `DSK`).
Whoever opens an epoch then includes the recovery recipient in the recipient set like any
device. To recover: a new device enrols normally (login plus `DSK`), the user types `RK`, the
client derives the recovery keys, fetches grants addressed to the recovery recipient, opens
them, signs a `device_add` entry for the new device with the recovery signer, and issues
grants to the new device for every epoch it can now open. The service never holds `RK`.

Hardware-bound variant (Phase 3): `RK` is replaced or wrapped by a secret derived from a
WebAuthn PRF or FIDO2 hmac-secret output (clients document, section 9).

### 11.3 Organisation escrow

| Element | Specification |
|---|---|
| Key | An X25519 organisation escrow recipient `OER`. Private half in a customer-controlled HSM or KMS, or split with Shamir secret sharing among named custodians (threshold t of n, ceremony tooling in Phase 3) |
| Registration | Project log entry `escrow_set` signed by the project owner and countersigned by the organisation administrator's `DSK`. Organisation policy may require it at project creation |
| Coverage | Every epoch opened after registration includes a grant to `OER`; earlier epochs are covered only if the owner backfills grants (a deliberate `escrow_backfill` entry) |
| Use | An administrator performs `escrow_recover`: obtain grants addressed to `OER`, open them using the HSM or quorum, re-seal to the user's new device, and sign a log entry `escrow_recover` naming user, device and ticket reference |
| Visibility | Epoch records list the escrow recipient in `recipients_digest`; clients show a persistent notice; every use appears in the signed log and in the service audit |
| Removal | `escrow_clear` entry plus a new epoch; earlier epochs stay readable by the escrow holder |

Escrow is a choice about **who the trusted parties are**. With escrow, the organisation's
administrators (and anyone who compromises the escrow key) can read the project. Without it,
loss of every member key means loss of the content. The product must make this trade-off
explicit and never enable escrow silently. The [master design](END-TO-END-ENCRYPTION.md)
records it as an organisation-level policy that members can see.

### 11.4 Local key protection

Key material on a device is stored by platform capability (clients document, section 3 and
7). For files the following format is proposed for `engine-cli` and for exported backups:

| Item | Proposal |
|---|---|
| Container | Extend the working-tree `dynodoc-key-v1` container (`crates/engine-cli/src/keyfile.rs`: format id, KDF parameters, key-store binding and public key bound as associated data; ChaCha20-Poly1305) to a `dynodoc-key-v2` that can carry the signing seed **and** the X25519 encryption key. `age` is the alternative if reuse of an externally reviewed format is preferred (OQ-CR-5) |
| Passphrase KDF | `dynodoc-key-v1` uses PBKDF2-HMAC-SHA256 with at least 600,000 iterations. PBKDF2 is not memory-hard, so offline guessing against a stolen file is cheaper on GPUs than with scrypt (N=2^17, r=8, p=1) or Argon2id (m=64 MiB, t=3, p=1). Because a file that opens an encrypted project exposes far more than one signature key, v2 should use a memory-hard KDF and keep PBKDF2 only as a read-compatibility path |
| Contents | `DSK` seed, `DEK` private key; cached epoch roots are not stored (re-derived from grants) |
| Browser | `CryptoKey` objects created non-extractable and stored in IndexedDB; a passphrase-wrapped extractable backup is optional and user-initiated |

### 11.5 Lost-key matrix

| Situation | Outcome | Action |
|---|---|---|
| One device lost, others enrolled | No loss | Revoke lost device (schema 33 revocation), open new epoch, re-grant |
| All devices lost, recovery key available | No loss | Recovery flow (11.2) |
| All devices lost, no recovery key, escrow present | No loss, organisation-visible | Escrow flow (11.3) |
| All devices lost, no recovery key, no escrow, other members exist | No loss; depends on another member | Member verifies identity out of band and re-grants (path 3) |
| Sole member, no recovery path, all devices lost | Content unrecoverable. The service holds only ciphertext and cannot help | Only deletion is possible |
| Passphrase for local key file forgotten | As device lost | As above |
| Organisation owner leaves | No loss if ownership was transferred before offboarding; otherwise escrow | `owner_transfer` entry signed by the old owner and accepted by the new |

The project creation flow must refuse to enable encrypted mode for a project whose only
member has no recovery key and no escrow, unless the user acknowledges "I understand the
service cannot recover this content" by typing the project name.

### 11.6 Crypto-erasure

The service already supports audited whole-document erasure (`migrations/0017_*`,
`/admin/erase`). For encrypted projects erasure keeps working at the ciphertext level: rows
and objects are deleted. A second, stronger meaning becomes available where the **service
holds the only copies of the grants**: deleting all grants for an epoch (and the explicit
document roots) makes that epoch's ciphertext unreadable even in restored backups, as long as
no member device still holds the epoch root. This is only an erasure guarantee for parties who
never cached keys; members' devices and exports are outside it. Product copy must say so.

## 12. What the construction does and does not protect

### 12.1 Properties

| Property | Delivered in Phase 1? | Notes |
|---|---|---|
| Confidentiality of content at rest, in backups and in server memory against the service | Yes, for sealed kinds, subject to the metadata inventory | Plaintext metadata remains (section 12.2) |
| Confidentiality against a removed member for content written after rotation | Yes | Not for content already seen or for epochs they hold |
| Integrity and authorship of ciphertext history | Yes | Existing hash chain, commit signatures, receipts |
| Ciphertext non-transplantation | Yes | AAD binding (8.3) |
| Membership integrity (service cannot silently add a reader) | Partly | Signed log and fingerprints; a user who never verifies is exposed to first-use substitution |
| Forward secrecy of past content | No, by design | 12.3 |
| Post-compromise security within an epoch | No | Rotation restores it at epoch granularity once the compromise is acted on |
| Metadata confidentiality | No | Membership, structure, timing, sizes, event types are visible |
| Resistance to a malicious client bundle | No in browsers without extra controls | Code delivery is the largest residual risk of web-based E2EE (threat model, AT3 in section 6) |
| Deniability | No | Commits are signed on purpose |
| Quantum resistance of grants | No | Harvest-now-decrypt-later applies to stored grants (12.4) |
| Availability against a malicious service | No | The service can delete or withhold ciphertext; clients can detect missing updates through pinned receipts only |

### 12.2 What the service still learns (Phase 1 summary)

Account and membership relationships; project and document identifiers; number of nodes, their
types (`paragraph`, `item`, ...), parent links, sibling order keys (`pos`), deletion state;
timestamps; event types (`FieldEdited`, `RichTextPatched`, ...); object sizes after padding
bucket; the commit graph and who published each commit; review decisions; which devices
accessed which objects when. The full inventory with mitigations is in
[the threat model](E2EE-THREAT-MODEL-AND-ASSURANCE.md#5-metadata-leakage-inventory).

### 12.3 Why forward secrecy is not a goal

Messaging systems delete message keys after use so that a later key compromise cannot decrypt
past traffic. For a provenance archive the opposite is required: members must be able to open
the same revision in five years, a new collaborator must be able to read the history, and an
auditor must be able to verify a historical state. Deleting keys would defeat the product.
What is wanted instead is *access revocation for the future*, which epochs provide.

The consequence must be stated: a thief of one device's `DEK` and the grants held on the
service can decrypt every epoch that device was a recipient of. Controls are device
protection, short-lived sessions, revocation and rotation, not ratcheting.

### 12.4 Post-quantum position

Content under AES-256 is expected to remain secure against a large quantum computer (Grover
reduces effective strength to about 128 bits). The exposed element is the **grant**: an X25519
shared secret protects `PER_e`, and grants are stored on the service for years. An adversary
who stores grants and ciphertext today and later obtains a cryptographically relevant quantum
computer could recover epoch roots. Phase 1 accepts this risk and states it. Mitigation path:
a hybrid KEM suite (X25519 combined with ML-KEM-768, for example X-Wing) as suite 0x10, adopted
when HPKE codepoints and audited libraries exist. `@hpke/hybridkem-x-wing` and `@hpke/ml-kem`
are labelled experimental and not standardised as of 14 October 2026; `hpke` and `hpke-rs`
list ML-KEM or X-Wing support. The grant structure already carries a suite object, so a new
suite changes only the KEM and the `info` string, and a re-grant job can upgrade old epochs.

### 12.5 Agility and versioning rules

1. Every sealed object, grant, record and log entry has an explicit `version` and the object
   or its epoch record carries a `suite`. Readers reject unknown values; they never guess.
2. A new suite or format gets new domain labels (4.3).
3. Epoch records fix the suite for an epoch; a project may use different suites in different
   epochs. Upgrade is a new epoch plus an optional re-grant of old epochs.
4. Old readers must fail closed on `format_version`, `suite`, `flags` or unknown kinds. This is
   the same posture as typed checkpoint v2 and commit v3 ("old readers reject it").

## 13. Comparison with other designs and why a lighter construction

### 13.1 Comparison

| System | Core mechanism | What it optimises | Fit for Dynodoc | Verdict |
|---|---|---|---|---|
| Signal (X3DH or PQXDH, Double Ratchet) | Per-message ratcheting keys, pairwise sessions | Forward secrecy and post-compromise security for live conversations between two parties | Wrong lifetime model: deletes keys, expects ordered delivery, pairwise sessions per device pair | Not adopted |
| MLS, [RFC 9420](https://www.rfc-editor.org/info/rfc9420) (Proposed Standard) | TreeKEM group key agreement, epochs, commits, welcome messages, HPKE | Efficient forward-secret group keys for groups of 2 to thousands with asynchronous members | Closest match for groups and epochs. Costs: a delivery service with strict ordering, a directory and credential model, tree state per client, no mature in-browser library we could verify, and ratchet semantics that delete old secrets | Not adopted for Phase 1. Our epoch record plus HPKE grants is the subset of MLS we need. Revisit only if group sizes or real-time editing require it |
| age ([C2SP spec](https://github.com/C2SP/C2SP/blob/main/age.md)) | One file key, wrapped for recipients (X25519, scrypt), ChaCha20-Poly1305 STREAM payload | Simple, auditable file encryption to a recipient list | Good model for per-object wrapping and for local key files (11.4). It has no revocation or membership concept | Adopted for key files only; per-object design borrows the single-file-key idea |
| Matrix (Olm and Megolm) | Pairwise Olm sessions distribute Megolm room-session keys; ratchet per sender | Many-recipient room messages with optional key sharing for history | Shows the practical pain of key sharing for history and unverified devices; the server controls the device list unless users verify | Lessons applied (fingerprints, verified devices); mechanism not adopted |
| Tresorit-style zero-knowledge storage | Per-folder (tresor) symmetric keys, user key pairs, key rotation on revocation, directory of public keys | Shared cloud folders with revocation | The same shape as ours. Public details of their protocol should be checked from their published whitepaper before any comparison claim (unverified here) | Closest product analogue; our design is of this kind |
| CryptPad ([whitepaper](https://blog.cryptpad.fr/images/whitepaper.pdf)) | Keys in link fragment, per-document keys, server stores ciphertext only | Open-source E2EE real-time editors with honest-but-curious server model | Similar threat model; its real-time design (chainpad) is a different problem | Ideas reused: separate-origin editor frame, CSP, key not sent to server |
| Google Workspace client-side encryption ([overview](https://developers.google.com/workspace/cse)) | Per-file data keys wrapped by a customer-run external key service | Customer-held keys for Docs and Drive without client key management | Model for Phase 3 BYOK/HYOK: key service in the customer's control | Reference for Phase 3 |
| Microsoft Double Key Encryption ([FAQ](https://learn.microsoft.com/purview/double-key-encryption-faq)) | Two keys: one held by Microsoft, one by the customer | Content only readable with the customer key | Documented limits: desktop Office on Windows only; extra network endpoints | Reference for Phase 3; also a caution about host coverage |
| Proton Docs ([announcement](https://proton.me/blog/docs-proton-drive)) | E2EE editor in Proton Drive | Consumer and business E2EE documents | Public technical design not found; claims are product-level (unverified) | Market reference only |

### 13.2 Why a lighter construction is chosen

1. **Asynchronous, long-lived, multi-device data.** The unit of work is a signed commit pushed
   hours or days apart and read for years. Ratchets and one-time prekeys add state and failure
   modes (out-of-order delivery, skipped keys) for no benefit here.
2. **Group sizes are tens to low thousands, and membership changes are rare.** TreeKEM's
   logarithmic update cost matters at much larger sizes. A flat epoch with one HPKE grant per
   device costs well under 100 KiB for realistic projects.
3. **The server already mediates review, receipts and permissions.** A cryptographic
   membership protocol that duplicates those would create two sources of truth. A signed log
   that records what the server enforces is enough.
4. **Client constraints.** Apps Script has no AEAD or X25519 in its server runtime (clients
   document, section 6), Office task panes and browsers expose WebCrypto only. An
   all-WebCrypto construction with HKDF, AES-GCM, HMAC, Ed25519 and X25519 runs everywhere.
5. **Reviewability.** Two message types (sealed object, key grant), two signed record types
   (epoch record, membership log entry) and no negotiation can be reviewed by two people in
   weeks. A full MLS integration is a multi-quarter effort whose review cost exceeds the
   product's near-term value.
6. **Alignment with existing code.** The proposal reuses canonical JSON, domain labels,
   Ed25519 device keys, signed commits, CAS refs and immutable tables.

What is given up: forward secrecy, post-compromise security inside an epoch, and
interoperability with standard messaging clients. These are the right losses for this product
and are stated in the claims policy.

## 14. Test-vector plan

Policy, copied from the existing fixtures practice
([REF-RECEIPTS.md](REF-RECEIPTS.md), [DEVICE-SIGNING-KEYS.md](DEVICE-SIGNING-KEYS.md)):
vectors use public synthetic seeds, are produced by an **independent** implementation, and are
never regenerated to make a change pass.

| Vector file (proposed path under `crates/engine-core/tests/fixtures/`) | Content | Independent producer |
|---|---|---|
| `seal-v1.json` | Key hierarchy (epoch root to `K_doc`, `K_meta`, `K_cid`), per-object key block, header, ciphertext, tag, address for kinds 1 to 8; includes appendix A | Node.js WebCrypto script plus Python `cryptography` |
| `seal-v1-negative.json` | One case per AAD field changed, per header byte class flipped, wrong suite, wrong epoch, bad commitment, truncated tag, duplicate or unsorted refs, nonzero padding | Same |
| `grant-v1.json` | HPKE grants for RFC 9180 suite 0x0020/0x0001/0x0002 with project info strings; recipient processing outcomes | Python `hpke` or reference RFC 9180 vectors plus project wrapper |
| `epoch-record-v1.json`, `membership-log-v1.json` | Record chain, signature domains, accepted and rejected sequences (forked log, unauthorized signer, skipped epoch) | OpenSSL Ed25519 plus Python JSON |
| `commit-v4.json` | Address and signature of a sealed commit, including path-token derivation | OpenSSL plus Python |
| `recovery-v1.json` | `RK` to recipient and signer keys, base32 check value | Python |
| `padding-v1.json` | Bucket boundaries | Python |

Cross-implementation matrix, run in CI: Rust (`engine-core::seal`), TypeScript (WebCrypto in
Node 24 and in a Chromium, Firefox and WebKit test browser), the WASM build, and the Apps
Script sidebar code path (headless browser with the generated `Protocol.html` bundle).

Fuzz and property targets (assurance plan, section 10.5): header parser, container open
(mutated ciphertext, header and AAD inputs), canonical JSON plaintext parser, grant parser,
membership-log verifier and epoch-record chain verifier. Property: open(seal(x)) = x for all
kinds; any single-bit change to header, refs, ciphertext or tag fails closed; no open
succeeds with a different AAD field.

## 15. Operations

| Topic | Specification |
|---|---|
| Key custody | The service never holds `PER`, `DR`, `K_*`, `RK` or private encryption keys. Operators cannot recover them. The service stores grants, records and log entries |
| Library pinning | HPKE and X25519 crates and packages are pinned by exact version and hash; upgrades go through the vector suite and a dependency review |
| Metrics (counts only, no content) | Open failures by reason (commitment mismatch, no grant, unknown suite); epoch age per project; devices holding stale epochs; pending grants for invited members; recovery and escrow events |
| Alerts | Commitment mismatches above a threshold (possible tampering or a client bug); epoch older than policy; membership log verification failure reported by a client |
| Failure: client RNG failure | Self-test at start-up refuses to seal; objects already written are unaffected |
| Failure: grant missing for a device | Device shows "ask a member to grant access"; no data loss |
| Failure: HPKE library defect found | Epoch rotation re-wraps nothing already sealed (objects use `K_doc`, not HPKE); only grants are redone using the fallback in 7.4 |
| Failure: AES-GCM defect found | New suite and new epoch; old content re-sealed by the optional re-seal job (6.7) |
| Rollout | Feature flag per organisation, then per project; the server rejects sealed objects for projects not in encrypted mode and rejects plaintext content for projects that are |
| Backups | Contain ciphertext, grants and logs. Restoring a backup does not need or reveal keys; it also cannot restore keys that no member holds |

## 16. Delivery plan (cryptography workstream)

Scope of this table: the cryptographic modules, vectors and their tests. Server tables, API,
clients and UI are costed in the [protocol](E2EE-PROTOCOL-AND-MIGRATION.md) and
[clients](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md) documents; external review is costed in the
[assurance plan](E2EE-THREAT-MODEL-AND-ASSURANCE.md).

| # | Task | Output | Person-weeks | Depends on |
|---|---|---|---:|---|
| C1 | Specification freeze, domain registry, vector generation by independent implementations | This document at version 1.0, `seal-v1*.json`, `grant-v1.json` | 6 | Decision on OQ-CR-1 to 4 |
| C2 | Rust `engine-core::seal` module: container, AAD, key block, commitment, padding, cid | Crate module behind a feature, unit and property tests | 5 | C1 |
| C3 | X25519 and HPKE adapter trait with `hpke` crate backend and the 7.4 fallback | Trait, two backends, RFC 9180 vector run | 3 | C1 |
| C4 | TypeScript package (WebCrypto and `@hpke/core`) mirroring C2 and C3 | npm workspace package, Node and browser tests | 5 | C1 |
| C5 | Epoch record, membership log and grant verification (Rust and TS) | Verifiers, negative vectors | 6 | C2 to C4 |
| C6 | Recovery key and escrow primitives (no UI) | Derivation, `recovery-v1.json`, Shamir tool | 4 | C5 |
| C7 | Fuzzing harnesses and cross-implementation CI matrix | cargo-fuzz and Jazzer.js targets, CI jobs | 4 | C2 to C5 |
| C8 | Internal security review of design and code, fix cycle | Review notes, closed issues | 3 | C2 to C7 |
| | **Total** | | **36** | |

Assumptions: two engineers with prior Rust and WebCrypto experience, one security engineer at
half time, ten-person-week slack not included. Confidence: medium (range 29 to 48
person-weeks). Excludes: external cryptographic review and its remediation, FIPS validation,
HSM vendor integrations, user interface, server schema and API, performance tuning for
documents over 100 MB, and ongoing library maintenance.

Acceptance: all vectors pass in all four implementations; every negative vector fails closed;
fuzzers run 24 hours without a finding for each target; an engineer who did not write the code
reproduces the appendix A vector from this document alone.

## 17. Risks and mitigations

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | HPKE libraries are unaudited | High | Medium | Minimal usage, RFC vectors as gate, fallback construction (7.4), external review includes them |
| R2 | Misuse of AES-GCM by a client bug (salt reuse, missing AAD) | Medium | High | Single API that cannot take caller-supplied nonces; negative vectors; fuzzing; server-side salt-collision metric |
| R3 | Commitment omitted by one implementation | Medium | High | Negative vector per implementation; reader refuses objects without it |
| R4 | Browser key storage lost (IndexedDB eviction) | High | Medium | Multi-device and recovery key requirements; clear warnings |
| R5 | Key substitution by a malicious service on first contact | Medium | High | Fingerprints, signed log, organisation-pinned directory (Phase 3) |
| R6 | Unclear user mental model of "removed member can still read old content" | High | Medium | Honest copy; optional re-seal job; do not market as "revoke access to everything" |
| R7 | Harvest-now-decrypt-later on grants | Low now, rising | High for 10-year secrets | Suite 0x10 plan, state limitation |
| R8 | Scope creep towards MLS features (real-time, large groups) | Medium | Medium | Written non-goals; revisit gate in master design |
| R9 | Padding and size leaks expected to be "zero knowledge" by buyers | Medium | Medium | Claims policy; metadata inventory shown to customers |

## 18. Open questions

| # | Question | Recommended default | Decider |
|---|---|---|---|
| OQ-CR-1 | One AEAD suite (AES-256-GCM) in Phase 1, or also XChaCha20-Poly1305 for native clients? | AES-256-GCM only; add nothing until WebCrypto ships ChaCha20-Poly1305 | Engineering lead |
| OQ-CR-2 | Plaintext container: canonical JSON or deterministic CBOR? | Canonical JSON (matches existing `CanonicalValue`) | Engineering lead |
| OQ-CR-3 | Expose truncated `cid` for server-side deduplication? | Off; available as audited policy | Product owner with security review |
| OQ-CR-4 | Hide child `refs` (sealed index nodes) in Phase 1? | No; revisit in Phase 3 for high-sensitivity tier | Product owner |
| OQ-CR-5 | Client key files: extend `dynodoc-key-v1` (built in the working tree, PBKDF2) or adopt `age` (scrypt)? | Extend to v2 with a memory-hard KDF and the encryption key; keep an `age` export for backups | Security engineer |
| OQ-CR-6 | Recovery key length and encoding | 256 bits, Crockford base32, checksum | Product owner |
| OQ-CR-7 | History policy default for new members | `full` | Organisation administrator (per org), product default by owner |
| OQ-CR-8 | Rotate on role downgrade? | No | Security engineer |
| OQ-CR-9 | Hybrid PQ suite timing | Wait for a standardised HPKE codepoint and an audited library | Owner |
| OQ-CR-10 | Use of ring versus a RustCrypto stack for all sealing in Rust | ring for AEAD, HKDF, HMAC, SHA, Ed25519; `hpke` crate for X25519 | Engineering lead |

## Appendix A: illustrative sealed-object vector (non-normative)

Computed on 14 October 2026 with Node.js 24.12 WebCrypto using only the primitives of suite
0x01 and fixed public test inputs. It illustrates the algorithm of section 8; the normative
vectors will be the independent files of section 14. The script is not part of the repository.

Inputs:

| Item | Value |
|---|---|
| Epoch root `PER_3` | `000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f` |
| Project UUID bytes | `a0a1a2a3a4a5a6a7a8a9aaabacadaeaf` |
| Document UUID bytes | `b0b1b2b3b4b5b6b7b8b9babbbcbdbebf` |
| Node ID | `01J9ZK3Q8M2V7N4X5C6D7E8F9G` |
| Kind / suite / epoch / revision | 1 (`node_fields`) / 1 / 3 / 5 |
| Plaintext fields (canonical JSON, 113 bytes) | `{"content":{"content":[{"text":"Revenue grew 5%.","type":"text"}],"type":"paragraph"},"label":"Revenue grew 5%."}` |
| Object salt | `c0c1c2c3c4c5c6c7c8c9cacbcccdcecfc0c1c2c3c4c5c6c7c8c9cacbcccdcecf` |
| Child refs | none (`refs_digest` = SHA-256 of the empty string) |

Derived values:

| Item | Value |
|---|---|
| `K_cid_3` (HKDF, info `dynodoc.k.cid.v1`, empty salt) | `aa761d84c71f1b23ccba87656d80892a58c53c50846a65059b24146e3c42682f` |
| `K_doc_{3,d}` (HKDF, empty salt, info `dynodoc.k.doc.v1` then project UUID then document UUID) | `748488f1b99cdee5d82ad406a29c9a7006a8c99595637d89f3324e70a2ece261` |
| `key_id` | `085428cdbf120f89` |
| `cid` | `4113aa16937b961695bf48598ef4266f827efef9cb28d255df8d269d9bc80fec` |
| Plaintext body (203 bytes) | `{"cid":"4113...0fec","fields":{...},"v":1}` |
| Framed and padded plaintext | 4-byte length + 203 bytes body, padded to bucket 256 |
| AAD length | 175 bytes |
| `refs_digest` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| AEAD key | `b8014bbc339853a12ee86a14c27a8ac4a5ae7ad5d54df3d055a9c9d6bf6ffbe7` |
| Nonce | `bc41609ee16409ad3bfac475` |
| Commitment key | `46f76d3dad6978caa51bdb3bbddb33dbc59cdb12b7ad1214449934a32d5b8fb8` |
| `key_commitment` | `d9d3eefc6923e82b1015ea358c1e2c84` |
| First 32 ciphertext bytes | `67883ffe13ce3904ac25e05d103765dfb6d41c3b44935e13ffc0d39185f96d63` |
| Tag | `d95345f60fdf7c6c496ca640f1e68add` |
| Header (74 bytes) | `44594e4f5345414c0101010000000003085428cdbf120f89c0c1c2c3c4c5c6c7c8c9cacbcccdcecfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd9d3eefc6923e82b1015ea358c1e2c840000` |
| Total envelope | 346 bytes (74 header + 256 ciphertext + 16 tag) |
| Address | `89e8e004de5459434f86d553f6064f1b2da33d49a2be9b982d64381cfd4ac250` |

## Appendix B: glossary

| Term | Meaning |
|---|---|
| AAD | Associated authenticated data: bytes authenticated but not encrypted |
| AEAD | Authenticated encryption with associated data |
| Epoch | Period with one epoch root and one recipient set |
| Epoch root (`PER`) | 32-byte random secret from which class and document keys derive |
| Explicit document root (`DR`) | Independent random key for a document with a narrower access list |
| Grant | HPKE-sealed delivery of an epoch root or document key to one device |
| HPKE | Hybrid public key encryption, RFC 9180 |
| Key commitment | Value binding a ciphertext to exactly one key |
| Keyed content identifier (`cid`) | HMAC of canonical plaintext under an epoch key |
| Membership log | Signed, hash-chained record of member, device, recovery and escrow changes |
| Recovery key (`RK`) | User-held secret from which a recovery recipient and signer derive |
| Sealed object | Container of section 8 |
| Suite | Authenticated identifier of the algorithm set |

## Appendix C: references

Retrieved 14 October 2026 unless stated. Items marked "from general knowledge" were not
re-fetched during this work and must be verified at build time.

| Reference | URL | Note |
|---|---|---|
| RFC 9180 Hybrid Public Key Encryption (February 2022, Informational, CFRG) | https://www.rfc-editor.org/rfc/rfc9180.html | Search result confirmed status and date |
| RFC 9420 Messaging Layer Security (Proposed Standard, 2023) | https://www.rfc-editor.org/info/rfc9420 | Fetched; exact day not shown on page |
| RFC 5869 HKDF | https://www.rfc-editor.org/rfc/rfc5869.html | From general knowledge |
| RFC 8032 Ed25519; RFC 7748 X25519; RFC 9106 Argon2; RFC 8949 CBOR | https://www.rfc-editor.org/ | From general knowledge |
| NIST SP 800-38D (GCM) | https://csrc.nist.gov/pubs/sp/800/38/d/final | From general knowledge; 2^32 random-nonce invocation limit |
| age specification | https://github.com/C2SP/C2SP/blob/main/age.md | Search summary; details to verify |
| WICG Secure Curves status thread | https://github.com/WICG/webcrypto-secure-curves/issues/20 | Fetched |
| Igalia, "Can I use Secure Curves in the Web Platform?" (28 February 2025, updated 31 March 2025) | https://blogs.igalia.com/jfernandez/?p=1913 | Fetched |
| IPFS blog, Ed25519 in Chrome (13 August 2025) | https://blog.ipfs.tech/2025-08-ed25519/ | Fetched |
| Chromium intent, algorithm updates in WebCrypto (17 June 2026) | https://groups.google.com/a/chromium.org/g/blink-dev/c/TF8y175J3YU/m/esAKJTZTAAAJ | Fetched |
| MDN SubtleCrypto.generateKey | https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto/generateKey | Fetched; no compatibility table |
| hpke-js | https://github.com/dajiaji/hpke-js | Fetched; not audited |
| rust-hpke | https://docs.rs/crate/hpke/latest | Search summary |
| hpke-rs | https://docs.rs/hpke-rs | Search summary |
| Dodis, Grubbs, Ristenpart, Woodage, "Fast message franking", CRYPTO 2018 ("invisible salamanders") | https://eprint.iacr.org/2019/016 | From general knowledge |
| Len, Grubbs, Ristenpart, "Partitioning oracle attacks", USENIX Security 2021 | https://www.usenix.org/conference/usenixsecurity21/presentation/len | From general knowledge |
| Nikitin et al., PURBs and Padme, PoPETs 2019 | https://petsymposium.org/popets/2019/popets-2019-0056.php | From general knowledge |
| CryptPad whitepaper | https://blog.cryptpad.fr/images/whitepaper.pdf | Search summary |
| Google Workspace client-side encryption | https://developers.google.com/workspace/cse | Search summary |
| Microsoft Purview Double Key Encryption FAQ | https://learn.microsoft.com/purview/double-key-encryption-faq | Search summary |
| Proton Docs announcement | https://proton.me/blog/docs-proton-drive | Marketing; no design detail |
| Signal protocol documentation (X3DH, PQXDH, Double Ratchet) | https://signal.org/docs/ | From general knowledge; not re-fetched |
| Matrix end-to-end encryption (Olm, Megolm) | https://matrix.org/docs/matrix-concepts/end-to-end-encryption/ | From general knowledge; not re-fetched |
| Tresorit security and encryption whitepaper | https://tresorit.com/security | From general knowledge; protocol specifics unverified |
