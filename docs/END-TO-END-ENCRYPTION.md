# End-to-end encryption and customer-managed encryption: master design

Status: design; one library module of Phase 0 is built and unused (section 7.11). Date: 14
October 2026. Owner: Verne Ltd. This set is documentation only. It changes no schema, API or
public wording. Anything described here as a capability is **proposed** unless a row cites
existing code.

Decision [015](decisions/015-customers-formula-engine-and-planned-products.md) (14 October
2026) records end-to-end encryption, with customer-managed at-rest encryption as Phase 0, as a
documented-not-built product. This is its master design.

## 0. How to read this document set

| Document | Question it answers |
|---|---|
| **This document** | What are we building, in what order, for whom, and what may we say? |
| [E2EE-CRYPTOGRAPHIC-DESIGN.md](E2EE-CRYPTOGRAPHIC-DESIGN.md) | Which keys, algorithms and constructions? |
| [E2EE-PROTOCOL-AND-MIGRATION.md](E2EE-PROTOCOL-AND-MIGRATION.md) | Which formats, tables and endpoints change, and how does a project migrate? What happens to every feature? |
| [E2EE-CLIENTS-AND-KEY-MANAGEMENT.md](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md) | How do browsers, add-ins, the CLI and local agents hold keys and enrol devices? Can the engine run in WebAssembly? |
| [E2EE-THREAT-MODEL-AND-ASSURANCE.md](E2EE-THREAT-MODEL-AND-ASSURANCE.md) | Against whom does it protect, what leaks, and what evidence do we need before claiming it? |
| [SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md) | The planned self-hosted edition (written concurrently; not authoritative here) |
| [AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md) | The planned agent connectors (written concurrently; not authoritative here) |

## 1. Summary

