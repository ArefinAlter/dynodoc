# End-to-end encryption: clients and key management

Status: design, not implemented. Date: 14 October 2026. Owner: Verne Ltd. Documentation
only. Everything below is **proposed** and describes clients that do not exist yet, except
where a row cites code.

Part of a set. Master design: [END-TO-END-ENCRYPTION.md](END-TO-END-ENCRYPTION.md).
Companions: [cryptographic design](E2EE-CRYPTOGRAPHIC-DESIGN.md),
[protocol and migration](E2EE-PROTOCOL-AND-MIGRATION.md),
[threat model and assurance](E2EE-THREAT-MODEL-AND-ASSURANCE.md). Related and concurrent, not
authoritative here: [SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md) and
[AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md).

## 1. Purpose, goals, non-goals, personas, metrics

### 1.1 Purpose

Describe what each Dynodoc client must do to hold keys, seal and open content, enrol devices,
add and remove members and recover from loss; assess concretely whether the Rust merge and
diff engine can run in the browser, in Office task panes and in Google sidebars; and state
what each client cannot do.

### 1.2 Goals

| ID | Goal |
|---|---|
| CL-G1 | One cryptographic implementation per runtime, verified against shared vectors. |
| CL-G2 | Keys at rest are non-extractable or encrypted wherever the platform allows it, and the client says plainly where it does not. |
| CL-G3 | Adding a device, removing a device and recovering from loss are flows a non-expert can complete without the operator. |
| CL-G4 | Member changes (add, remove, role change) produce the right cryptographic action automatically and show what happened. |
| CL-G5 | A path to run the existing merge, diff and integrity logic on the client without a second hand-written implementation. |
| CL-G6 | Local AI agents can work on an encrypted project only through an explicit, scoped, logged delegation. |

### 1.3 Non-goals

- Defending a compromised endpoint (malware, a hostile browser extension, a malicious host
  editor). Stated, not solved.
- Hiding plaintext from Microsoft or Google: the host editors hold the document content in
  the clear by design. Dynodoc's encryption concerns the copy held by the Dynodoc service.
- A native desktop application in Phase 1 or 2. The command line is the high-assurance client.
- Mobile clients (none exist today).

### 1.4 Personas

| Persona | Client | Need |
|---|---|---|
| Researcher using the web workspace | Browser | Open, edit and review without thinking about keys |
| Analyst using Word or Excel | Office add-in | Capture and apply changes on an encrypted project |
| Operator of a regulated site | CLI | Scriptable, auditable operations, hardware-bound keys |
| Team lead | Browser | Invite, remove, recover a colleague |
| Developer or analyst using an AI assistant | Local MCP server | Let the assistant work on one file without sending the project key anywhere |
| Self-hosted administrator | All | Same clients against their own server ([SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md)) |

The three customer profiles of [decision 015](decisions/015-customers-formula-engine-and-planned-products.md)
(small team or lab on one node; department or university with SSO; enterprise or regulated
site, HA, intranet-only or air-gapped) shape the defaults: the first wants a recovery key and
no escrow, the second wants escrow with SSO-linked offboarding, the third wants hardware keys,
no browser clients and an offline CLI.

### 1.5 Success metrics

| Metric | Target |
|---|---|
| First encrypted project created and a second device added, by a new user without help | Under 10 minutes; at least 85 percent success in moderated tests |
| Support contacts for "lost access" per 100 encrypted projects per quarter | Under 2, none unrecoverable where two recovery paths were set |
| Time to open a 100-page encrypted document in the browser from cache | Under 2 s p95 on a 2022 laptop |
| Client unlock failures due to storage eviction | Measured; each followed by a successful recovery in under 15 minutes |
| Share of encrypted projects with at least two recovery paths | At least 90 percent |

## 2. Current state versus target

| Capability | State | Evidence |
|---|---|---|
| Browser editors and import or export of DOCX, XLSX, PPTX entirely in the client | EXISTS | `frontend/researcher/src/lib/workspace/docx-import.ts`, `xlsx-import.ts`, `pptx-import.ts` (workers), `docx-export.ts`, `xlsx-export.ts`, `pptx-export.ts` |
| TypeScript port of the event fold | EXISTS | `frontend/researcher/src/lib/materializer.ts` (documented as mirroring `materializer.rs`; no cross-language fuzz parity harness) |
| TypeScript rich-text patch application | EXISTS | `frontend/researcher/src/lib/workspace/richtext.ts` (`applyRichTextPatch`) |
| Browser spreadsheet calculation | EXISTS (in-house engine since 14 Oct 2026, decision 015) | `frontend/researcher/src/lib/workspace/workbook-calculator.ts`; engine in `src/lib/workspace/formula/` |
| Merge, diff, convergence, integrity, expressions in the browser | MISSING | Only in Rust: `workspace_merge.rs`, `merge.rs`, `repository/convergence.rs`, `integrity.rs`, `expr.rs` |
| Browser holds a signing key | MISSING by design | [decision 014](decisions/014-web-branch-authoring-and-follows.md), "No signing identity in the page" |
| Enrolled Ed25519 device keys for CLI and native clients | EXISTS | [DEVICE-SIGNING-KEYS.md](DEVICE-SIGNING-KEYS.md); `engine-cli device keygen`, `device prove` |
| Passphrase- or OS-keystore-encrypted CLI key file | PARTIAL (working tree) | `crates/engine-cli/src/keyfile.rs`, `keystore.rs`, `passphrase.rs` |
| CLI local repository, push, fetch, pull, merge, review, receipts | EXISTS | `crates/engine-cli/src/repository.rs`, `sync.rs`, `receipt.rs`; [SELECTED-HISTORY-SYNC.md](SELECTED-HISTORY-SYNC.md) |
| Office add-ins (Word, Excel, PowerPoint) as web task panes with in-memory connection keys | EXISTS (test builds; no listing) | `extensions/word/manifest.xml`, `frontend/researcher/src/lib/provenance/`, [listings/addin-data-handling.md](listings/addin-data-handling.md) |
| Google add-ons (Docs, Sheets, Slides) as Apps Script with sidebar | EXISTS (test builds) | `extensions/google-docs/`, `google-sheets/`, `google-slides/` |
| Browser approval of an add-in connection with a code confirmed in both places | EXISTS | [CONNECTOR-PAIRING-AND-INSTALLATION.md](CONNECTOR-PAIRING-AND-INSTALLATION.md) |
| Local MCP server | MISSING | Planned in [AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md) |
| Key store, unlock, enrolment of encryption keys, grants UI, recovery UI | MISSING | This document |
| WebAssembly build of the engine | MISSING | Section 4 |

## 3. The web workspace

### 3.1 What the application is today

A SvelteKit application (`adapter-node`) served from the operator's origin. Workspace pages
are client-rendered: server `load` functions exist only for admin and public view routes
(`routes/admin/+page.server.ts`, `routes/view/[token]/+page.server.ts`); the workspace fetches
through the same-origin proxy `routes/api/engine/[...path]/+server.ts`. Sessions are encrypted
HttpOnly cookies. CSP and framing are set in `src/lib/server/security.ts`. Import parsing and
export assembly run in the browser, so the browser **already holds plaintext** while editing.
The server proxy and the web tier therefore need to handle ciphertext only; routes that make
the web tier see content must be disabled for encrypted projects: `routes/api/convert-doc`
(converter), `routes/api/research` and `routes/api/google-forms` (third-party imports fetched
server-side).

### 3.2 Key custody in the browser

```
 +---------------------- origin app.example (page, workers) ----------------------+
 |  UI thread: editor, review UI  --postMessage-->  Crypto worker                 |
 |                                                  - seal / open / derive        |
 |                                                  - holds CryptoKey handles     |
 |                                                  - no DOM, no network          |
 |  IndexedDB "dynodoc-keys"                                                      |
 |     DSK  CryptoKey Ed25519   (extractable: false)                              |
 |     DEK  CryptoKey X25519    (extractable: false)                              |
 |     PER_e  CryptoKey HKDF    (extractable: false), one per held epoch          |
 |     fingerprints of members (TOFU records), pinned project mode, log heads     |
 +--------------------------------------------------------------------------------+
```