Dynodoc stores document content in a form the service can read, by design: the server folds
events, merges, diffs, searches and renders. The public pages say so ("Not end-to-end
encrypted", `frontend/researcher/src/routes/(site)/security/+page.svelte`). Customers in
enterprise and academia, and self-hosting customers with their own security teams, will ask
for stronger confidentiality. This design answers in four phases, each with its own claim.

| Phase | Name | Is it end-to-end? | One-sentence promise it can support |
|---|---|---|---|
| 0 | Customer-managed at-rest envelope encryption | **No** | "Your disks and backups are unreadable without keys you control; you can rotate and revoke them." The running service still reads content |
| 1 | Encrypted projects | Yes (after review) | "Content is encrypted on members' devices before upload; the service stores ciphertext and cannot read it, with these stated limits" |
| 2 | Client-side engine (WebAssembly) | Same property, better product | "Merge, diff and review work in the browser and add-ins on encrypted projects" |
| 3 | Enterprise key management | Same property, more control | "Bring your own key service, hardware keys, organisation escrow, offboarding automation" |

Recommendations, in order of confidence:

1. **Build Phase 0 first and sell it honestly.** It serves the self-hosted profiles at the
   lowest cost and risk and needs no client change.
2. **Treat Phase 1 as conditional on demand evidence** (section 12.3). It is a large, lasting
   commitment: about 170 person-weeks to a reviewed first release, a permanent assurance cost,
   and visible feature loss (server search, public links, server conversion).
3. **Do the engine refactor early** (feature gates, deterministic diff). It has value without
   encryption and shortens Phase 2 by months.
4. **Never promise "zero knowledge"** and never use "end-to-end encrypted" before independent
   review (section 3).

## 2. Purpose, goals, non-goals, personas, metrics

### 2.1 Purpose

Provide a decision-ready plan for confidentiality of document content against the Dynodoc
service and against theft of its storage, matched to the customer profiles of decision 015,
with explicit limits and an order that never ships a claim ahead of its evidence.

### 2.2 Goals

| ID | Goal |
|---|---|
| G1 | A customer can keep the provider's disks, backups and snapshots unreadable with keys the customer controls (Phase 0). |
| G2 | A customer can keep the provider's running service unable to read content of selected projects (Phase 1). |
| G3 | History, review, signatures, receipts, retention and erasure keep working over ciphertext. |
| G4 | Each feature lost or changed is named, with a client-side alternative where one exists. |
| G5 | Key loss, member removal and recovery are designed flows, not afterthoughts. |
| G6 | Every claim is tied to an assurance gate. |

### 2.3 Non-goals

- Protecting content from Microsoft or Google (host editors hold plaintext).
- Protecting a compromised endpoint or a user who exports plaintext.
- Hiding metadata (membership, structure, timing, sizes) from the service in Phases 1 and 2.
- Real-time collaborative editing.
- Replacing the receipt, witness or pruning designs; this work composes with them.
- Offering encryption for the legacy questionnaire adapter or civic features (not in product
  scope per decision 012).

### 2.4 Personas and use cases

| Persona (profile of decision 015) | Use case | Needs |
|---|---|---|
| Lab lead (small team, one node) | Draft and review a grant proposal with collaborators at two institutions | Simple recovery, no admin; Phase 0 via encrypted disk and backups is often enough |
| University IT security officer (department with SSO) | Allow research data protocols on the service without trusting the vendor's staff | Phase 1 with escrow for departing staff, SSO offboarding, audit |
| Enterprise CISO (regulated, HA, intranet-only or air-gapped) | Keep board and legal documents unreadable to the cloud provider and the platform team | Phase 0 with HSM; Phase 1 with CLI or self-hosted web; Phase 3 key service |
| Compliance officer | Show who had access, when keys rotated, any recovery | Signed log, audit |
| Analyst working in Word or Excel | Same workflow in the add-in | Phase 2 delegation |
| Self-hosted administrator | Run backups, rotate keys, upgrade | Phase 0 runbooks |
| Developer using a local AI assistant | Work on one file with an assistant | Local delegation, consent |

### 2.5 Success metrics

| Metric | Phase | Target |
|---|---|---|
| Customers on encrypted volumes and encrypted backups with restore drill passed | 0 | 100 percent of self-hosted installs within one release of availability |
| KMS outage handling: read and write behaviour as documented, no data loss | 0 | Passed in failure injection |
| Plaintext-class values stored for encrypted projects | 1 | 0 (storage scan) |
| Encrypted projects with two recovery paths | 1 | At least 90 percent |
| Browser merge and review on encrypted projects | 2 | Parity with plaintext on the corpus |
| Open critical or high review findings at availability | 1 | 0 |
| Customer demand evidence recorded before Phase 1 start | 1 | Checklist 12.3 complete |

## 3. Claims policy

What Verne may and may not say, now and per phase. This is the single source for
`/security`, `/privacy`, the FAQ, `llms.txt`, sales and support. The gates are defined in the
[assurance plan](E2EE-THREAT-MODEL-AND-ASSURANCE.md#10-assurance-plan).

| Level | Condition | Permitted | Must also say |
|---|---|---|---|
| 0 (today) | None | "Not end-to-end encrypted. The service can read content. No encryption-at-rest configuration is claimed." | Existing limits in `/security` |
| 1 | Phase 0 shipped, restore drill passed | "Customer-managed encryption at rest" for volumes, backups and covered columns | "The running service can still read content. Not end-to-end." |
| 2 | Phase 1 beta, gate G3 | "Encrypted projects (beta)" | "Not independently reviewed. The service sees membership, structure and timing." Do not use "end-to-end" (decision 015) |
| 3 | Gate G2 passed, G4 | "End-to-end encrypted projects" within the stated scope | Metadata visible; hosts hold plaintext; endpoint compromise out of scope; browser code-delivery limit; removed members keep past content |
| Never | n/a | "Zero knowledge", "unhackable", "military grade", "quantum-proof", "we cannot be compelled", "your data is deleted everywhere" | n/a |

### 3.1 Words that need care

| Phrase | Rule |
|---|---|
| "End-to-end encrypted" | Only at level 3 and only for encrypted projects, never for the product as a whole |
| "We cannot read your files" | Only with the sentence "we can see who works on what, when, and the rough shape and size of documents" |
| "Bring your own key" | Phase 0 customer-managed keys are BYOK in the provider sense: the running service can use them. Do not present as E2EE |
| "Encrypted at rest" | Specify what: volumes, backups, columns; and what remains readable by the running service |
| "Secure" or "private" | Avoid unqualified use |

## 4. Current state versus target

| Capability | State | Evidence |
|---|---|---|
| Content stored readable by the service (events, nodes, snapshots, uploads, objects) | EXISTS (by design) | [protocol document, 2.1](E2EE-PROTOCOL-AND-MIGRATION.md#21-where-content-and-metadata-are-stored-today) |
| Public statement "Not end-to-end encrypted" and no at-rest claim | EXISTS | `frontend/researcher/src/routes/(site)/security/+page.svelte`, `privacy/+page.svelte`, [LEGAL-PAGES.md](LEGAL-PAGES.md) |
| TLS in transit, sessions, restricted DB role, immutable history, signatures, receipts | EXISTS | [SECURITY-HARDENING.md](SECURITY-HARDENING.md), [DATABASE-PRIVILEGES.md](DATABASE-PRIVILEGES.md) |
| Encrypted backups, off-site copy | MISSING | `infra/demo/backup.sh` (plain dump, same host); owner-deferred off-site |
| Disk or volume encryption configuration | MISSING (not claimed; provider-dependent) | `/security` page |
| KMS integration, per-project data keys, key rotation | MISSING in the product; library core only, used by nothing | Section 7.11 status note; `crates/engine-core/src/at_rest/` |
| Typed, chunked, content-addressed state with structural records separate from payloads | EXISTS | [TYPED-CHECKPOINTS.md](TYPED-CHECKPOINTS.md), [CHUNKED-CONTENT.md](CHUNKED-CONTENT.md) |
| Client-side merge over typed state (CLI) | EXISTS | `repository/convergence.rs`, `project_merge.rs` |
| Browser import, export, fold and spreadsheet calculation | EXISTS | `frontend/researcher/src/lib/workspace/`, `materializer.ts` |
| Client-side engine for merge, diff and integrity in the browser | MISSING | [clients document, section 4](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md#4-the-engine-in-webassembly) |
| Encryption keys, sealed objects, epochs, grants | MISSING | Cryptographic design |
| Passphrase- or keystore-encrypted CLI signing key | PARTIAL (working tree) | `crates/engine-cli/src/keyfile.rs`, `keystore.rs` |
| Independent review, fuzzing, SBOM, signed releases, bounty | MISSING | [assurance plan, section 2](E2EE-THREAT-MODEL-AND-ASSURANCE.md#2-current-state-versus-target) |
| Self-hosted edition and agent connectors | Planned (documents in progress) | [SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md), [AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md) |

Production today: one VPS, schema 40, shared writers off, no receipt-signing authority
installed ([RECEIPT-AUTHORITY-DECISION.md](RECEIPT-AUTHORITY-DECISION.md)). The working tree
of 14 October 2026 contains concurrent, uncommitted work on authority rotation, key files and
local compression; rows marked working tree can change.

## 5. Options considered

| Option | Description | Protects against | Cost | Verdict |
|---|---|---|---|---|
| A. Infrastructure encryption only | Encrypted volumes and backups, no application change | Disk and backup theft | Very low | Part of Phase 0 (0a) |
| B. Application envelope encryption with customer KMS | Per-project data keys wrapped by a KEK in KMS or HSM; blobs and optionally JSON columns encrypted | Disk, backup, snapshot and some insider theft; gives rotation, revocation, crypto-erase | Medium; no client change | **Phase 0** |
| C. Encrypted projects with structure and signatures in clear | Clients seal payloads; service validates structure, signatures and graph | The service and its operators for content | High; clients and feature loss | **Phase 1** |
| D. Full client-side repository (local-first, service as dumb relay) | All logic on clients; service stores blobs and relays | Everything in C plus structure hiding | Very high; abandons server-side review and activity | Phase 3 option for a sealed-index tier |
| E. MLS-style group messaging protocol | TreeKEM epochs, ratcheting | Forward secrecy and post-compromise security | Very high; fits messaging, not archives | Rejected ([comparison](E2EE-CRYPTOGRAPHIC-DESIGN.md#13-comparison-with-other-designs-and-why-a-lighter-construction)) |
| F. Trusted execution (confidential computing) | Server process in an enclave with attested code | Operator without enclave break | High; trust moves to hardware vendor; weak match for self-host | Not planned; revisit only on explicit demand |

Decision matrix (1 poor to 5 good):

| Criterion | A | B | C | D | E | F |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| Protects against provider staff | 1 | 2 | 5 | 5 | 5 | 3 |
| Keeps current features | 5 | 5 | 3 | 1 | 2 | 5 |
| Time to first customer value | 5 | 4 | 2 | 1 | 1 | 2 |
| Review burden | 5 | 4 | 2 | 1 | 1 | 2 |
| Fits the signed-commit architecture | 5 | 5 | 5 | 4 | 2 | 5 |
| Works for self-hosted profiles | 5 | 5 | 4 | 3 | 2 | 2 |

## 6. Architecture

### 6.1 Components and trust (target, all phases)

```
 +--------------------------- member devices (trusted for plaintext and keys) ---------------+
 |  Browser workspace   CLI (engine-cli)   Office panes   Google sidebars   local MCP server |
 |  - key store, crypto module (WebCrypto / ring+HPKE)                                       |
 |  - engine: TS fold today, Rust-in-WASM in Phase 2 (merge, diff, integrity)                |
 +---------------------------------------+---------------------------------------------------+
                                         | TLS; payloads sealed (Phase 1+)
 ==================== service boundary: sees ciphertext + metadata =========================
 +---------------------------------------v---------------------------------------------------+
 | Web tier (static client delivery, proxy)   API (auth, authorization, structural validation,|
 | CAS refs, receipts, reviews, quotas, audit, key directory, grants, membership log)         |
 +---------------------------------------+---------------------------------------------------+
                                         |
 +---------------------------------------v--------------------+   +-------------------------+
 | PostgreSQL + object tables: ciphertext objects, signed      |   | KMS / HSM (Phase 0, 3) |
 | commits, receipts, key epochs, grants, logs                 |<->| customer-controlled    |
 | Phase 0 adds: envelope encryption of columns/blobs          |   +-------------------------+
 +------------------------------------------------------------+
 | Backups (encrypted, off host)    Converter (consent-only)   Admin console (counts only)   |
 +------------------------------------------------------------+
```

### 6.2 Data flows (Phase 1)

Write (commit path):

```
 editor -> client engine -> seal changed nodes (K_doc) -> typed v3 structure (clear) -> commit v4
       -> sign with device key -> stage objects -> stage commit -> publish -> CAS ref (receipt)
 service: verifies hashes, headers, signature, roles, quotas, mode; never decrypts
```

Review:

```
 reviewer client: fetch commit + objects -> open with epoch keys -> diff locally -> approve
 service: counts independent approvals by identity; enforces protected main
```

Removal:

```
 manager client: log entry member_remove + new epoch + grants for remaining devices (one request)
 service: removes membership, stores atomically, revokes sessions and grants
```

### 6.3 What stays where

| Concern | Service | Client |
|---|---|---|
| Authentication, sessions, device-key enrolment, per-project grants | Yes | Holds device keys |
| Authorization (roles, floors, approvals, CAS) | Yes, unchanged | Verifies signer roles from the signed log |
| Ordering and storage of ciphertext, quotas, retention, erasure | Yes | n/a |
| Content validation, merge, diff, integrity, search, similarity | No (encrypted projects) | Yes |
| Key generation, wrapping, rotation | No | Yes |
| Audit of operations (who, when, what class) | Yes (metadata) | Local logs optional |

## 7. Phase 0: customer-managed at-rest envelope encryption (not end-to-end)

### 7.1 What it is and is not

Phase 0 encrypts data at rest with keys the customer controls in their own KMS or HSM. The
running Dynodoc service **decrypts in memory to do its work**, exactly as today. It is not
end-to-end encryption and must not be described as such (claims policy level 1). Its value is
real and cheap: it protects against theft or exposure of storage media, snapshots, replicas
and backups, gives the customer rotation, revocation and project-level crypto-erasure, and
produces documented key management for audits.

It is the **fast path for self-hosted customers** (see
[SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md)) because the operator is the customer and the
residual threat is storage and backup exposure, not the application.

### 7.2 Tiers

| Tier | Scope | Changes | Protects | Does not protect |
|---|---|---|---|---|
| 0a Infrastructure | Encrypted volumes for PostgreSQL and object storage with customer-managed keys; encrypted, off-host, separately keyed backups; encrypted logs where they contain identifiers | Runbooks, `backup.sh` replacement, restore drill; no engine change | Disk and snapshot theft, decommissioned media, backup theft | Anyone with SQL or application access; cloud admin who can attach the volume while it is unlocked |
| 0b Blob columns | Application-level envelope encryption of the large, immutable content stores: `checkpoint_object.bytes`, `document_upload.content`, `provenance_bundle.body`, `snapshot.state`, `workspace_draft.base_state` | `EnvelopeStore` wrapper over `PgStore`; KMS provider; `data_key` table; backfill tool | As 0a, plus DB-level access (a DBA with `SELECT`) cannot read these without KMS | Derived and JSON columns in 0c; the running service |
| 0c JSON content columns | `event.payload`, `node.current_fields`, `draft_edit.payload`, `document.title`, `document_version.name`, issue text, `change_request_review.note`, `operation_audit.detail` where it holds text | Query changes: `node` search moves to in-process after decrypt; SQL access to `payload->>'field'` in `ops/apply.rs` moves to the application; new indexed clear columns for fields the SQL still needs | As 0b for those columns | Same; larger performance and compatibility cost |

Recommendation: 0a for every installation; 0b for installations whose profile requires
column-level protection; 0c only on explicit request (most of its value is covered by 0a and
0b for the stated threats, and it touches many queries).

### 7.3 Key hierarchy

```
 Customer KMS / HSM
   KEK_data   (never leaves the KMS)           KEK_backup (separate key, separate admins)
     |  wrap / unwrap with encryption context     |
     v                                            v
   DEK_p  one per project (and one installation DEK for non-project data)
          32 random bytes, stored wrapped in data_key; cached unwrapped in API memory
     |  HKDF-SHA-256 with per-row salt, info = table || column || row identity
     v
   K_row  single-use AES-256-GCM key and nonce per stored value
```

Properties: a KMS call per DEK per cache lifetime, not per row; encryption context binds the
wrapped DEK to the installation, project and purpose so it cannot be replayed elsewhere;
per-row keys avoid nonce-count limits (same reasoning as the
[sealing design](E2EE-CRYPTOGRAPHIC-DESIGN.md#84-per-object-keys-and-the-nonce-strategy)).

### 7.4 Stored form (proposed)

```
 DYNOENC\0 | version u8 | dek_id u32 BE | kek_version u32 BE | salt[32] | ciphertext | tag[16]
 AAD = "dynodoc.at-rest.v1" || installation_uuid || table || column || row_key
```

The magic differs from `DYNOSEAL` on purpose: an at-rest envelope is service-decryptable and
must never be mistaken for an end-to-end sealed object.

**Content-addressed tables.** `PgStore::put_batch` compares stored bytes with incoming bytes
for an existing address (`stored.bytes=input.bytes`, `shared_checkpoint/postgres.rs`). A
random salt would break that equality. For `checkpoint_object`, the salt is derived
deterministically: `salt = HMAC-SHA-256(DEK_p, "salt" || document || address)`. The same
(key, document, address) always encrypts the same plaintext (the address is a hash of it), so
a deterministic ciphertext reveals nothing beyond equality of identical objects, which the
address already reveals to anyone who can read the table today.

### 7.5 Provider abstraction

```rust
// proposed, in engine-core behind a feature
trait KeyProvider {
    fn wrap(&self, dek: &[u8; 32], context: &Context) -> Result<Wrapped, KmsError>;
    fn unwrap(&self, wrapped: &Wrapped, context: &Context) -> Result<[u8; 32], KmsError>;
    fn describe(&self) -> ProviderInfo;    // id, version, region, for audit
}
```

| Provider | Binding of the encryption context | Notes |
|---|---|---|
| AWS KMS | `EncryptionContext` on `Encrypt` and `Decrypt` | Customer-managed keys, optional external key store (HYOK-like) |
| Azure Key Vault or Managed HSM | AAD where the chosen wrap algorithm supports it (verify per algorithm) | Supports HSM-backed keys |
| Google Cloud KMS | `additionalAuthenticatedData` on `Encrypt` and `Decrypt` | Optional external key manager |
| HashiCorp Vault Transit | `context` parameter (with derived keys) | Common for self-hosted and air-gapped sites |
| PKCS#11 HSM through a small sidecar | Context hashed into the wrap input | For on-premises HSMs; sidecar isolates the vendor library |
| Local key file (passphrase) | Context as AAD | Weakest; for evaluation and small sites only; documented as such |

Provider specifics and prices are not verified here; confirm at implementation time.

### 7.6 Flows

| Flow | Behaviour |
|---|---|
| Startup | Provider health check; load active `data_key` rows lazily; refuse to start in `envelope` mode if the provider is unreachable and no cached key exists (configurable: `deny_on_unavailable`) |
| Write | Get DEK for the project from cache or KMS; derive `K_row`; encrypt; store `DYNOENC` bytes |
| Read | Parse header; fetch DEK by `dek_id` (cache or unwrap); decrypt; validate address and codec as today |
| Cache | Unwrapped DEKs held in memory with a TTL (default 15 minutes), zeroized on eviction best effort; no unwrapped DEK is written to disk |
| KEK rotation | New KMS key version; a background job rewraps `data_key.wrapped` for all DEKs (`engine-cli kms rewrap`); no row data is touched |
| DEK rotation | New DEK per project (scheduled or on demand); new writes use it; old rows remain decryptable (`decrypt_only`) and a background job re-encrypts them over time where the table allows (section 7.8) |
| Revocation (kill switch) | Customer disables the KEK in KMS; after cache TTL the service can no longer unwrap; all covered data becomes unreadable; restore by re-enabling |
| Crypto-erase of a project | Destroy the project's wrapped DEK rows and the KMS key version if it was dedicated; covered rows and backups become unrecoverable; recorded through the existing audited erasure receipt path |
| Backup | `pg_dump` piped to an encryptor with recipients from the offline backup key (age or KMS envelope), written off host; restore requires the backup key, not the live KEK |
| Restore drill | Quarterly: restore into an isolated host with keys from escrow; verify event chains and snapshots (`engine-cli verify`, `verify-snapshots`) |

### 7.7 Proposed SQL and configuration

```sql
-- PROPOSED. Placeholder number 00NN; not created.
create table kms_key (
    id          uuid primary key default gen_random_uuid(),
    provider    text not null check (provider in
                  ('aws-kms','azure-keyvault','gcp-kms','vault-transit','pkcs11','file')),
    key_ref     text not null,              -- ARN, URL or path; never secret material
    purpose     text not null check (purpose in ('data','backup')),
    status      text not null check (status in ('active','retiring','retired','disabled')),
    created_at  timestamptz not null default now()
);
create table data_key (
    id            integer generated always as identity primary key,
    project_id    uuid references project_settings(space_id),   -- null: installation scope
    kms_key_id    uuid not null references kms_key(id),
    kek_version   text not null,
    wrapped       bytea not null check (octet_length(wrapped) between 16 and 4096),
    state         text not null check (state in ('active','decrypt_only','destroyed')),
    created_at    timestamptz not null default now(),
    retired_at    timestamptz
);
create unique index data_key_active on data_key(coalesce(project_id,
    '00000000-0000-0000-0000-000000000000'::uuid)) where state = 'active';
```

```yaml
# proposed installation configuration (environment or file)
at_rest_encryption:
  mode: envelope                 # off | envelope
  tiers: [infrastructure, blobs] # infrastructure | blobs | json
  provider:
    type: vault-transit          # aws-kms | azure-keyvault | gcp-kms | vault-transit | pkcs11 | file
    endpoint: https://vault.example.internal:8200
    key: dynodoc-data
    cache_ttl_seconds: 900
    deny_on_unavailable: true
  backup:
    recipients: ["age1qqqq...offline-held-recipient"]
    destination: "s3://backup.example.internal/dynodoc/"
    retention_days: 35
```

### 7.8 Migration of existing data (the immutability constraint)

Existing rows in `checkpoint_object`, `snapshot`, `draft_edit`, `document_version` and
`provenance_bundle` are guarded by `before update` triggers that call `event_immutable()`
(`migrations/0012_*`, `0025_*`, `0028_*`). A backfill that issues `UPDATE ... set bytes =`
would be refused, and the runtime role has no right to try. Options:

| Option | Method | Trade-off |
|---|---|---|
| M-a Re-encrypt on restore | Dump, transform with `engine-cli kms reencrypt` offline, restore into a new database during a maintenance window | Safe; downtime; works for every table |
| M-b Operator-only function | A migration-owned `SECURITY DEFINER` function permitted to replace `bytes` only when the replacement decrypts to the identical plaintext (hash and address re-verified) and writes an audit receipt | Online; needs a reviewed migration and an exception to immutability; operator privilege only |
| M-c New writes only | Encrypt only rows written after enablement; cover the old rows with tier 0a | No exception; partial column-level coverage; clear statement required |

Default: **M-c immediately (so new data is protected), then M-a at the next maintenance
window.** M-b is available if downtime is unacceptable and the schema guardian review
accepts the exception.

### 7.9 What Phase 0 does and does not protect

| Adversary | Phase 0 outcome |
|---|---|
| Disk, snapshot or decommissioned-media thief | Cannot read (tier 0a) |
| Backup thief | Cannot read if backup key is not co-located |
| Cloud provider staff with storage access | Cannot read blobs at rest; may read memory of a running VM unless confidential computing is used |
| DBA with SQL access to covered columns | Cannot read tier 0b columns; **can read** uncovered columns (0c) and can act through the application |
| Operator with shell on the application host | Can obtain unwrapped DEKs from process memory or call KMS as the service; **not protected** |
| Compromised application | **Not protected**; plaintext in memory and derived tables |
| Customer revokes KEK | Service loses access to covered data (feature) |
| Subpoena to the service operator | Content producible by the operator with KMS cooperation |
| Self-hosted customer's own staff | Same as the DBA and operator rows |

### 7.10 Failure modes

| Failure | Effect | Mitigation |
|---|---|---|
| KMS outage | New unwraps fail; cached DEKs work until TTL | Cache TTL tuning; regional replicas; documented degraded mode; alert |
| KMS key deleted by mistake | Data unrecoverable except from backups with their own key | KMS deletion protection and waiting periods; separate backup key; drill |
| Wrong `dek_id` or corruption | Decrypt fails, row flagged | Checksums, verification job |
| Clock or region failover | None (no time dependence in format) | n/a |
| Cache memory exposure | DEKs readable by process memory attacker | Out of Phase 0 scope; stated |
| Backup key lost | Backups unreadable | Two custodians, offline copy, restore drill |

### 7.11 Effort

| # | Task | Person-weeks |
|---|---|---:|
| P0-1 | Infrastructure runbooks, encrypted backup tooling, restore drill, documentation | 3 |
| P0-2 | `KeyProvider` trait with two reference providers (Vault Transit, one cloud KMS), file provider | 5 |
| P0-3 | `EnvelopeStore` wrapper, `DYNOENC` format, vectors, tier 0b columns | 6 |
| P0-4 | `data_key` tables, rotation jobs, kill switch, crypto-erase integration | 4 |
| P0-5 | Backfill tooling (M-a, M-c), verification, rehearsal on a production copy | 3 |
| P0-6 | Observability, failure injection, internal review | 2 |
| | **Total** | **23** |

Assumptions: two Rust engineers, one DevOps engineer at half time, the engine's existing
test infrastructure. Confidence: medium (range 18 to 30 person-weeks). Excludes: additional
KMS providers beyond the references (about 2 person-weeks each), tier 0c (about 8
person-weeks), PKCS#11 vendor qualification, FIPS-validated module integration, performance
tuning for 100 MB files, external review of the envelope design.

**Implementation status, at-rest envelope library core (P0-2 and P0-3 in part), 20 October
2026.** Built, in `crates/engine-core/src/at_rest/` and nowhere else (one `pub mod at_rest;`
line in `lib.rs`; no new dependency; no storage, API, CLI or migration code calls it): the
`DYNOENC` stored form of 7.4 (`seal`, `seal_content_addressed`, `open`, `Header::parse`), with
a per-value key and nonce derived by HKDF-SHA-256 from the data key, AES-256-GCM through
`ring`, a length-framed AAD, strict parsing with a distinct error per rule, and the
deterministic salt of 7.4 for content-addressed tables; the `KeyProvider` trait (`wrap`,
`unwrap`, `rewrap`, `describe`), `FileKeyProvider` (KEK versions in owner-only files, bounded,
rotation by adding a version), a deterministic `TestKeyProvider`, and a `DekCache` with a
lifetime and a size bound on an injectable clock; zeroising `Dek`/`Kek`/`DataKey`. Evidence:
68 tests (48 unit, 13 property and tamper, 7 vector) including every-byte tamper, AAD binding,
key and nonce uniqueness, fixed-seed parser runs on arbitrary bytes, and a KEK rotation, DEK
rotation and kill-switch walk-through; `tests/fixtures/at-rest-v1.json` from the independent
Python generator `gen_at_rest_vectors.py` (PyPI `cryptography`, `hmac`, `hashlib`),
reproduced byte for byte by the Rust tests. Refinements of 7.4 (length-framed AAD that also
binds `version`, `dek_id` and `kek_version`; framed salt input) and four owner questions are
in [AT-REST-ENVELOPE.md](AT-REST-ENVELOPE.md). **Not built:** `EnvelopeStore` and any use
by `PgStore`, snapshots, uploads or drafts; the `kms_key` and `data_key` tables and every
migration; backfill (M-a, M-c) and the `engine-cli kms` commands; the Vault Transit and cloud
KMS providers and any asynchronous provider interface; the configuration of 7.7; tiers 0a
and 0c; the restore drill; observability and failure injection. Nothing here is a public
claim: claims level 1 still needs Phase 0 shipped and a passed restore drill. **Not
verified:** the access list of a key file is not checked on Windows (the Unix mode check
is); macOS was not run; the vectors share a reading of the design with the Rust code, no
external review of the construction has taken place, and the cache and wipe behaviour were
not measured under load.

## 8. Phase 1: encrypted projects

| Item | Statement |
|---|---|
| Scope | Opt-in per project. Structure (ids, tree, `pos`, types, revisions), signatures, receipts and reviews stay in the clear; payloads (fields, comments, names, messages, uploads) are sealed on clients. Specified in the [protocol](E2EE-PROTOCOL-AND-MIGRATION.md) and [cryptographic](E2EE-CRYPTOGRAPHIC-DESIGN.md) documents |
| Clients | `engine-cli` (reference), basic browser workspace (view, single-file edit through the TS fold), no add-ins, no browser merge or review |
| Server-side features lost | Search, similarity, public links, conversion, server merge and diff; see the [matrix](E2EE-PROTOCOL-AND-MIGRATION.md#11-feature-by-feature-matrix) |
| Migration | M1 "seal from now" with successor documents; legacy plaintext retained until erased ([protocol 9](E2EE-PROTOCOL-AND-MIGRATION.md#9-migrating-a-project-in-place)) |
| Recovery | Other device, recovery key, member re-grant; escrow primitives present, UI in Phase 3 |
| Exit | Gates G1, G2 passed for the claims of level 3; acceptance criteria of the protocol and clients documents |
| Effort | About 168 person-weeks including assurance (section 12) |

## 9. Phase 2: client-side engine in WebAssembly

| Item | Statement |
|---|---|
| Scope | Feature-gate `engine-shared` and `engine-core`, fix the time-based diff deadline, build `engine-wasm`, integrate in the browser worker, add-in panes and sidebars; client-side search index and review rendering |
| Deliverables | Browser merge, diff, review, restore on encrypted projects; Office and Google delegation flows; local MCP server |
| Exit | Parity corpus identical to native; size and timing thresholds ([clients 14](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md#14-delivery-plan-clients-workstream)); installed-host acceptance for Word and a Google test deployment |
| Independent value | The refactor and determinism fix benefit the existing engine and the unfinished "decision 014" browser-authored branches |
| Effort | About 54 person-weeks |

## 10. Phase 3: enterprise key management

| Item | Statement |
|---|---|
| Scope | Organisation escrow with threshold custody and ceremony tooling; HYOK key service integration; hardware key factors (WebAuthn PRF, FIDO2, PIV); offboarding automation (SSO and SCIM driven rotation); sealed index nodes (structure-hiding tier); M2 history re-seal; transparency log integration of membership heads |
| Definitions | **BYOK**: customer key held by the provider (Phase 0 style); **HYOK**: key stays with the customer and is used through a service; **escrow**: organisation as additional recipient (clients document, section 11) |
| Exit | Review of the escrow and key-service designs; customer pilot in a regulated profile |
| Effort | About 44 person-weeks |

## 11. Product behaviour and interplay

### 11.1 Creating and using an encrypted project (Phase 1 journey)

| Step | What the user sees |
|---|---|
| 1 | "New project" offers "Encrypted project" with a one-screen explanation: what stays private, what the service still sees, what stops working (search, public links, conversion in the service), and that nobody at Dynodoc can reset access |
| 2 | Create or confirm a recovery key and at least one more device or member; refuse sole-owner projects without a recovery path unless the user types the project name |
| 3 | Invite members: the client shows each invitee's device fingerprint to compare; members without a device key become "pending key" |
| 4 | Work as usual: import, edit, commit, request review, approve, merge. Merge and review are CLI-only in Phase 1; browser from Phase 2 |
| 5 | Remove a member: the client opens a new epoch; the project shows "rotation pending" until it completes |
| 6 | Lose a device: revoke it, re-grant from another device, or recover with the key |

### 11.2 Organisation policy

Proposed organisation settings (audited, visible to members): new projects `forbidden`,
`optional` or `required` to be encrypted; escrow `none`, `optional` or `required`; agent delegation
`none`, `local_only` or `export_allowed`; default history policy; maximum days legacy
plaintext may remain after migration. Defined in the
[protocol document, 8.3](E2EE-PROTOCOL-AND-MIGRATION.md#83-organisation-policy-proposed).

### 11.3 Interplay with the self-hosted edition

[SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md) (concurrent, not authoritative here) plans
three profiles. The relevance of each phase differs because the operator is the customer:

| Profile (decision 015) | Operator is | Main residual threat | Recommended path |
|---|---|---|---|
| Small team or lab on one node | The lab | Stolen laptop or backup | Phase 0 tier 0a: encrypted volume, encrypted off-host backups, restore drill. Phase 1 rarely needed |
| Department or university with SSO | Central IT | Insider access, backup exposure, central IT versus a research group | Phase 0 tiers 0a and 0b with Vault Transit or cloud KMS; Phase 1 for groups that must exclude central IT; escrow with SSO offboarding (Phase 3) |
| Enterprise or regulated site, HA, intranet-only or air-gapped | The enterprise | Regulators, insider threat, supplier access | Phase 0 with HSM (PKCS#11); Phase 1 with CLI and customer-run web delivery; HYOK service (Phase 3); no browser code delivery by a third party |

Hosted Dynodoc (operated by Verne) is the case where Phase 1 changes the trust model most:
the operator is not the customer. Self-hosted customers gain from Phase 1 mainly against
their own administrators and cloud provider.

### 11.4 Interplay with AI-agent connectors

[AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md) plans connectors for Claude first, then
ChatGPT (apps, connectors, actions) and others. The interaction with encryption, stated
precisely (detail and consent flow in the
[clients document, section 8](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md#8-local-mcp-and-ai-agents)):

1. **Phase 0 projects**: connectors work as for any project; the service decrypts in memory.
   The claims policy must say that the service can read content.
2. **Phase 1 projects**: a connector hosted by Dynodoc cannot read content because the service
   holds no keys. It can serve metadata only. The only agent paths that read content are a
   **local server that holds delegated keys** on the user's device, a customer-operated
   gateway (Phase 3), or the user's own export of plaintext.
3. **Consent and warnings** are mandatory: the user is told that text leaves the device for
   the AI provider they configured and that Dynodoc's encryption does not apply afterwards;
   members can see that a delegation exists; organisation policy can forbid it.
4. **Writes** from agents are proposals signed by an agent device key and cannot approve their
   own work.

### 11.5 Public wording to change, and when

| Surface | Today | Change at |
|---|---|---|
| `/security` "Limits and what we do not claim" | "Not end-to-end encrypted... A particular encryption-at-rest configuration is not claimed." | Level 1: add customer-managed at-rest statement with the limit. Level 2 and 3 per the claims policy |
| `/privacy` | "Dynodoc is not end-to-end encrypted: it reconstructs and reviews versions on the server" | Level 3: scope to "projects not created as encrypted"; add encrypted-project paragraph with metadata list |
| FAQ, `llms.txt`, `llms-full.txt`, JSON-LD | Mirror the above | Same gates |
| Add-in listings and data handling ([listings/addin-data-handling.md](listings/addin-data-handling.md)) | Describes plaintext flows | Phase 2: describe sealing and delegation; marketplace privacy forms updated |
| Legal pages ([LEGAL-PAGES.md](LEGAL-PAGES.md)) | Non-claims "must stay" | Change only with the claims ladder and counsel |

## 12. Delivery plan

### 12.1 Phases, effort and calendar

All estimates are person-weeks of Verne engineering and security effort, with the exclusions
stated per workstream in each document.

| Phase | Workstreams | Person-weeks | Range | Calendar (6 FTE) | Depends on |
|---|---|---:|---:|---|---|
| 0 | Section 7.11 | 23 | 18 to 30 | 3 to 4 months | None |
| Engine refactor (early, independent) | Clients K6, determinism fix B5 | 8 | 6 to 12 | 6 to 8 weeks | None |
| 1 | Crypto 36, protocol 44, clients Phase 1 43, assurance 45 | 168 | 130 to 230 | 7 to 9 months | Demand evidence; engine refactor optional |
| 2 | Clients K7 to K11 and tests | 46 (plus 8 refactor above = 54) | 42 to 80 | 4 to 6 months | Phase 1 formats, engine refactor |
| 3 | Clients K12 14, sealed index 8, M2 re-seal 10, offboarding automation 6, transparency integration 6 | 44 | 32 to 70 | 5 to 6 months | Phases 1 and 2 |
| **Total** | | **about 289** | **about 220 to 420** | 16 to 20 months with overlap | |

Staffing assumptions: six full-time equivalents: three Rust and cryptography-capable
engineers, two TypeScript and WebCrypto engineers, one security engineer; plus half a
designer, a technical writer and a product owner. External reviewer fees, legal fees,
hardware and bounty rewards are **not** included. Confidence: medium for Phase 0, medium for
Phase 1 protocol and crypto, low to medium for Phase 2 (WASM size, determinism, host
behaviour), low for Phase 3.

```
 month   1  2  3  4  5  6  7  8  9 10 11 12 13 14 15 16
 Phase 0 ===========
 refactor   ======
 Phase 1          decision gate ^   ================================  (G1 early, G2 late)
 Phase 2                                              ======================
 Phase 3                                                         ======================
```

### 12.2 Dependencies and ordering rules

1. Phase 0 and the engine refactor start independently.
2. Phase 1 starts only after the go decision (12.3) and G0.
3. No Phase 1 production code before G1 (specification review).
4. Phase 2 may start the WASM spike during Phase 1 because it only needs the pure modules.
5. Phase 3 items are chosen by customer demand; none blocks Phases 1 or 2.

### 12.3 Go and no-go checklist for Phase 1

**Customer demand evidence (all required before starting Phase 1):**

| # | Evidence | Threshold |
|---|---|---|
| D1 | Written statements from independent organisations (letter of intent, procurement requirement, security questionnaire answer, or pilot agreement) that policy requires the hosting provider to be unable to read content, and that customer-managed at-rest encryption (Phase 0) is not sufficient | At least 3 organisations, at least 1 in academia and 1 enterprise |
| D2 | A design partner willing to run a beta, accepting the degraded features (no server search, no public links, no server-side conversion, CLI-first merge and review) | At least 1, with a named sponsor |
| D3 | Interviews confirming acceptance of the key-loss model and recovery responsibilities | At least 5, recorded |
| D4 | Lost or blocked opportunities attributable to missing end-to-end encryption in the last two quarters, with reason codes from sales or support | At least 2, or a stated regulatory driver |
| D5 | Share of customers hosted by Verne versus self-hosted | If most target customers self-host, re-evaluate: Phase 0 may suffice |
| D6 | Evidence that Phase 0 was offered and rejected, or an explicit reason it cannot satisfy the requirement | Recorded |

**Internal readiness (all required):**

| # | Item | Test |
|---|---|---|
| R1 | Phase 0 shipped, or consciously waived with owner sign-off | Release record |
| R2 | Open decisions closed (OQ-1 to OQ-4 below; crypto OQ-CR-1 to 4; protocol OQ-PR-1 to 4) | G0 record |
| R3 | External review budget approved and two reviewers identified | Contract or quote |
| R4 | Staffing of the six FTE profile for at least 9 months | Plan |
| R5 | CI hygiene: dependency audit, SBOM, fuzz infrastructure started | CI jobs green |
| R6 | Claims ladder approved by owner and counsel | Document |
| R7 | Support playbooks and recovery policy drafted | Documents |
| R8 | Formula engine replacement and add-in acceptance not blocking (decision 015) | Status |

**No-go triggers (any one):** demand evidence incomplete; Phase 0 meets the stated
requirement; cannot fund external review; the product direction changes toward real-time
collaboration that this architecture does not serve; a critical open decision on key
custody or recovery responsibility.

### 12.4 Acceptance and test strategy (summary)

Each phase has acceptance criteria in its document: Phase 0 in section 7 (restore drill,
failure injection, kill-switch test), Phase 1 in the
[protocol](E2EE-PROTOCOL-AND-MIGRATION.md#125-acceptance-criteria-for-phase-1-protocol-work)
and [clients](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md#14-delivery-plan-clients-workstream)
documents, and the gates G0 to G5 in the
[assurance plan](E2EE-THREAT-MODEL-AND-ASSURANCE.md#102-gates).

### 12.5 Rollout and migration

| Stage | Action |
|---|---|
| Internal | Verne's own projects on a staging stack; storage scan and adversarial-server tests |
| Design partners | Invite-only encrypted projects behind `E2EE_ALLOW_CREATE`; private bounty; weekly review |
| Beta | Documented limits; level 2 wording; no marketing claim |
| General availability | G2 and G4 passed; level 3 wording; support readiness |
| Existing plaintext projects | Untouched until their owners migrate (M1); never forced |

## 13. Operations

| Topic | Phase 0 | Phase 1 and later |
|---|---|---|
| Deployment | Provider configuration, KMS access policy, backup destination; no new service | New tables, endpoints; clients with sealing; optional witness and key services |
| Configuration | `at_rest_encryption` block (7.7) | `E2EE_ENABLED`, `E2EE_ALLOW_CREATE`, budgets ([protocol 12.1](E2EE-PROTOCOL-AND-MIGRATION.md#121-configuration-proposed)) |
| Upgrade | Rolling application upgrade; KEK and DEK rotation are background jobs | Server and client formats versioned; capability negotiation; old clients fail closed |
| Rollback | Disable `envelope` mode only for new writes; existing envelope rows remain readable by builds that include the decrypt path | `E2EE_ENABLED=false` serves ciphertext read-only; no database downgrade |
| Observability | KMS latency and error rates, cache hit ratio, rows by DEK, rewrap progress, backup age | Epoch age, grants unclaimed, validation failures, `encrypted_project` errors by feature, migration state |
| Failure modes | KMS outage, key deletion, backup key loss (7.10) | Lost keys, rotation pending, fork or rollback detection, unsupported client, malformed sealed events |
| Incident response | KEK compromise: rotate KEK, rewrap, review KMS audit; DEK exposure: rotate DEK, re-encrypt | Key leak: revoke devices, rotate epochs, optional re-seal; malicious bundle: advisory, forced client update, communication plan |
| Support | Standard runbooks | Support cannot recover keys or read content; playbooks in clients document |

## 14. Risks and mitigations

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Phase 0 mistaken for end-to-end encryption in sales or documentation | High | High | Claims policy level 1 wording; training; review of every customer-facing change |
| R2 | Phase 1 effort exceeds estimate | Medium | High | Go/no-go gating; spike WASM and sealing early; scope control (CLI-first) |
| R3 | Key loss incidents damage trust | Medium | High | Mandatory recovery path; escrow option; clear copy |
| R4 | Browser code delivery undermines claims | High | High | Say it; recommend CLI; code integrity controls; self-hosting |
| R5 | Feature loss disappoints customers | High | Medium | Matrix in sales material; client-side alternatives; typed errors |
| R6 | Cryptographic flaw found late | Medium | Critical | G1 before code; formal model; independent review; bounty; small construction |
| R7 | Migration leaves legacy plaintext, users overestimate protection | Medium | High | Labels; deadline policy; erasure workflow; claims |
| R8 | Engine determinism or WASM size blocks Phase 2 | Medium | Medium | Early spike; refactor first; CLI fallback |
| R9 | Concurrent work (authority rotation, pruning, witnesses) changes assumptions | Medium | Medium | Cross-link, re-verify at G0; this set marks working-tree dependence |
| R10 | Regulatory expectations differ from assumptions | Low to medium | Medium | Counsel review; mapping is guidance only |
| R11 | Support burden for recovery | Medium | Medium | Self-service flows; playbooks |

## 15. Open questions

| # | Question | Recommended default | Decider |
|---|---|---|---|
| OQ-1 | Adopt Phase 0 now as the first deliverable, independent of Phase 1? | Yes | Owner |
| OQ-2 | Is column-level encryption (tier 0b) in scope for the first self-hosted release, or only infrastructure encryption (0a)? | 0a first; 0b in the same quarter if a customer asks | Owner with self-hosted lead |
| OQ-3 | Who owns key custody policy in the hosted service (Verne's own KMS for Phase 0 on hosted)? | Not offered on hosted until Phase 1; hosted uses provider volume encryption with a stated limit | Owner |
| OQ-4 | Does the hosted service (Verne Systems) take on Phase 1 operationally given a single-VPS deployment and same-host backups? | Only after off-site, encrypted backups exist | Owner |
| OQ-5 | Minimum viable Phase 1 clients: CLI only, or CLI plus basic browser? | CLI plus basic browser | Product owner |
| OQ-6 | Support and legal position on unrecoverable projects | Documented acceptance at creation; no operator recovery | Owner with counsel |
| OQ-7 | Hardware keys for owners in regulated profiles | Optional Phase 3 | Customer-driven |
| OQ-8 | Whether escrow is enabled by default for organisations | No; explicit policy | Organisation administrators |
| OQ-9 | Publication of design documents and review reports | Publish the design set and a review summary | Owner |
| OQ-10 | Interaction with the planned witness and pruning designs: embed membership heads and honour sealed refs | Yes (compatible by design) | Engineering lead |
| OQ-11 | Phase 1 file cap (2048 files per commit) acceptable? | Yes until indexed path maps | Engineering lead |
| OQ-12 | Which KMS providers are reference implementations? | Vault Transit and one hyperscaler KMS; others on request | Engineering lead |

## Appendix A: glossary

| Term | Meaning |
|---|---|
| At-rest encryption | Encryption of stored data; the running service can decrypt |
| BYOK | Bring your own key: customer supplies or controls a key held by the provider's key service |
| DEK, KEK | Data encryption key, key encryption key |
| End-to-end encryption | Content encrypted on members' devices; the service cannot decrypt it |
| Epoch | Period with one project epoch root (Phase 1) |
| Escrow | Organisation as an additional recipient of keys |
| HYOK | Hold your own key: key stays in customer systems |
| KMS, HSM | Key management service, hardware security module |
| Sealed object | Authenticated ciphertext container with clear header |
| Successor document | New document identity created when a project migrates |
| Zero knowledge | Term this product does not use |

## Appendix B: references

External references and dates are collected where they are used:
[cryptographic design, appendix C](E2EE-CRYPTOGRAPHIC-DESIGN.md#appendix-c-references)
(RFCs, WebCrypto support, HPKE libraries), [clients document, appendix B](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md#appendix-b-references)
(Office and Apps Script constraints, WebAuthn PRF) and
[assurance plan, appendix B](E2EE-THREAT-MODEL-AND-ASSURANCE.md#appendix-b-references)
(tools and frameworks). Repository references:

| Document | Role |
|---|---|
| [PRODUCT-SPEC.md](PRODUCT-SPEC.md) | States that an optional client-encrypted mode needs a separate search, review and key-sharing design (this set) |
| [decisions/012](decisions/012-local-first-document-version-control.md), [013](decisions/013-private-application-licence.md), [014](decisions/014-web-branch-authoring-and-follows.md), [015](decisions/015-customers-formula-engine-and-planned-products.md) | Product direction, licensing, browser signing limits, planned products |
| [ENGINE-EVOLUTION-PLAN.md](ENGINE-EVOLUTION-PLAN.md) | Build order into which this work fits |
| [SECURITY-HARDENING.md](SECURITY-HARDENING.md), [DATABASE-PRIVILEGES.md](DATABASE-PRIVILEGES.md), [BROWSER-AND-WORKLOAD-SECURITY.md](BROWSER-AND-WORKLOAD-SECURITY.md) | Current controls |
| [REF-RECEIPTS.md](REF-RECEIPTS.md), [REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md), [RECEIPT-AUTHORITY-DECISION.md](RECEIPT-AUTHORITY-DECISION.md) | Receipts and authority |
| [DESIGN-WITNESSES-AND-TRANSPARENCY.md](DESIGN-WITNESSES-AND-TRANSPARENCY.md), [DESIGN-COMMIT-PRUNING.md](DESIGN-COMMIT-PRUNING.md) | Concurrent engine designs, not authoritative |

## Appendix C: draft decision text (for the owner)

Proposed **decision 016**, not recorded:

> 1. Adopt customer-managed at-rest envelope encryption (Phase 0) as the first confidentiality
>    deliverable for the self-hosted edition. It is not end-to-end encryption and is described
>    as such.
> 2. Treat encrypted projects (Phase 1) as conditional on the demand evidence of the master
>    design, section 12.3.
> 3. Adopt the claims policy of section 3. "End-to-end encrypted" is used only after gate G2;
>    "zero knowledge" is not used.
> 4. Schedule the engine refactor (feature gates, deterministic diff) independently.