| Item | Design |
|---|---|
| Algorithms | WebCrypto Ed25519 and X25519 where available (browser matrix in the [cryptographic design, 3.4](E2EE-CRYPTOGRAPHIC-DESIGN.md#34-webcrypto-support-verified-14-october-2026-with-limits)); AES-GCM, HKDF, HMAC always |
| Non-extractable keys | `generateKey(..., extractable=false)`; stored in IndexedDB as structured-clone `CryptoKey` handles. Epoch roots are imported as HKDF keys with `extractable=false` after the HPKE open, so the raw 32 bytes exist in JS memory only during import |
| What non-extractable does not give | Any script running in the origin can still **use** the key (sign, derive). It prevents theft of the bytes, not misuse by injected script. The control for that is script integrity (3.4) |
| Isolation | A dedicated worker for crypto; optional separate-origin iframe for the key store (`keys.<domain>`) with a narrow `postMessage` API, as CryptPad does for editors (their [whitepaper](https://blog.cryptpad.fr/images/whitepaper.pdf)); strict CSP; Trusted Types where available |
| Unlock policy | Default: keys usable while the session is signed in on this browser profile. Optional organisation policy: require a passphrase or a WebAuthn PRF-derived secret per session (3.5) |
| Memory | JavaScript offers no reliable zeroization; decrypted state is held in worker memory and dropped on lock. Stated, not solved |
| Offline | Ciphertext objects are cached in IndexedDB; a decrypted search index is stored sealed under a device-local key |
| Fallback without native Ed25519 or X25519 | Pure-JavaScript or WASM implementations (for example `@noble/curves`, unverified for audit status) keep working but keys cannot be non-extractable; the UI shows "software key, less protected" and organisation policy can forbid it |

### 3.3 Persistence and loss

| Risk | Facts and handling |
|---|---|
| Storage eviction and "clear site data" | Browser storage is best-effort. Call `navigator.storage.persist()` and show its result. Safari limits script-writable storage for sites the user does not interact with for seven days (WebKit ITP policy, from general knowledge, verify at build time); Home Screen web apps are exempt. Design assumption: **a browser device can disappear at any time**, so a project must have another device or a recovery path (cryptographic design, 11.5) before encryption is enabled |
| Private browsing | IndexedDB may be unavailable or discarded; the client refuses to enrol a device in a private window |
| Multiple tabs | One crypto worker per origin through a `SharedWorker` or Web Locks to avoid concurrent epoch handling |
| Browser upgrade | Keys survive; self-test (cryptographic design, 3.6) runs on every start |

### 3.4 Code delivery: the largest residual risk of browser E2EE

The JavaScript that holds the keys is delivered by the same service the design excludes from
reading content. An operator (or an attacker controlling the web tier) can ship a modified
bundle that exfiltrates keys or plaintext. Mitigations, from weakest to strongest:

| Control | Effect | Notes |
|---|---|---|
| CSP, no third-party scripts, SRI on any external asset | Blocks injection by other parties | Existing posture in `security.ts`; does not stop the operator |
| Published build hashes and reproducible builds | Lets a third party verify what was shipped | Needs a place to publish and people who check |
| Detached signature over the application bundle manifest with an offline release key, verified by a small client-side loader or browser extension (an approach like Meta's Code Verify, unverified) | An attacker with only server access cannot ship an unsigned bundle to users who run the verifier | Requires an extension or an installed shell; not available to a plain browser tab |
| Packaged or isolated web application (browser mechanisms for signed bundles, status unverified) | Removes live code delivery | Watch item, not a plan |
| Use the CLI or a native shell for high-assurance projects | Code is installed and versioned by the user | The recommended answer for regulated profiles |
| Self-hosted edition | The operator is the customer | See [SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md); changes who the adversary is |

Product statement: browser E2EE protects stored data against database, backup and passive
operator access. It does not protect against an operator who actively serves a malicious
application. The claims policy in the master design requires this sentence wherever the
browser client is described.

### 3.5 Hardware and passkey options

WebAuthn PRF (the `prf` extension) can derive a stable 32-byte secret from a credential,
usable as a wrapping key for local key material. Support is uneven: vendor and community sources
retrieved 14 October 2026 report platform-authenticator support in Chrome (from about 132 on
macOS with iCloud Keychain, 147 on Windows for PRF at creation) and Safari 18, uncertain Firefox
versions, and a March 2026 WebKit bug about CTAP2 security keys returning an encrypted
hmac-secret in Safari. Treat as **provisional and Phase 3**: feature-detect, never require.
Sources are listed in section 16 with their limits.

### 3.6 Changes to the existing web code (summary)

| Area | Change |
|---|---|
| `lib/workspace/api.ts`, `documentStore.svelte.ts`, `changes.ts` | Route reads and writes through a sealing layer when a project is encrypted; keep plaintext path for plaintext projects |
| New `lib/e2ee/` | Key store, worker, grants, epoch and log verification, fingerprints |
| `routes/connectors/*` | Pairing extended with key delegation (section 5) |
| `routes/api/convert-doc`, `api/research`, `api/google-forms` | Refuse when the target project is encrypted, unless the per-file consent path of the protocol document (row 12) is used |
| `hooks.server.ts` / `security.ts` | Add worker and frame sources; Trusted Types; no inline script |
| Admin site | Show counts only; no content views exist today and none are added |

## 4. The engine in WebAssembly

### 4.1 Why

Merge, diff, convergence, expression parsing, referential integrity and similarity exist once,
in Rust. The browser has a TypeScript port of the fold and of rich-text patches
(`materializer.ts`, `richtext.ts`) but nothing for merge or diff. Without a client-side
engine, an encrypted project cannot be merged or reviewed in the browser (matrix rows 3 to 5 of
the protocol document). Decision 014 names this path: "a stable WebAssembly build with the
published test vectors". A second hand-written TypeScript implementation of about 6,500 lines
of merge and validation logic would diverge; one Rust source compiled twice is the better bet,
provided the build is practical. This section assesses that by reading the code.

### 4.2 Module inventory (HEAD plus working tree, 14 October 2026)

| Module (`crates/engine-core/src`) | Lines | Dependencies beyond serde | I/O | WASM readiness |
|---|---:|---|---|---|
| `materializer.rs` (fold, `DocumentState`) | 493 | `engine_shared` rows (`Event`, `Snapshot`) | none | Ready after `engine-shared` is split |
| `richtext.rs` (patch apply, text diff, merge) | 657 | `ring::digest`, `similar` (deadline), `crate::log::canonical_json` | none | Blocked by B4 and B5 below |
| `workspace_merge.rs` (three-way merge, diff) | 658 | `materializer`, `richtext` | none | Ready after richtext |
| `merge.rs` (event-branch merge) | 515 | `integrity`, `materializer` | none | Ready |
| `integrity.rs`, `expr.rs`, `ops.rs`, `governance.rs` | 804, 762, 752, 773 | `engine_shared`, `materializer` | none | Ready |
| `similarity.rs` | 726 | `materializer` | none | Ready |
| `shared_checkpoint.rs`, `shared_checkpoint/canonical.rs`, `typed.rs`, `content.rs` | 1045, 448, 1090, 686 | `ring::digest`, `tokio::io` traits, `sqlx::Error` in `Error` | store trait only | Blocked by B3 and B9 |
| `repository.rs`, `attestation.rs`, `device.rs`, `device_session.rs`, `compare.rs`, `recovery.rs`, `convergence.rs`, `project_merge.rs` | 451, 650, 182, 316, 144, 353, 773, 218 | `ring::signature`, `uuid`, `sqlx::Error` in `Error` | `convergence`, `project_merge` take `LocalRepository` | Verification and signing ready after B3; convergence needs a store trait instead of `LocalRepository` |
| `log.rs`, `snapshot.rs`, `snapshot_verification.rs`, `database.rs` | 645, 540, 518, 288 | `sqlx::PgPool`, `tokio::spawn`, `futures` | PostgreSQL | Server only; never compiled for WASM |
| `shared_checkpoint/postgres.rs`, `repository/local*.rs`, `receipt_journal.rs`, `server_authority.rs` | 445, 1283+, 549, 566 | `sqlx` (Postgres, SQLite), `std::fs` | Databases, files | Server and CLI only |

About 12,000 lines of the crate are pure computation and are the WASM candidate set.

### 4.3 Concrete blockers found by reading Cargo manifests and source

| # | Blocker | Evidence | Fix |
|---|---|---|---|
| B1 | `engine-shared` depends unconditionally on `sqlx` with the workspace feature set (Postgres, `runtime-tokio-rustls`, macros, migrate) for `FromRow` and `Type` derives | `crates/engine-shared/Cargo.toml`; `rows.rs` (`#[derive(FromRow)]` on `Document`, `Node`, `Event`, `Snapshot`, ...); `ids.rs` (`sqlx::Type` on seven newtypes) | Cargo feature `db` (default on) with `#[cfg_attr(feature = "db", derive(sqlx::FromRow))]` and likewise for `Type`; `sqlx` becomes optional |
| B2 | `engine-core` depends unconditionally on `sqlx` (Postgres and SQLite), `tokio` with `features = ["full"]`, `futures`, `ring`, `uuid` v4, `ulid`, `chrono`; `tokio` `full` enables `net`, `fs` and `process`; tokio documents that only a subset of features is supported on WebAssembly targets (verify at build time) | `crates/engine-core/Cargo.toml`; workspace `Cargo.toml` | Features `persistence` (default on: `sqlx`, full `tokio`, `futures` stream) and a default-off `pure` build using `tokio` with `io-util` only |
| B3 | Error enums embed `sqlx::Error`: `shared_checkpoint::Error::Db`, `repository::Error::Db`, `log::EventError::Db`, and `shared_checkpoint::Error::History(SnapshotError)` | `shared_checkpoint.rs:77`, `repository.rs:46`, `log.rs:43` | `#[cfg(feature = "persistence")]` on those variants, or a boxed `dyn Error` variant |
| B4 | Pure code reaches into the database module: `richtext::fingerprint` calls `crate::log::canonical_json`, and `log.rs` imports `sqlx::PgPool`; `snapshot::merkle_root` is pure but lives in a file that imports `sqlx` and spawns tasks | `richtext.rs` (`fingerprint`, near line 69), `log.rs:26-33`, `snapshot.rs:6,464` | Move `canonical_json`, `ZERO_HASH`, `merkle_root` to a small pure module (`canonical`, `merkle`) and re-export from the old paths |
| B5 | **Wall-clock deadline in the text diff.** `richtext::text_diff` bounds the diff to 20 ms with `std::time::Instant::now()` | `richtext.rs:14-17,152-159` | (a) `std::time::Instant::now()` panics on `wasm32-unknown-unknown` (no clock). `similar` 3.2.0 has feature `wasm32_web_time` (verified in the local registry copy) that swaps its `Instant` for `web_time::Instant`; the call site must use that type. (b) More important: a **time-based** bound makes the diff, and therefore merge output, depend on machine speed. A slower device can fall back to a coarse replacement and produce a different merged state or different conflicts than a fast one. This is a determinism risk for v3 `same_file_resolutions` and `verify-merge`, independent of WASM. Replace the deadline with a deterministic work budget (maximum edit-script cost or step count) before relying on cross-device verification. Needs a test to confirm the current behaviour |
| B6 | `ring` on `wasm32-unknown-unknown` needs feature `wasm32_unknown_unknown_js` (routes randomness to `getrandom/js`) and a clang toolchain to build its C and assembly; its `agreement` module supports ephemeral keys only | `ring-0.17.14` `Cargo.toml`, `src/rand.rs`, `build.rs` | Keep `ring` with the feature for SHA-256, HMAC, HKDF, AES-GCM, Ed25519; add `hpke` or `x25519-dalek` for static X25519 (cryptographic design, 3.3); or move hashing and signatures to RustCrypto crates (`sha2`, `hkdf` and `ed25519-compact` are already in `Cargo.lock`, through `sqlx` and `pasetors`) under a `crypto` trait |
| B7 | Randomness and time APIs: `uuid` v4 needs `getrandom` with the `js` feature; `ulid::Ulid::new()` reads `SystemTime`, used in `log.rs` (server path); `chrono::Utc::now()` is used only in tests of pure modules | `Cargo.toml` features, `log.rs:96`, `materializer.rs:357` | Enable `uuid/js`; take time and entropy as parameters for ULID creation on the client (the TypeScript `ulid` package is already used for new nodes) |
| B8 | `ObjectStore` requires `Send` and returns `impl Future + Send`; a JavaScript-backed store (IndexedDB) is not `Send` | `shared_checkpoint.rs:159-205` | Pre-load needed objects into the existing `MemoryStore` from JavaScript and call the engine synchronously over it (bounded by `READ_LIMIT` style budgets); or make the trait `?Send` behind a feature |
| B9 | `typed::write_state`, `content::{write, read_into}` take `tokio::io::AsyncRead` and `AsyncWrite` | `content.rs:6`, `typed.rs:12` | `tokio` with `io-util` compiles for wasm; or provide slice and `Vec<u8>` entry points |
| B10 | `convergence` and `project_merge` are written against `LocalRepository` (SQLite) | `repository/convergence.rs:13`, `project_merge.rs:23` | Define a small `CommitSource` trait implemented by `LocalRepository` and by an in-memory view for WASM |
| B11 | Default semantic budgets are server-sized: 256 MiB of string bytes (`SemanticLimits`), 1,000,000 objects and 512 MiB (`Limits`) | `typed.rs:35-48`, `shared_checkpoint.rs:95-100` | Client profiles with lower limits (proposed 64 MiB desktop, 32 MiB constrained); WASM linear memory is at most 4 GiB and browsers allow less |
| B12 | Stack depth: recursive async radix-map build (`Box::pin` up to depth 64) and JSON nesting up to 64 on a default 1 MiB wasm stack | `shared_checkpoint.rs:404-423`, `typed.rs:25` | Keep the existing depth bounds; add a test at maximum depth under the wasm stack size; raise the stack if needed |
| B13 | Unconditional `miniz_oxide` (working tree, for local object compression) | `crates/engine-core/Cargo.toml` diff | Pure Rust; fine. Sealed objects do not compress; the local compactor should skip them |

None of these is a redesign. B1 to B4 and B8 to B10 are mechanical refactors with existing
tests as the safety net. B5 is both a WASM issue and a correctness question and should be
fixed first. B6 needs a decision on the cryptography crate split.

### 4.4 Proposed shape

```
 crates/engine-shared  [features: db]            types, enums, payload (no sqlx by default)
 crates/engine-core    [features: persistence]   pure modules always; PG/SQLite behind feature
 crates/engine-wasm    (new, cdylib, wasm-bindgen)
        exports (JSON or bytes in, JSON or bytes out; no callbacks):
          open_state(objects[], root, keys)        -> DocumentState (opaque handle)
          merge(base, team, draft, resolutions)    -> MergePlan
          diff(a, b)                               -> ops[]
          validate_integrity(state)                -> violations[]
          similarity(stateA, stateB)               -> comparison
          seal(kind, plaintext, ctx) / open(envelope, ctx)    (if Rust crypto is used)
          commit_address(commit) / verify_commit(signed)
          verify_log(entries) / verify_epoch_chain(records)
```

Data exchange rules: JavaScript owns storage and network; WASM is a pure function library
over byte slices. Large states stay inside WASM behind a handle (avoid copying a 50 MB state
into JavaScript objects). A dedicated worker owns the module and enforces a memory budget.

### 4.5 Verification of parity

| Check | Method |
|---|---|
| Existing Rust tests | Run the pure test modules under `wasm32-wasip1` with a WASI runtime (no browser needed) and under `wasm-bindgen-test` in Chromium, Firefox and WebKit |
| Golden vectors | Reuse `tests/fixtures/v1-log.json`, `content-v1.json`, `device-enrollment-v1.json`, `ref-receipt-v1.json`, the commit and typed fixtures, plus the new sealing vectors |
| Differential | Random operation sequences through Rust native, WASM and the TypeScript materializer; compare states and addresses (the TS port currently lacks such a harness, `materializer.ts` header) |
| Determinism | Merge results identical across machines of different speed (B5) and across native and WASM |
| Fuzz | `cargo-fuzz` on native with the same targets; corpus replayed in WASM |

### 4.6 Size and performance (estimates, unverified)

No measurement exists. Expected order of magnitude to be confirmed by a spike: module of 0.5 to
2 MB before compression (`serde_json`, `similar`, `ring` assembly), merge of a 10,000-node
document in under 1 s, cold start under 300 ms with streaming instantiation. Acceptance
thresholds are in section 14. Build tooling: `wasm-bindgen` and `wasm-opt`; Rust toolchain is
pinned (`rust-toolchain.toml` 1.96.0).

### 4.7 Alternatives

| Option | For | Against |
|---|---|---|
| A. WASM from the Rust source (recommended) | One implementation, vectors shared, review once | Refactor B1 to B12; module size; debugging |
| B. Port merge, diff, integrity, expr and similarity to TypeScript | No toolchain change; small bundle | Second implementation of about 6,500 lines with divergence risk; the existing fold port has no parity harness |
| C. Keep merge server-side with an attested enclave | No client work | Moves the trust boundary back to the operator; contradicts E2EE |
| D. CLI-only for merge and review | Smallest | Web users cannot review; fails customer expectation |

Recommendation: A, preceded by the B5 fix and the B1 to B4 refactors, which also benefit
the existing code (they remove server dependencies from pure logic).

## 5. Office add-ins (Word, Excel, PowerPoint)

### 5.1 What exists

| Aspect | Today | Evidence |
|---|---|---|
| Hosting | Task pane page loaded from the operator origin on every open (`https://app.dynodoc.online/connectors/<host>`) | `extensions/word/manifest.xml`; [listings/addin-data-handling.md](listings/addin-data-handling.md) |
| Runtime | Windows: Microsoft Edge (Chromium) with WebView2; Mac and iOS: Safari WKWebView; Android: Chrome; Office on the web: the user's browser | [Microsoft documentation](https://learn.microsoft.com/office/dev/add-ins/concepts/browsers-used-by-office-web-add-ins) (page dated 19 August 2026) |
| Connection credential | 32 random bytes made in the pane; only its SHA-256 goes into the approval URL; held in pane memory only; seven-day grant | addin-data-handling section 4 |
| Local state | Capture ledger in the pane's IndexedDB (`dynodoc-provenance`): last observed text, change list, base checkpoint | addin-data-handling section 3 |
| Scope | One file and one host per key; read and propose only | `provenance.rs` `check_grant` |

### 5.2 What encryption changes

The pane reads plaintext from Word, Excel or PowerPoint through the host API, so the host
already has plaintext. For an encrypted project the pane must (a) obtain keys for **one
file**, (b) seal what it uploads, (c) open the checkpoint and bundles it downloads, and (d) show
review and apply flows from decrypted data.

Chosen approach: **delegated document keys, memory-only by default**, reusing the existing
browser approval flow.

```
 Pane (no keys)                 Dynodoc service              Browser (holds keys)
 1. make eph X25519 key pair E
    make connection key K (as today)
 2. open approval URL: hash(K), E.pub, host, file hint ------------------------->
                                                       3. user confirms code shown in both places
                                                       4. browser builds delegation list
                                                          [(epoch, K_doc_{epoch,d}) ...]
                                                          seals to E.pub (HPKE), signs
                                                       5. POST grant (kind 'agent'/'addin') --> store
 6. GET connection with K  <----------- connection + sealed grant ---------------
 7. HPKE open with E.priv; hold K_doc list in memory
 8. seal bundles with K_doc; upload ciphertext; fetch checkpoint, open locally
```

| Decision | Choice | Reason |
|---|---|---|
| Identity of the pane | None by default | A pane has no signing key (decision 014). The bundle is authenticated by the connection key as today; the delegation itself is recorded as a signed `agent_grant` log entry by the approving browser's device |
| Key scope | Only the epochs and the one document in the approval | Least privilege, G7 |
| Lifetime | Memory only; closing the pane ends it. Optional: persist `CryptoKey` handles non-extractable for N hours when the user chooses "remember on this device" | Matches today's behaviour (reopening needs sign-in) |
| Writes | Bundles are proposals as today (create a review draft; never a merge) | Existing contract in `provenance.rs` |
| Local ledger | Plaintext in IndexedDB as today; same exposure as the open document on that machine | Optional later: seal under a device-local non-extractable key |

### 5.3 Constraints

- **Code is delivered live.** The pane page is fetched from the operator's origin at each
  launch; the code-delivery risk of section 3.4 applies in full. A marketplace listing would
  not change that (the listing points at a URL).
- **Cryptography availability.** WebView2 follows the evergreen Chromium runtime, so native
  Ed25519 and X25519 arrive with the runtime versions in the [cryptographic design, 3.4](E2EE-CRYPTOGRAPHIC-DESIGN.md#34-webcrypto-support-verified-14-october-2026-with-limits).
  WKWebView follows the operating system; the stable Safari version that shipped X25519 is not
  confirmed here. The self-test (cryptographic design, 3.6) gates use; the pane shows "update
  your Office or operating system" when it fails.
- **Storage sharing.** Do not assume the pane in Word and the pane in Excel share storage with
  each other or with the browser. Each is its own device or an ephemeral delegate.
- **Apply preview** writes decrypted, reviewed text into the document, unchanged.
- **No offline capture without keys.** If the delegation is not in memory the pane can still
  capture locally (plaintext ledger) and uploads after approval.

## 6. Google add-ons (Docs, Sheets, Slides)

### 6.1 What exists

Editor add-ons with a server-side Apps Script runtime (V8) and an HTML Service sidebar.
`UrlFetchApp` talks to `https://app.dynodoc.online/api/connectors/` only (manifest
`urlFetchWhitelist`). Credentials, the capture ledger and queue chunks live in
`PropertiesService.getUserProperties()` (per user, 500 KB total, 9 KB per value, verified
against the [Apps Script quotas page](https://developers.google.com/apps-script/guides/services/quotas)
on 14 October 2026). `Protocol.html` is generated from the TypeScript protocol code at build
(`pnpm build:connectors`). Evidence: `extensions/google-docs/appsscript.json`,
`extensions/google-common/Pairing.gs`.

### 6.2 What Apps Script cannot do

| Need | Apps Script server runtime | Source |
|---|---|---|
| SHA-256 and HMAC-SHA-256 | Yes: `Utilities.computeDigest`, `computeHmacSha256Signature` | [Utilities reference](https://developers.google.com/apps-script/reference/utilities) (search of 14 October 2026) |
| AES-GCM or any AEAD | No such function in `Utilities` (not found; re-verify at build time) | |
| X25519, HPKE, Ed25519 | Not available (RSA-SHA256 signing exists; nothing for Curve25519) | Same |
| WebCrypto | Not in the server runtime; available in the sidebar page (the user's browser) over HTTPS | HTML Service pages are served from Google's `googleusercontent.com` origins; confirm secure-context and permissions-policy behaviour at build time |

### 6.3 Design: cryptography in the sidebar, transport in the script

```
 Sidebar (HTML Service iframe, user's browser)         Apps Script server (.gs)
   WebCrypto: seal / open, HPKE, key handling   <---->  DocumentApp / SpreadsheetApp / SlidesApp
   holds delegated K_doc list in memory                 reads host text, applies approved changes
   google.script.run.send(ciphertext)  ---------------> UrlFetchApp.fetch(Dynodoc, ciphertext only)
```

| Concern | Design |
|---|---|
| Where plaintext exists | In Google's own storage (the document), in the Apps Script runtime while it reads the document, and in the sidebar. The Dynodoc service receives ciphertext only |
| Key delegation | As for Office: the sidebar makes an ephemeral X25519 pair and the approval flow returns a sealed delegation list; no key is stored |
| Optional persistence | Wrapped key blob (`K_doc` list sealed under a key derived from a passphrase typed in the sidebar, or a WebAuthn PRF output where permitted) in `UserProperties`, within the 9 KB value limit (a delegation list is a few hundred bytes) |
| Browser storage | Do not rely on `localStorage` or IndexedDB inside the HTML Service iframe: partitioning and sandbox behaviour are not verified |
| CORS | If the sidebar called the API directly the connector routes would need CORS for the Google origin; transport through the script avoids this |
| Quotas | `UrlFetchApp` payload 50 MB per call and 20,000 calls per day on consumer accounts (100,000 on Workspace), per the quotas page; sealed chunks of 64 KiB fit |
| Code integrity | Installed add-on code changes only through a new Apps Script deployment; the sidebar page is embedded, not fetched. This is better than live-delivered Office panes, but Verne still controls what the next deployment contains |
| Slides | The mapping from Google shape IDs to Dynodoc node IDs stays in `UserProperties` (not secret) |
| Google Workspace client-side encryption | Documents under Google's own CSE are a separate Google feature; whether Apps Script add-ons can read them is not verified here. Dynodoc must not claim to work with them |

Because the Google editors hold the plaintext, **Dynodoc's end-to-end encryption does not
protect content from Google**. Product copy must say it.

## 7. Command line (`engine-cli`) and the local repository

The CLI is the Phase 1 reference client: it already has the local SQLite repository, signed
commits, selected-history sync, merge with same-file convergence, review commands and receipt
tooling (`crates/engine-cli`, [SELECTED-HISTORY-SYNC.md](SELECTED-HISTORY-SYNC.md)).

### 7.1 Key storage

| Platform | Existing (working tree) | Added for encryption |
|---|---|---|
| All | `dynodoc-key-v1`: PKCS#8 signing key encrypted under a passphrase (PBKDF2, 600,000 iterations, ChaCha20-Poly1305) | `dynodoc-key-v2`: also carries the X25519 key; memory-hard KDF (cryptographic design, 11.4) |
| Windows | DPAPI wrapping secret (`--keystore`) | Same |
| macOS | Login keychain through `security` | Same; Secure Enclave holds P-256 keys only (from general knowledge), so it can wrap but not hold Curve25519 keys |
| Linux | Secret Service through `secret-tool` | Same; TPM-sealed secrets via `systemd-creds` as an option (unverified) |
| Hardware token | None | PIV or FIDO2 hmac-secret as a wrapping factor (Phase 3); YubiKey support for Curve25519 in PIV is firmware dependent (unverified) |
| Non-interactive use | `DYNODOC_KEY_PASSPHRASE_FILE` (path, not secret) | Dedicated "robot" devices with narrow grants, never an owner key |

### 7.2 Commands (proposed)

```
engine-cli e2ee keygen    --out KEYFILE [--keystore]            # DSK + DEK, encrypted
engine-cli e2ee enroll    --server URL --key KEYFILE            # register DEK binding
engine-cli e2ee status    --project ID
engine-cli e2ee enable    --project ID [--recovery-key | --escrow] [--sealed-name]
engine-cli e2ee invite    --project ID --member ACCOUNT --role editor [--history full]
engine-cli e2ee remove    --project ID --member ACCOUNT          # remove + open new epoch, atomic
engine-cli e2ee rotate    --project ID --reason scheduled
engine-cli e2ee device    list | add --code CODE | approve --code CODE | revoke ID
engine-cli e2ee recovery  create | use
engine-cli e2ee verify    --project ID                           # log, epoch chain, grants, objects
engine-cli e2ee decrypt   --project ID --file ID --out PATH      # explicit export, audited locally
engine-cli e2ee migrate   --project ID --mode seal-from-now
engine-cli mcp serve      --project ID --file ID --scope read|propose   # section 8
```

Existing `repository`, `sync` and `receipt` commands gain a `--sealed` behaviour that is
selected automatically by the project's pinned mode. The local repository stores ciphertext
objects; decrypted working files are the user's own files on disk (already plaintext).

### 7.3 Interaction with local compaction

The working tree adds transparent per-object compression in the local repository
(`repository/local/compact.rs`, `miniz_oxide`). Sealed objects are incompressible; the
compactor must recognise the `DYNOSEAL` magic and skip them. Retention planning
(`retention.rs`) works because child `refs` are in the clear (cryptographic design, 8.7).

### 7.4 Air-gapped use

Grants, epoch records and log entries are small JSON files. The CLI can export and import
them (`e2ee grant export`, `import`) so a site without connectivity to the service can run an
encrypted repository over removable media with the same verification.

## 8. Local MCP and AI agents

This section states precisely how encrypted projects interact with AI agents. The connector
programme is described in [AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md) (concurrent and not
authoritative here); only the interaction with encryption is specified.

### 8.1 Which agent paths can read an encrypted project

| Path | Holds keys? | Can read content? | Behaviour |
|---|---|---|---|
| Local MCP server over stdio started by the user on a device that holds keys (Claude Desktop, Claude Code, an IDE agent) | Yes, delegated per file | Yes | Supported (Phase 1 CLI, Phase 2 packaged) |
| Remote connector hosted by Dynodoc (Claude custom connector, ChatGPT app, action or connector) | No: the service never holds keys | **No.** It sees ciphertext and metadata only | Content tools return `422 encrypted_project` with `alternative: "local_agent"`; metadata tools (list files, review state, history graph) can work |
| User pastes or uploads text into a chat | n/a | Yes | A user-initiated **declassification to the model provider**; outside Dynodoc's control |
| Export plaintext to a plaintext project, then connect a cloud connector | n/a | Yes | Explicit declassification (section 8.4); audited |
| Customer-operated agent gateway that holds delegated keys in the customer's own environment | Yes, in customer control | Yes | Phase 3 design placeholder; Dynodoc's hosted service still never holds the keys |
| Browser extension or third-party script reading the open page | n/a | Yes | Out of scope; treated as endpoint compromise |

Therefore: **cloud connectors cannot work on encrypted projects unless the user exports
plaintext.** That is a property of the design, not a missing feature, and the connector
documentation must say it up front.

### 8.2 Architecture of the local server

```
  AI client (Claude Desktop, IDE)
        | stdio (JSON-RPC, MCP)
        v
  engine-cli mcp serve  --project P --file F --scope read|propose
        |   holds: agent device key (own DSK), delegated K_doc list, session policy
        |   tools: list_files, read_nodes, find_text (local), propose_edit,
        |          add_comment, open_change_request
        v
  Dynodoc service (ciphertext only, normal project-device-session scopes)
```

| Property | Specification |
|---|---|
| Identity | A **separate device key** enrolled for the agent (label shows "agent"), with a schema-40 per-project grant limited to `project:read`, `history:read` and `history:write`; never `history:review`, so the agent cannot approve its own work |
| Cryptographic scope | The document key list for the chosen file (read delegation, 6.5 of the cryptographic design); expires with the session or at the next epoch |
| Writes | Proposals only: commits on an agent branch or change request; protected `main` still needs independent approvals; the publisher (the agent's account) cannot self-approve |
| Local safeguards | Read-only default; the server refuses `propose_edit` unless the user enabled propose for this session; every tool call is logged locally; a size cap per read; no network tool in the same server |
| Visibility | A signed `agent_grant` entry in the project's log (who delegated, which file, scope, expiry) appears in the members' activity |

### 8.3 Consent flow and warnings

1. The user runs the server for a file. The CLI prints and the host shows a confirmation with
   the following text (wording to be finalised by legal and UX):

   > This assistant will read the text of **Budget 2027.docx** from your encrypted project
   > "Finance" and may send parts of it to **the AI provider you configured in your assistant
   > application**. Dynodoc's encryption protects the file in Dynodoc's systems. It does not
   > apply after the text leaves your device. Check that provider's terms before continuing.
   > Members of the project will see that you granted this access.
   > [Allow read for 1 hour] [Allow read and propose for 1 hour] [Cancel]

2. Organisation policy (`agent_delegation`: `none`, `local_only`, `export_allowed`) is checked first; `none`
   blocks the delegation.
3. The first use per project shows a stronger explanation of prompt injection: document text
   can contain instructions aimed at the assistant.
4. Expiry is enforced by the server-side session scope (one hour tokens) and by the
   delegation's `expires_at_ms` for honest clients.

### 8.4 Declassification for cloud connectors

If an organisation wants a cloud connector to see a file, the only supported route is an
explicit action by a manager: "Publish a plaintext copy to a connected workspace". It creates
a new file in a plaintext project, shows what will leave encryption, records the audit entry
and the membership-log entry, and can be restricted by policy. The original stays encrypted.

### 8.5 What the design does not solve

A local agent that reads plaintext and a provider that retains it. Prompt-injection leading
an agent to send content elsewhere (mitigated by no network tools and review gates). A
malicious or compromised local server binary (supply chain, adversary ADV7 in the threat model).

## 9. Device enrolment and multi-device key sync

### 9.1 No key sync: grants instead

Private keys are never copied between devices and never uploaded. Each device has its own
`DSK` and `DEK`. "Syncing keys" means that an existing device issues **grants** for the new
device (cryptographic design, 7). The service stores the grants; it cannot open them.

### 9.2 First device

```
 User             Device A (browser or CLI)                    Service
  |  sign in  --------------------------------------------------> account session
  |           generate DSK, DEK (non-extractable / key file)
  |           enrol DSK (existing challenge and proof) -------------> device_signing_key
  |           register DEK binding (signed by DSK) -----------------> device_encryption_key
  |           create recovery key RK, show once, user confirms
  |           register recovery recipient and signer in log -------> project log entries
```

### 9.3 Second device approved by the first

```
 New device B                          Device A (existing)               Service
  generate DSK_B, DEK_B
  show: 6-word code + QR(pub keys)
                                        scan or type code
                                        show fingerprint of B, user compares
                                        sign log entry device_add(B)  ------> append
                                        for each project and epoch A holds:
                                          grant(B) ------------------------> store
  login, enrol DSK_B, register DEK_B ----------------------------------------> store
  fetch grants, verify chain, open
```

The code and QR contain only public keys and a short authenticator; they carry no secret that
would let the service impersonate B. A wrong fingerprint on A ends the flow.

### 9.4 New device with the recovery key

The user logs in, enrols a new device, types `RK`; the client derives the recovery recipient
and signer, opens grants addressed to it, signs `device_add` with the recovery signer and
issues grants to itself (cryptographic design, 11.2). No other member is needed.

### 9.5 Device removal

A device is removed from the profile (existing revocation, permanent). Remaining devices then
open a new epoch for each affected project. Until they do, the project shows
"rotation pending" to its owners; a removed device that still holds an old epoch root can
read what that epoch protects, which is stated at removal time.

### 9.6 Browser, CLI and add-ins side by side

| Client | Device identity | Grant it receives |
|---|---|---|
| Browser profile | Full device (DSK, DEK) | All epochs of projects the account is in |
| CLI | Full device | Same |
| Office pane, Google sidebar | No persistent identity (default) | Delegated document keys per session |
| Local MCP agent | Agent device key | Delegated document keys, read or propose |
| Robot or CI | Device with narrow grants | Specific files only |

## 10. Member add and remove flows

### 10.1 Roles to cryptographic capabilities

| Capability | Owner | Manager | Editor | Reviewer or approver | Viewer | Notes |
|---|:-:|:-:|:-:|:-:|:-:|---|
| Decrypt project content | yes | yes | yes | yes | yes | All members hold the epoch root; viewer versus editor is **not** a cryptographic distinction |
| Seal new content (write) | yes | yes | yes | proposals | no | Enforced by the server's role check and by clients ignoring commits signed by keys without write role at the commit's log position |
| Open an epoch, add or remove members | yes | yes | no | no | no | Server role plus signed-log authorization |
| Grant keys to a new device of an existing member | yes | yes | yes (own devices) | yes (own) | yes (own) | A member can always issue grants for their own enrolled devices |
| Set escrow, change history policy, migrate, declassify | yes | owner decides | no | no | no | Phase 3 for escrow |
| Delegate a document to an agent or guest | yes | yes | yes (own role) | no | no | Delegation never exceeds the delegator's own access |

Roles come from `space_member` (`owner`, `manager`, `editor`, `reviewer`, `viewer`) and
`document_access` (`author`, `reviewer`, `auditor`) (`migrations/0010_*`, `0015_*`).
Per-document narrower access maps to explicit document roots (cryptographic design, 4.2).

### 10.2 Adding a member

```
 Owner/manager client                    Service                     New member
  1 directory lookup  ----------------->  list member devices + fingerprints
  2 verify fingerprint (or TOFU note)
  3 sign member_add(role, history policy) ->  append log, create space_member
  4 for each of the member's device keys:
       grant epochs allowed by policy ----->  store
  5 if member has no device key yet:
       invitee grant to one-time invitation key; send key out of band ---------> member
                                                                    6 enrol device, open invitee grant,
                                                                      issue own grant (device_add)
```

### 10.3 Removing a member (atomic remove and rotate)

```
 Owner/manager client                    Service
  1 sign member_remove(account)      \
  2 draw PER_{e+1}, grants for all    >  one request: POST .../e2ee/members/{id}/remove
    remaining devices (and recovery,  /  body = log entry + epoch record + grants
    escrow)
  server: verify, then in ONE transaction:
     remove membership, append log entries, store epoch and grants, revoke the member's
     project device grants (schema 40) and sessions
```

The one-request form avoids a window in which the member is removed from the service but
still receives the next epoch's key, or the epoch rotates without the removal being recorded.
If the acting client is offline when someone must be removed urgently (for example by an
organisation administrator), the service can revoke **access** immediately (existing
membership and device-session checks) and flags the project `rotation_pending`; the
cryptographic rotation then waits for a client with keys, with escalating visibility to
owners. The service cannot rotate by itself because it holds no keys.

### 10.4 Bulk offboarding

An organisation administrator disables the account (existing control: it revokes device keys
and sessions, `migrations/0033_*`). The admin console lists projects where the departed user
was a member and `rotation_pending` is true; owners and managers receive a notification;
with escrow, an escrow-authorised administrator client can perform the rotations. Target: all
projects rotated within the organisation's policy window (default 72 hours).

### 10.5 Guests and external reviewers

A guest receives a **delegation grant** for specific files with an expiry, plus a normal
account and project role that the service enforces. They cannot be added to the membership
log as full members, and the delegation stops working at the next epoch.

## 11. Organisation escrow, administrator recovery, lost keys and offboarding

### 11.1 Terms

| Term | Meaning | Is it end-to-end encryption? |
|---|---|---|
| Escrow | An organisation-held recipient of every epoch grant, so the organisation can recover content | Yes, with the organisation as an additional trusted party |
| BYOK (bring your own key) | The customer supplies a key that the provider stores and uses | **No.** The provider can use the key; this is the model of Phase 0 (customer-managed at-rest keys), not E2EE |
| HYOK (hold your own key) | The key stays in customer systems; the provider calls a customer key service for each operation | Yes if the provider never sees content keys; adds a customer-controlled kill switch and an availability dependency |
| Recovery key | A user-held secret that can re-create access | Yes |
| Key custodian | A person who holds a share of an escrow or key-service secret | n/a |

### 11.2 Escrow ceremony and use

| Step | Specification |
|---|---|
| Create | On an offline machine, generate an X25519 escrow key; split the private half with Shamir secret sharing (threshold `t` of `n`, for example 3 of 5) onto hardware tokens or sealed paper held by named custodians; verify by reconstructing once; destroy the machine copy. Alternative: key generated inside a customer HSM or KMS with a non-exportable key and an access policy |
| Register | Organisation policy stores the public key; every encrypted project of the organisation records `escrow_set` (owner signature plus administrator countersignature) and future epochs include the grant |
| Backfill | Optional `escrow_backfill`: a manager's client issues grants for earlier epochs; otherwise escrow covers only the future |
| Use: lost access | The user's request is verified by the organisation's own process (SSO plus a manager approval ticket). `t` custodians authorise on a dedicated machine; the admin client opens grants addressed to the escrow key and re-seals to the user's new device; signs `escrow_recover` (user, device, ticket, project list). Members see a notice |
| Use: legal hold or discovery | Same mechanism with purpose `legal_hold`; output is an export through `e2ee decrypt`; recorded in the log and in the organisation's audit |
| Compromise or custodian change | `escrow_clear`, new escrow key, new epoch for every project; earlier epochs remain exposed to the old key. Re-seal job if required (cryptographic design, 6.7) |
| Transparency | The escrow recipient is listed in each epoch record's recipient digest; clients display "recoverable by your organisation" on the project; there is no hidden recipient |

Rules: escrow is never enabled silently; it cannot be added to a project without an owner's
signature; an organisation that requires escrow says so before a project is created
(`escrow_required`); a member who objects can move their work elsewhere, which is why the
statement must be visible.

### 11.3 HYOK key service (Phase 3 design sketch)

For organisations that want a kill switch and central policy without escrow of content keys:

```
 Client                                   Organisation key service (customer-run)
   1 open grant needs K_wrap  ----------->  POST /v1/unwrap
        { project, epoch, wrapped_key, device attestation, SSO assertion }
                                           2 policy: membership, device posture, time, quota
                                           3 log request
   <------------ unwrapped key ----------  4 respond (TLS, mTLS optional)
```

| Aspect | Specification |
|---|---|
| Layering | A grant carries `PER_e` sealed to the device; with HYOK the `PER_e` is additionally wrapped under an organisation key `KEK_org` that only the key service can use. Clients cache unwrapped epoch roots in memory for a session |
| Revocation | Disabling a user or device at the key service stops future unwraps; cached keys on a running device are not recalled |
| Availability | If the key service is down, members cannot open new sessions; existing sessions and cached roots keep working until restart (policy choice) |
| Comparison | Google client-side encryption uses an external key service with per-file data keys wrapped by it ([overview](https://developers.google.com/workspace/cse)); Microsoft Double Key Encryption keeps a second, customer-held key and has documented platform limits ([FAQ](https://learn.microsoft.com/purview/double-key-encryption-faq)). The design above is similar in shape and should be reviewed against both before specification |
| Interop | `KEK_org` may live in a customer HSM through PKCS#11 or a cloud KMS; Phase 0 already defines KEK handling for at-rest keys and can share the connector |

### 11.4 Lost-key behaviours (summary)

Complete matrix in the [cryptographic design, 11.5](E2EE-CRYPTOGRAPHIC-DESIGN.md#115-lost-key-matrix).
Client behaviour on each:

| User sees | Meaning | Client offers |
|---|---|---|
| "This device cannot open this project yet" | No grant for this device | Add device using another device; recovery key; contact a member |
| "Your recovery key did not match" | Wrong key or no recovery recipient registered | Check characters; try another path |
| "No member can restore your access" | Sole member, no recovery path | Explain permanence; offer deletion only |
| "Your organisation can restore access" | Escrow present | Start an escrow request |
| "Key changed for <name>" | Fingerprint differs from first use | Block grants and show both fingerprints; require explicit trust decision |

### 11.5 Runbooks (clients and operators)

| Event | Steps |
|---|---|
| Device stolen or lost | Revoke device in profile (permanent); another device opens a new epoch for each project; check log for unexpected entries; change local passphrase; assess whether the device was unlocked |
| Account compromise suspected | Administrator revokes sessions and device keys (existing control); user re-enrols with recovery key or escrow; projects rotate; review log, receipts and audit |
| Owner leaves the organisation | Transfer ownership by signed `owner_transfer` before offboarding; otherwise escrow-assisted transfer |
| Custodian leaves | Re-share the escrow secret with a new threshold (or rotate escrow); old shares destroyed |
| Suspected library vulnerability | Server advisory refuses sealed writes from flagged client builds (safety net, a malicious client can ignore it); ship fixed client; if keys were exposed, rotate epochs |
| Restore from backup | Devices that hold newer epoch roots re-publish signed records and grants (idempotent); members verify the log head against their pinned heads |

## 12. User-facing requirements and warnings

These strings and states are requirements on any client; exact wording belongs to UX and
legal review and must be localisable.

| State or action | Requirement |
|---|---|
| Creating an encrypted project | Explain in plain language what the service can and cannot see, that search, public links and legacy conversion are off, that administrators cannot reset access, and require recovery set-up. Do not use "zero knowledge", "military grade" or "unhackable" |
| Showing the recovery key | Display once, offer print and copy to a password manager, require re-entry of the first and last groups to confirm, never email it, never send it to the service |
| Project header | "Encrypted project" badge; "Legacy plaintext history present" until erased; "Recoverable by your organisation" when escrow is on; "Rotation pending" when removal has not been followed by a new epoch |
| Fingerprint screen | Show 60-digit and QR forms; explain why to compare out of band; keep a first-use record per member |
| Fingerprint change | Blocking dialog, not a toast; both fingerprints; "Do not share keys until you have verified" |
| Content not readable here | Distinguish: no grant, wrong epoch, corrupted object, unsupported client version |
| Features unavailable | Name the feature and the alternative (client-side search after unlock; plaintext copy for public sharing; convert in Word first) |
| Removing a member | State that the person keeps what they already received and that new content will be protected after rotation; show rotation status |
| Declassify or publish plaintext copy | Names the destination, the file list and who will be able to read; typed confirmation; audit note |
| Agent access | Consent text of section 8.3; visible list of active delegations with revoke |
| Browser client | Persistent link to "What does browser encryption protect?" containing the code-delivery sentence of section 3.4 |
| Accessibility | All flows keyboard operable; recovery key and fingerprints available as text for screen readers; no meaning conveyed by colour only |

## 13. Operations

| Topic | Specification |
|---|---|
| Client versions | Clients send `X-Dynodoc-Formats` (protocol document, 4.7). The service rejects unsupported combinations with `426`. A minimum client version per format can be raised to retire a flawed build; this is a safety net, not a security control against a malicious client |
| Release channels | Browser: continuous with signed-manifest option; CLI: versioned binaries with checksums and signatures; add-ins: operator-delivered (Office) or deployments (Google); WASM module pinned by hash in the app manifest |
| Telemetry | Counts only: unlock failures by reason, self-test failures, grant fetch latency, WASM load and merge timings. No content, no keys, no file names |
| Support | Support staff cannot recover keys or read content. Playbooks: "lost all devices" (recovery key, member re-grant, escrow, otherwise permanent), "fingerprint changed", "browser storage cleared", "client says unsupported format" |
| Self-hosted | Same clients with a configured server URL; offline package of the CLI; the self-hosted operator can run the application bundle themselves (changes the code-delivery trust, see [SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md)) |
| Rollback | A client release can be rolled back; key files and local stores are versioned and forward-compatible within a major format; `dynodoc-key-v1` files remain readable |

## 14. Delivery plan (clients workstream)

| # | Task | Phase | Person-weeks | Depends on |
|---|---|---|---:|---|
| K1 | Web: key store and crypto worker, enrolment, unlock, encrypted project open and edit (TS materializer path), grants UI | 1 | 14 | Crypto C2 to C5 |
| K2 | CLI: `e2ee` commands, key file v2, sealed local repository, verify | 1 | 10 | Crypto C2 to C5 |
| K3 | Member flows: invite, remove-and-rotate, recovery key, device approval (web and CLI) | 1 | 8 | K1, K2, protocol S3 |
| K4 | UX copy, accessibility review, help content, support playbooks | 1 | 5 | K1 to K3 |
| K5 | Cross-browser end-to-end tests (Chromium, Firefox, WebKit), chaos tests | 1 | 6 | K1 to K3 |
| K6 | Engine feature split (B1 to B4, B8 to B10), determinism fix (B5) | 2 | 8 | None |
| K7 | `engine-wasm` crate, bindings, build pipeline, parity harness | 2 | 10 | K6 |
| K8 | Web integration: worker, memory budgets, merge, diff, review rendering, client-side search index | 2 | 14 | K7, K1 |
| K9 | Office add-ins: delegation flow and sealing | 2 | 8 | K7, K1 |
| K10 | Google sidebar: sealing and delegation | 2 | 8 | K7, K1 |
| K11 | Local MCP server, consent, delegation, agent device flow | 2 | 6 | K2, K3 |
| K12 | Enterprise: escrow tooling, HYOK key service reference, hardware key factors | 3 | 14 | K3 |
| | **Phase 1 subtotal** | | **43** | |
| | **Phase 2 subtotal** | | **54** | |
| | **Phase 3 subtotal** | | **14** | |
| | **Total** | | **111** | |

Assumptions: three engineers (one Rust, two TypeScript with WebCrypto experience) plus a
designer at half time and a technical writer part time; existing Playwright infrastructure
(`frontend/researcher/e2e`, 103 browser tests at schema 39). Confidence: medium for Phase 1,
low to medium for Phase 2 (WASM size and determinism are open), low for Phase 3. Ranges:
Phase 1 34 to 58, Phase 2 42 to 80, Phase 3 10 to 24 person-weeks. Excludes: the
cryptographic module and server work (costed in their documents), external review, store
publication of add-ins, mobile clients, native desktop shells, FIDO2 hardware procurement,
localisation beyond English, and vendor HSM integrations beyond one reference.

Acceptance (Phase 1): a new user creates an encrypted project, adds a second device and a
colleague, removes the colleague, and recovers on a third device using the recovery key, in
Chromium, Firefox and WebKit and with the CLI, with no plaintext-class value stored by the
service (protocol document storage scan) and all vectors passing.
Acceptance (Phase 2): the same operations including merge and review in the browser using the
WASM engine, identical results to native on the parity corpus, module size and timings within
the thresholds below, and a real Word on Windows and a Google Docs test deployment passing the
capture, preview and apply flows on an encrypted project.

Performance thresholds (proposed): seal or open a 4 KiB object p95 under 5 ms in a worker;
open a 600-node document from cached ciphertext under 2 s; WASM module under 2 MiB
compressed and instantiated in under 300 ms on a mid-range laptop; merge of two 10,000-node
states under 1 s.

## 15. Risks and mitigations

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| CR1 | Malicious or compromised web bundle exfiltrates keys | Medium | Critical | Section 3.4 controls; CLI for high assurance; claims policy sentence; external review of build pipeline |
| CR2 | Users lose all devices and have no recovery path | High over years | High for that user | Mandatory second path; recovery key ceremony; escrow option |
| CR3 | Browser storage eviction breaks unlock | Medium | Medium | Persist request; multi-device; recovery |
| CR4 | WASM engine too large or too slow on constrained devices | Medium | Medium | Spike first; split modules; lazy load; CLI fallback |
| CR5 | Non-deterministic text diff makes merge results differ across machines | Medium | High for signed merges | Fix B5 before WASM; cross-machine determinism test |
| CR6 | Office pane or Google sidebar cannot reach required crypto or storage | Medium | Medium | Self-test; delegation without persistence; documented limits |
| CR7 | Users misunderstand scope ("Microsoft or Google cannot see my files") | High | High reputational | Plain copy, per-client statements, no unqualified claims |
| CR8 | Agent delegation leaks content to model providers | High by design | Medium | Consent, policy, no cloud connectors on encrypted projects |
| CR9 | Rotation lag after removal leaves a window | Medium | Medium | Atomic remove and rotate; `rotation_pending` visibility; escrow-assisted rotation |
| CR10 | Scope creep to a native desktop client | Medium | Medium | Out of scope until demand evidence |

## 16. Open questions

| # | Question | Recommended default | Decider |
|---|---|---|---|
| OQ-CL-1 | Should Phase 1 ship a browser client at all, or CLI only? | CLI plus a basic browser client for single-file edit and view; full browser parity in Phase 2 | Product owner |
| OQ-CL-2 | Separate-origin key frame for the browser key store? | Yes in Phase 1 if cost is under 3 person-weeks | Engineering lead |
| OQ-CL-3 | Persist delegated keys in add-ins? | No by default; opt in with a time limit | Security engineer |
| OQ-CL-4 | Pure-JavaScript curve fallback permitted? | Allowed with a visible "software key" label; organisation can forbid | Security engineer |
| OQ-CL-5 | WASM crypto (Rust `ring` or RustCrypto) versus WebCrypto in the same page? | WebCrypto for primitives in browsers; Rust crypto only for non-browser targets and parity tests | Engineering lead |
| OQ-CL-6 | Allow guests (external reviewers) in Phase 1? | No; delegation grants arrive with agents in Phase 2 | Product owner |
| OQ-CL-7 | Hardware key requirement for owners in regulated profiles? | Optional in Phase 3 | Customer-driven |
| OQ-CL-8 | Native shell for code-integrity (signed app) before Phase 3? | No; revisit on customer demand | Owner |

## Appendix A: glossary

| Term | Meaning |
|---|---|
| Crypto worker | Web Worker that holds `CryptoKey` handles and performs sealing |
| Delegation grant | Grant carrying document keys for one file, scoped and time-limited |
| Device | A (client, profile) pair with its own signing and encryption keys |
| HYOK | Hold your own key: key stays in customer systems |
| Parity corpus | Operation sequences and states run through native, WASM and TypeScript implementations |
| Rotation pending | Member removed at the service but the next epoch not yet opened by a client |

## Appendix B: references

Retrieved 14 October 2026 unless stated; limits noted.

| Reference | URL | Note |
|---|---|---|
| Office Add-ins: browsers and webview controls (updated 19 August 2026) | https://learn.microsoft.com/office/dev/add-ins/concepts/browsers-used-by-office-web-add-ins | Fetched; Windows uses WebView2 for any Office version |
| Apps Script quotas | https://developers.google.com/apps-script/guides/services/quotas | Fetched; properties 9 KB per value, 500 KB per store; UrlFetch 50 MB, 20,000 per day consumer |
| Apps Script Utilities reference | https://developers.google.com/apps-script/reference/utilities | Search result; HMAC and digest confirmed |
| WebAuthn PRF support (vendor and community posts) | https://www.corbado.com/blog/passkeys-prf-webauthn; https://bugs.webkit.org/show_bug.cgi?id=311099 | Provisional; verify against W3C WebAuthn Level 3 and browser release notes |
| CryptPad whitepaper | https://blog.cryptpad.fr/images/whitepaper.pdf | Search summary |
| Google Workspace client-side encryption | https://developers.google.com/workspace/cse | Search summary |
| Microsoft Double Key Encryption FAQ | https://learn.microsoft.com/purview/double-key-encryption-faq | Search summary; verify limits |
| WebCrypto algorithm support and HPKE libraries | [cryptographic design, appendix C](E2EE-CRYPTOGRAPHIC-DESIGN.md#appendix-c-references) | |
| Concurrent engine designs on witnesses and commit pruning | [DESIGN-WITNESSES-AND-TRANSPARENCY.md](DESIGN-WITNESSES-AND-TRANSPARENCY.md), [DESIGN-COMMIT-PRUNING.md](DESIGN-COMMIT-PRUNING.md) | Concurrent drafts, not authoritative |
