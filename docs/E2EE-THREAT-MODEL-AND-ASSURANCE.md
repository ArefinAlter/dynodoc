# End-to-end encryption: threat model and assurance plan

Status: design, not implemented. Date: 14 October 2026. Owner: Verne Ltd. Documentation only.
This is a threat model of a **proposed** design, not an assessment of a product. Nothing here
is an assurance claim about the running service.

Part of a set. Master design: [END-TO-END-ENCRYPTION.md](END-TO-END-ENCRYPTION.md).
Companions: [cryptographic design](E2EE-CRYPTOGRAPHIC-DESIGN.md),
[protocol and migration](E2EE-PROTOCOL-AND-MIGRATION.md),
[clients and key management](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md). Related, concurrent and not
authoritative here: [SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md),
[AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md),
[DESIGN-WITNESSES-AND-TRANSPARENCY.md](DESIGN-WITNESSES-AND-TRANSPARENCY.md).

[Decision 015](decisions/015-customers-formula-engine-and-planned-products.md) states that
anything marketed as "end-to-end encrypted" or "zero knowledge" needs the independent review
described in section 10 of this document first.

## 1. Purpose, goals, non-goals, personas, metrics

### 1.1 Purpose

1. Name the assets, adversaries and trust boundaries of an encrypted Dynodoc project.
2. Show, per adversary and per phase, what they can and cannot do, with a STRIDE table for the
   components.
3. Inventory the metadata the service and network observers still learn, and the traffic
   analysis that follows.
4. State endpoint compromise as out of scope, plainly.
5. Define the assurance plan that must be completed before any strong public claim:
   independent cryptographic review, formal methods options, fuzzing, bug bounty,
   transparency and certification mapping.

### 1.2 Goals

| ID | Goal |
|---|---|
| TM-G1 | A reviewer can enumerate every adversary assumption and find the control or the stated residual risk. |
| TM-G2 | Product and legal wording can be checked against this document: no claim exceeds the evidence. |
| TM-G3 | The assurance plan has dated gates, owners and exit criteria, not intentions. |

### 1.3 Non-goals

- Penetration test or code audit results (none exist).
- A compliance attestation. Mapping to standards is guidance for scoping, not a statement of
  compliance, and is not legal advice.
- Modelling the host editors (Word, Excel, PowerPoint, Docs, Sheets, Slides) beyond stating
  that they hold plaintext.

### 1.4 Personas

| Persona | Question they bring |
|---|---|
| Customer security architect | Who can read my content, and what proof do you give? |
| Customer counsel or DPO | What happens on a subpoena, a breach, an erasure request? |
| External cryptographic reviewer | What is the claimed property, and where is the attack surface? |
| Verne engineering lead | Which gate blocks the next release? |
| Verne owner | Which sentence may we publish now? |

### 1.5 Success metrics

| Metric | Target |
|---|---|
| Open critical or high findings at general availability | 0 |
| Findings from external review reproduced by a regression test or vector | 100 percent |
| Public claims traceable to a gate in section 10 | 100 percent |
| Time from valid external vulnerability report to fixed release for a high severity item | Under 30 days (target; to be confirmed against staffing) |
| Fuzz target coverage of parsers listed in section 10.5 | 100 percent with no unresolved crash older than 14 days |

## 2. Current state versus target

Basis: repository HEAD `994c429` plus the working tree of 14 October 2026.

| Control or evidence | State | Evidence |
|---|---|---|
| Hash-chained append-only event log, verification API and CLI | EXISTS | `crates/engine-core/src/log.rs`; [VERIFIED-SNAPSHOTS.md](VERIFIED-SNAPSHOTS.md) |
| Restricted runtime database login, startup privilege checks | EXISTS | [DATABASE-PRIVILEGES.md](DATABASE-PRIVILEGES.md); `crates/engine-core/src/database.rs` |
| CSP with nonces, HSTS, scoped framing, workload admission | EXISTS | [BROWSER-AND-WORKLOAD-SECURITY.md](BROWSER-AND-WORKLOAD-SECURITY.md); `frontend/researcher/src/lib/server/security.ts` |
| Signed project commits, ref receipts, client pins | EXISTS (authority not installed in production) | [REF-RECEIPTS.md](REF-RECEIPTS.md), [RECEIPT-AUTHORITY-DECISION.md](RECEIPT-AUTHORITY-DECISION.md) |
| Receipt-authority rotation and recovery | PARTIAL (working tree, schema 41, not deployed) | [REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md) |
| Per-project device-key grants and review scope | EXISTS (schema 40) | [DEVICE-KEY-GRANTS-API.md](DEVICE-KEY-GRANTS-API.md) |
| Property-based tests of hash chain tamper and reorder | EXISTS | `crates/engine-core/src/log.rs` (proptest, 1,000 cases) |
| Independent audit, penetration test or certification | MISSING (stated publicly) | `/security` page: "No independent assurance"; [LEGAL-PAGES.md](LEGAL-PAGES.md) |
| Fuzzing | MISSING | No fuzz targets or `cargo-fuzz` configuration in the repository |
| Dependency vulnerability scanning in CI | MISSING | `.github/workflows/ci-rust.yml` runs fmt, clippy, build, test, OpenAPI drift, source packaging; no audit or deny step |
| SBOM, signed releases, reproducible builds | MISSING | None found |
| Vulnerability disclosure channel | EXISTS | `frontend/researcher/static/.well-known/security.txt` (RFC 9116; email and GitHub private reporting for the engine repository) |
| Bug bounty | MISSING (stated: "No bug bounty is offered") | Same file |
| Independent witnesses, transparency log | MISSING (design only) | [DESIGN-WITNESSES-AND-TRANSPARENCY.md](DESIGN-WITNESSES-AND-TRANSPARENCY.md) |
| Confidentiality of content against the service | MISSING | `/security` and `/privacy`: "Not end-to-end encrypted"; no at-rest claim |
| Encrypted backups | MISSING | `infra/demo/backup.sh` writes plain dumps on the same host |
| Threat model for E2EE | This document | |

## 3. System model, assets and trust boundaries

### 3.1 Assets

| ID | Asset | Confidentiality | Integrity | Availability |
|---|---|:-:|:-:|:-:|
| A1 | Document content: text, formatting, cell values, comments, suggestions | High | High | High |
| A2 | Names: file names, titles, project names, version names, issue text, commit messages | High | Medium | Medium |
| A3 | Epoch roots, document keys, device private keys, recovery keys, escrow key | Critical | Critical | High |
| A4 | Membership and role records (log, space membership) | Medium | Critical | Medium |
| A5 | History: event chain, commits, signatures, receipts, reviews | Low (content sealed) | Critical | High |
| A6 | Account identity and session state | Medium | High | Medium |
| A7 | Metadata: structure, timing, sizes, graph shape | Medium | Low | Low |
| A8 | Client software and update channel | n/a | Critical | Medium |
| A9 | Backups and exports | As A1 | As A5 | High |
| A10 | Audit records | Low | Critical | High |

### 3.2 Trust boundaries

```
  USER DEVICE (trusted for A1, A3 in plaintext)             HOSTS (trusted by the user, not by us)
 +-----------------------------------------------+         +-----------------------------+
 | Browser profile / CLI / add-in pane           |         | Microsoft 365, Google       |
 |  - key store, crypto worker, WASM engine      | <-----> | Workspace: hold plaintext   |
 |  - decrypted state in memory                  |         | document copies             |
 +-----------------------+-----------------------+         +-----------------------------+
                         | TLS (network adversary in path)
 ======================= | ================== SERVICE BOUNDARY (not trusted for A1, A2, A3) ====
 +-----------------------v-----------------------+
 | Web tier (SvelteKit proxy)  - ciphertext only |  <- also delivers client code (A8): trusted
 | API (engine-api)            - ciphertext only |     for availability, NOT for A1 in browsers
 | PostgreSQL / object tables  - ciphertext      |     unless code integrity controls exist
 | Backups, logs, admin console, email provider  |
 | Converter (consent-only path)                 |  <- sees plaintext only by explicit consent
 +-----------------------------------------------+
 +-----------------------------------------------+     +---------------------------------+
 | Customer KMS / HSM (Phase 0, Phase 3)         |     | AI provider (local-agent path)  |
 +-----------------------------------------------+     +---------------------------------+
```

| Boundary | Statement |
|---|---|
| Device | Plaintext and keys live here. A compromise of the device is out of scope (section 8). |
| Service | Everything behind TLS termination. Treated as honest-but-curious for storage and as potentially malicious for integrity (it may reorder, drop, replay, substitute). |
| Client code delivery | The service delivers browser and Office pane code. This is a trust dependency that the sealing design cannot remove. |
| Host editors | Microsoft and Google hold plaintext independently of Dynodoc. |
| KMS | In Phase 0 the KMS is trusted to protect at-rest keys; in Phase 3 it is the customer's policy point. |
| AI provider | Receives plaintext when a local agent reads content, by the user's choice. |

## 4. Adversaries

### 4.1 Catalogue

| ID | Adversary | Capabilities | Goals | Assumptions |
|---|---|---|---|---|
| ADV1 | Curious operator (Verne staff, or a self-hosting customer's DBA) | Read database, object store, backups, logs, admin console, memory of server processes; passive | Read content, learn who works on what | Does not modify served code |
| ADV2 | Database or backup thief | Obtains a database dump, volume snapshot, backup file, or object store copy; offline | Read content, find secrets | No access to member devices |
| ADV3 | Compromised or malicious server | Everything in ADV1 plus: modify or drop data, reorder events, substitute keys and directory entries, serve a modified client, run man-in-the-middle for all clients | Read content, inject forged history, add a rogue reader, deny service | Cannot break primitives; cannot compromise a device out of band |
| ADV4 | Malicious member | Valid account and keys for some projects; may write malformed sealed data, exfiltrate plaintext, collude with ADV3 | Read beyond role, disrupt others, plant content | Role-limited keys |
| ADV5 | Removed member | Retains epoch roots and ciphertext copies from their membership; may still hold valid session tokens briefly | Read new content, keep influence | Server removal and rotation as designed |
| ADV6 | Device thief | Physical access to a locked or unlocked device, or an unwrapped key file | Read content, impersonate user | May know or guess a short passphrase |
| ADV7 | Supply chain attacker | Compromises a dependency (crate, npm, CDN script such as the Office.js library), the build pipeline, a container image, a marketplace listing, or the WASM module | Execute code in clients or servers | Targets weakest build link |
| ADV8 | Network adversary | On-path observer or injector | Read, modify, correlate traffic | TLS in use |
| ADV9 | Compelled disclosure | Legal order to Verne Systems (Dhaka) or a hosting provider for data or assistance | Obtain content, keys, user data | Jurisdictional specifics are for counsel |
| ADV10 | Rogue customer administrator (escrow holder, org admin) | Uses escrow, policy and admin tools beyond intent | Read members' content | Phase 3 only |
| ADV11 | Malicious or compromised local agent or prompt injection | Reads what the delegation allows; may send it to a provider; may propose malicious edits | Exfiltrate, corrupt | Local MCP path only |
| ADV12 | Other tenant | Authenticated user of another organisation on the same service | Cross-tenant reads, existence oracles | Shared infrastructure |

### 4.2 Outcome per adversary and phase

"Reads content" means plaintext of sealed kinds. Phase 0 is customer-managed at-rest
encryption and is not end-to-end; Phase 1 browser means the browser client with live-delivered
code.

| Adversary | Today | Phase 0 | Phase 1 CLI | Phase 1 browser | Phase 2 and 3 |
|---|---|---|---|---|---|
| ADV1 curious operator | Reads all | Reads via running application and database access; blocked only from disks and backups at rest if keys are not on the host | Cannot read content; sees metadata (section 5) | Cannot read if code is not modified | Same, plus escrow admin only by quorum |
| ADV2 DB or backup thief | Reads all | Cannot read if KEK and DEK are not co-located; reads derived tables not covered by Phase 0 tiers | Cannot read | Cannot read | Cannot read |
| ADV3 compromised server | Reads and rewrites all | Reads and rewrites all (application holds keys in use) | Cannot read; can deny, drop, rollback; detected by pins and receipts; cannot forge signed history | **Can read** by serving malicious code unless code integrity controls hold | Add-on (Google) code integrity better; Office panes as browser |
| ADV4 malicious member | Reads per role | Same | Reads what role holds; can write malformed sealed events (isolated by clients) | Same | Same, with scoped delegation |
| ADV5 removed member | Loses access | Same | Keeps epochs held; cannot read new epoch content | Same | Same; re-seal job for stronger cleanup |
| ADV6 device thief | n/a | n/a | Key file passphrase strength decides; OS keystore binds to account | Non-extractable keys usable by thief if profile unlocked | Hardware factors (Phase 3) |
| ADV7 supply chain | Full compromise possible | Same | Release signing and reproducible builds reduce; not eliminate | Largest exposure (live code) | WASM hash pinning, SBOM, signing |
| ADV8 network | TLS protects | TLS | TLS plus sealed payloads (content stays sealed if TLS fails) | Same | Same |
| ADV9 compelled disclosure | All content producible | Content producible with KEK holder's cooperation | Metadata and ciphertext producible; content only from members or escrow | Same, plus code-change compulsion risk | Escrow policy decides |
| ADV10 rogue admin | n/a | KMS admin can use KEK | Cannot read without escrow | Same | Escrow quorum controls |
| ADV11 agent | n/a | n/a | Reads delegated files; provider sees it | Same | Same; cloud connectors cannot read |
| ADV12 other tenant | Per access checks | Same | Cannot read ciphertext usefully; existence oracles remain per existing checks | Same | Same |

## 5. Metadata leakage inventory

Phase 1 hides content, not structure. The table lists what an observer of the service
(ADV1, ADV2, ADV3) learns for an encrypted project, why the service needs it, and the option
that reduces it. "Phase 3 option" refers to sealed index nodes and related controls.

### 5.1 Inventory

| # | Datum | Visible to service | Why it is in the clear | Reduction |
|---:|---|---|---|---|
| 1 | Account identities and emails of members; roles | Yes | Authorization, invitation, audit | None; inherent to a hosted service |
| 2 | Project, space and document identifiers; counts of files per project | Yes | Routing, permissions, quotas | None |
| 3 | Organisation, team, folder names | Yes | Navigation, invitation email | Sealed project name option (protocol 3.3); org names stay |
| 4 | File names, titles, version names, notes, commit messages | No (sealed) | Content class | Done |
| 5 | Number of nodes per file; node types (`paragraph`, `item`, ...) | Yes | Structural validation | Phase 3 sealed index nodes |
| 6 | Parent links and sibling order keys (`pos`) | Yes | Cycle and containment checks | Phase 3 sealed index nodes; note `pos` fractional keys also reveal insertion patterns |
| 7 | Node revision counters, tombstone flags | Yes | Ordering, restore validity | Phase 3 |
| 8 | Event types (`FieldEdited`, `RichTextPatched`, `CommentAdded`, `Deployed`, ...) and target node IDs | Yes | Activity labels, FK, notification routing | Option to seal type for centralized path (breaks activity labels) |
| 9 | Timing of every write, publish, review and merge | Yes | Ordering, receipts | Client batching; see 5.3 |
| 10 | Which account or device authored each commit or event | Yes | Provenance (a product goal) | None |
| 11 | Sealed object sizes after padding bucket | Yes | Storage, quotas, transfer | Larger buckets; chunk padding |
| 12 | Number of new objects per commit and their reuse (which addresses are re-referenced) | Yes | Transfer negotiation | None; reveals which nodes changed |
| 13 | Commit graph (parents, branches, merges) and branch names | Yes | CAS, receipts, reviews | Sealed branch names option (not planned) |
| 14 | Review decisions, reviewer identity, timing | Yes | Approval counting | None |
| 15 | Device keys, encryption keys, fingerprints, enrolment times | Yes | Directory | None |
| 16 | Membership log entries (type, subject, signer, role) | Yes | Server-side authorization checks | None for Phase 1 |
| 17 | Epoch numbers, rotation times, reasons | Yes | Recipient set integrity | None |
| 18 | Existence and timing of escrow, recovery recipients, agent delegations | Yes | Same | None |
| 19 | IP addresses, user agents, TLS fingerprints, session times (web and API logs) | Yes | Operations | Log minimisation; self-hosting; privacy-preserving relays are out of scope |
| 20 | Access patterns: which objects a client fetches when | Yes | Serving | ORAM-style hiding is out of scope |
| 21 | Notification timing and recipients | Yes | Delivery | None |
| 22 | Quota consumption and storage growth | Yes | Billing, limits | None |
| 23 | Erasure and migration events | Yes | Audit | None |
| 24 | Whether two projects hold the same content | No by default | Keyed identifiers differ per project | Dedup mode (crypto 9.3) would reveal within a project only |
| 25 | Cross-project similarity via tree shape and size sequences | Partly | Structure is visible | Phase 3 sealed index nodes; padding |
| 26 | Total sealed size of a project over time | Yes | Quotas | None |
| 27 | Published Merkle or witness leaves (if the transparency design is enabled) | Blinded tags only | Witness design uses keyed tags | See [DESIGN-WITNESSES-AND-TRANSPARENCY.md](DESIGN-WITNESSES-AND-TRANSPARENCY.md) section 7 |

### 5.2 Inference examples

| Observation | What an observer can infer |
|---|---|
| A file has 12 sections of 3 paragraphs each and one table of 40 cells | Document skeleton; with outside knowledge, identify a known template or a person's report |
| Many small `RichTextPatched` events 1 to 3 seconds apart on one node | An active editing session on that paragraph; typing cadence; roughly how much text (by bucket) |
| A burst of node creations followed by a commit | A paste or import |
| Reviewer approves within seconds of publication | Review likely did not involve reading |
| Member removed then epoch opened | Who was removed and when |
| A new project whose tree shape and sizes equal an existing project's | Likely a copy; no content is revealed |

### 5.3 Traffic analysis

| Channel | Observable | Mitigations in the design | Residual |
|---|---|---|---|
| Object sizes | Bucketed 128 B to 64 KiB | Power-of-two buckets, 64 KiB chunks | Size class leaks; an observer learns "short" versus "long" |
| Edit cadence on the centralized path (one event per change) | Count and timing | Client coalescing: debounce autosave to at least 15 seconds and batch events per request; configurable | Fine-grained cadence still visible at the 15 second granularity |
| Commit path | One commit per push | Natural batching | Commit times visible |
| Fetch patterns | Which objects, in which order | None | Linkable to reading behaviour |
| Network layer | Packet sizes, destination, TLS SNI | TLS 1.3; web tier shares an origin with the rest of the site | A network observer can tell that someone uses Dynodoc at a time |
| Dummy traffic or cover writes | n/a | Not planned: cost and complexity exceed value for stated customers | Accepted; state in customer documentation |

Customer-facing statement (proposed): "Dynodoc hides what your documents say. It does not
hide that you use Dynodoc, which projects exist, who is a member, how often and when people
work, or the rough shape and size of documents."

## 6. Attacks specific to this design

### 6.1 Attack catalogue

| ID | Attack | Adversary | Impact | Control | Residual |
|---|---|---|---|---|---|
| AT1 | **Key substitution:** the directory returns an attacker's encryption key for a member when an owner invites them or opens an epoch | ADV3 | Rogue reader | Signed membership log; device fingerprints compared out of band; first-use record and blocking change alert; organisation-pinned directory (Phase 3) | Users who never verify are exposed at first contact; the first fingerprint is trusted |
| AT2 | **Rollback or fork:** serve an old head, or different heads to different clients | ADV3 | Hidden recent changes, split view | Ref receipts with generation and `previous`, client pins; `membership_head` and epoch embedded in each signed commit; witnesses and transparency log as the independent check | Receipt authority not installed in production; isolated clients cannot detect a fork they never observe |
| AT3 | **Malicious client code delivery:** modified web bundle or Office pane exfiltrates keys or plaintext | ADV3, ADV7 | Total loss of confidentiality for users who load it | Section 3.4 of the clients document: CSP, published hashes, signed manifest verified by a loader or extension, CLI for high assurance, self-hosting; Google add-on code is deployed through Google | Largest residual for browsers; not solved by cryptography |
| AT4 | **Downgrade to plaintext:** the service tells a client the project is not encrypted, or serves a plaintext object | ADV3 | Client uploads plaintext | Client pins mode on first sight; mode changes require a signed log entry; storage guard server-side; clients refuse plaintext writes to a pinned-sealed project | A first-contact client has no pin |
| AT5 | **Ciphertext transplant:** swap sealed objects between nodes, documents, projects or epochs | ADV3, ADV4 | Wrong content shown | AAD binding of project, document, node, kind, epoch, revision; key commitment | Within one node and epoch, a stale version can be replayed if the signed state root is not checked; clients check the state root against the signed commit |
| AT6 | **Grant forgery or replay:** craft a grant for an attacker's key, or replay an old grant to a new device | ADV3, ADV4 | Rogue recipient | Grants signed by an authorized member key checked against the log; HPKE `info` binds project, epoch, scope, device; epoch record lists recipients digest | Same as AT1 for the directory |
| AT7 | **Epoch withholding:** the service refuses to publish a new epoch or serves an older one | ADV3 | Removed member keeps reading new content (the service decides whether they can fetch) | Clients derive the expected current epoch from the signed log head; refuse to write under an older epoch; owners see `rotation_pending` | Availability, not secrecy: a removed member cannot read epoch e+1 content they never receive keys for |
| AT8 | **Membership log truncation:** drop the latest `member_remove` | ADV3 | Removed member appears present to some clients | Log head embedded in commits and receipts; clients pin heads; gossip through witnesses | Clients that never saw the newer head |
| AT9 | **Malformed sealed event or object by a member** | ADV4 | Break every client's fold; denial of service | Structural validation at server; clients isolate unreadable or invalid events as error events; review before merge to protected branches | A malicious editor can still make their own contribution unusable |
| AT10 | **Metadata-based inference** | ADV1, ADV3 | Learn structure and behaviour | Section 5 | Accepted for Phase 1 |
| AT11 | **Grant storage exhaustion** | ADV4 | Fill storage or slow clients | Per-project budget table, per-request limits | Within budget |
| AT12 | **Partitioning oracle or key-commitment attack** on AES-GCM | ADV3, ADV4 | Decrypt oracle across keys | Per-object single-use key; key commitment checked before decryption | Implementation errors (omitted commitment) |
| AT13 | **RNG failure or salt reuse** | Bug | Plaintext XOR leak for objects sharing salt | Start-up self-test; one sealing API with no caller nonce; server metric on repeated salts | Silent entropy failure |
| AT14 | **Timing side channel on commitment comparison** | ADV3 | Oracle on key bytes | Constant-time compare in all implementations | Browser JS timing resolution |
| AT15 | **Recovery key theft** | ADV6 | Full access to the user's projects | Offline storage guidance; hardware factor (Phase 3); revoke through another device | User behaviour |
| AT16 | **Escrow abuse** | ADV10 | Silent reading | Quorum, signed log entry, member notice, audit | Collusion of quorum with the service |
| AT17 | **Erasure assurance misunderstood** | User, counsel | False belief that data is gone | Product copy: erasure removes service copies; devices and exports are outside | Residual copies |
| AT18 | **Agent exfiltration or injection** | ADV11 | Content leaves via provider or is edited maliciously | Consent, scoped delegation, no network tools, propose-only, review | Provider retention |
| AT19 | **Library defect** (HPKE, WebCrypto polyfill, JSON parser) | ADV7 | Key or plaintext exposure | Pinned versions, vectors, fuzzing, advisory kill switch, external review | Unknown defects |
| AT20 | **Operator tampering with reproducible-build claims** | ADV7 | False assurance | Publish hashes through an independent channel; third-party rebuilds | Requires someone to check |

### 6.2 Flow-by-flow analysis

Each flow lists what the attacker (usually ADV3, the compromised service, or ADV4) can try and
which check stops it. "Client" means any client with keys.

**F1. Create an encrypted project**

| Step | What happens | Attacker option | Control |
|---|---|---|---|
| 1 | Owner client generates keys, enrols device encryption key | Service returns a different signing key challenge | Existing enrolment proof binds the account and installation; binding signature ties the encryption key to the signing key |
| 2 | Owner generates epoch 1 root, signs genesis log entry and epoch record | Service refuses or alters the record | Records are signed; the owner client verifies what it stored by reading it back |
| 3 | Mode set to `sealed` | Service reports `none` later (AT4) | Client pins the mode and the log head |
| 4 | Recovery key shown | Screen capture, shoulder surfing | Out of scope (section 8); user guidance |

**F2. Write (commit path)**

| Step | What happens | Attacker option | Control |
|---|---|---|---|
| 1 | Client seals changed nodes and builds a signed commit | Malicious extension reads plaintext | Out of scope |
| 2 | Upload objects, stage and publish commit | Service drops or alters objects | Hash addresses and signature; the client verifies on next fetch |
| 3 | Reference advance (CAS) | Service forks histories (AT2) | Receipts and pins; embedded log head |
| 4 | Server records audit | Metadata leak | Section 5 |

**F3. Read and open**

| Step | What happens | Attacker option | Control |
|---|---|---|---|
| 1 | Client fetches commit, verifies signature, signer role from verified log | Service presents a commit signed by a removed member | Client checks the signer's role at the commit's `membership_head` |
| 2 | Fetch objects, verify addresses | Substitute objects | Hash mismatch |
| 3 | Open with epoch key | Transplanted ciphertext (AT5) | AAD and commitment |
| 4 | Render | Malformed plaintext | Parser limits, fuzzed (10.5) |

**F4. Review and approve**

| Step | What happens | Attacker option | Control |
|---|---|---|---|
| 1 | Reviewer client opens candidate and base, computes diff locally | Service shows an older base | Base comes from the signed commit's parent address |
| 2 | Approve | Reviewer approves without reading | Not preventable; same as today |
| 3 | Service counts independent approvals | Service forges approvals | Reviews are server rows; a compromised service can forge them (existing limit); receipts bind the published head |

**F5. Add a member**

| Step | What happens | Attacker option | Control |
|---|---|---|---|
| 1 | Manager looks up member's devices | Directory substitution (AT1) | Fingerprint comparison; first-use record; signed log entry |
| 2 | Grants issued for allowed epochs | Service adds a second recipient on the wire | Recipient digest in the signed epoch record; clients of existing members detect an unsigned recipient |
| 3 | Member opens grants | Replay of old grants | HPKE info binds epoch, scope, device |

**F6. Remove a member**

| Step | What happens | Attacker option | Control |
|---|---|---|---|
| 1 | Manager signs `member_remove` and opens epoch with new grants atomically | Service omits the epoch (AT7) | Clients expect the epoch from the log; owners see `rotation_pending` |
| 2 | Service revokes sessions and device grants | Removed member keeps a valid token for up to one hour | Existing one-hour token limit; membership checks per request |
| 3 | Removed member retains epoch roots | Reads content they already received | Stated limitation; optional re-seal job |

**F7. Recover**

| Step | What happens | Attacker option | Control |
|---|---|---|---|
| 1 | New device enrols | Attacker enrols a device with a stolen password or session | Account security (existing); recovery requires the recovery key or member approval |
| 2 | Recovery key typed | Phishing page asks for the key | Warning copy; the service never needs the key; client accepts it only in the application origin or CLI |
| 3 | Grants opened and re-issued | Escrow abuse (AT16) | Quorum, signed entry, member notice |

**F8. Migrate (M1)**

| Step | What happens | Attacker option | Control |
|---|---|---|---|
| 1 | Owner client reads plaintext through the API | Service tampers with legacy plaintext before sealing | Legacy event chain and snapshot verification run first (`verify`); mismatch report |
| 2 | Seal and publish successor documents | Service keeps a copy of legacy plaintext (it already has it) | Claims policy; legacy erased by the owner path |
| 3 | Complete | Mode flip without a signed entry | Clients require a `migration_complete` entry |

**F9. Local agent**

| Step | What happens | Attacker option | Control |
|---|---|---|---|
| 1 | Delegation grant created for the agent device | Malicious process impersonates the agent | Agent has its own device key; local stdio or socket only |
| 2 | Agent reads nodes | Prompt injection in document text | Read-only default; no network tools; user consent text |
| 3 | Agent proposes edits | Malicious or erroneous edit | Proposal only; independent review; protected main |

## 7. STRIDE per component

Components: C1 client application (browser, CLI, pane), C2 client key store, C3 web tier and
API gateway, C4 API service, C5 database and object tables, C6 backups, C7 converter,
C8 admin console and support tooling, C9 build and release pipeline, C10 KMS or HSM,
C11 local MCP server.

| Component | Spoofing | Tampering | Repudiation | Information disclosure | Denial of service | Elevation of privilege |
|---|---|---|---|---|---|---|
| C1 Client | Fake service or fake peer key (AT1) | Malicious update or extension (AT3) | Local actions unsigned unless committed | Plaintext in memory, clipboard, temp files | Resource exhaustion by large objects | Script in origin uses non-extractable keys |
| C2 Key store | Cloned profile | Overwrite wrapped key | None | Passphrase guessing, profile theft | Eviction deletes keys | Other local account reads file (ACLs, DPAPI scope) |
| C3 Web tier | TLS misissuance | Serve modified bundle | Request logs minimal | Sees ciphertext and metadata; must not render content | Proxy saturation | Session fixation, CSRF (existing controls) |
| C4 API | Stolen device token (1 hour) | Reorder or drop events | Audit rows exist; detail must stay free of sealed text | Metadata (section 5) | Workload admission exists | Role confusion; scope checks per route (existing) |
| C5 Database | n/a | Owner rewrites rows (existing residual) | Immutable triggers can be disabled by owner | Ciphertext and metadata | Fill grants or logs | Runtime role restricted; owner out of scope |
| C6 Backups | Restore of altered dump | Altered dump | n/a | Ciphertext only for sealed data; legacy plaintext until erased | Loss of backups | n/a |
| C7 Converter | n/a | Malicious output (LibreOffice parsing) | None | Sees whole plaintext file when consent path is used | CPU and memory (existing limits) | Document exploit in LibreOffice; isolated container (existing) |
| C8 Admin and support | Impersonation of support | Edit rows by hand | Admin actions audited | Counts only; no content views | n/a | Operator can erase (audited) and cannot read content |
| C9 Build and release | Malicious dependency | Tampered artifacts | Unsigned builds | Secrets in CI | Build failure | Compromised release key |
| C10 KMS | Rogue caller | Key replacement | KMS audit log | Key extraction by admin (BYOK) | KMS outage blocks Phase 0 unwraps | Over-broad IAM |
| C11 Local MCP | Another process connects to stdio or socket | Prompt injection edits | Tool call log local | Agent sends plaintext to provider | Model loops | Tool scope creep; propose only |

## 8. Endpoint compromise: out of scope, stated

If an attacker controls a member's device, browser profile, operating system account, add-in
host or editor, they have the member's keys in use and the member's plaintext. The design
does not and cannot defend this. The following are explicitly **not** protected:

- A device with malware or a malicious browser extension.
- A browser tab running attacker-supplied script in the application origin (the key is
  non-extractable but usable).
- Content already decrypted, displayed, copied, printed, exported, screenshotted or sent to
  an AI provider.
- A host editor's cloud copy (Microsoft 365, Google Workspace).
- Keyloggers, screen capture, shoulder surfing.
- A user who shares their recovery key or passphrase.

Partial mitigations exist (OS keystores, hardware factors, short sessions, revocation,
rotation) and are described elsewhere; none changes the statement that endpoint compromise is
out of scope.

## 9. What is not claimed

| We do not claim | Because |
|---|---|
| "Zero knowledge" | The service knows membership, structure, timing and sizes; and a malicious browser bundle can read content |
| "We cannot read your data" without conditions | True for stored content under the stated assumptions (no malicious code delivery, no endpoint compromise); false for metadata |
| "Military-grade", "unbreakable", "quantum-safe" | Unquantifiable or false; grants are not post-quantum in Phase 1 |
| Protection from Microsoft or Google | They hold the editor copies |
| Forward secrecy | Not a goal (cryptographic design 12.3) |
| That removal revokes past access | Removed members keep what they received |
| Compliance with any regulation or standard | Mapping in section 10.8 is for scoping only |
| That an AI agent's use is private | Content goes to the provider by design |
| That deletion removes all copies | Backups, devices and exports are outside |
| Independent verification | Until section 10 gates G3 to G5 pass |

## 10. Assurance plan

### 10.1 Principles

1. **Evidence before words.** A claim is published only after the gate that supports it.
2. **Independence.** Review by people who did not design or write the code, from outside Verne
   for cryptography.
3. **Reproduce every finding.** Each finding becomes a vector, a test or a fuzz seed.
4. **Small surface.** The assurance cost scales with the number of constructions. The design
   keeps one AEAD, one key-wrap, two signed record types and no new primitives.
5. **Plain language limits.** Every published statement includes what is not covered
   (section 9).

### 10.2 Gates

| Gate | Name | Entry | Exit evidence | Permits |
|---|---|---|---|---|
| G0 | Design freeze | The five E2EE documents reviewed internally; open questions OQ-CR-1 to 4 and OQ-PR-1 to 4 decided | Signed decision record; vectors from an independent producer | Prototype implementation |
| G1 | Specification review | G0 | External desk review of the cryptographic design and protocol; all critical and high findings closed or accepted in writing | Building production code paths |
| G2 | Implementation review | Phase 1 release candidate; fuzzers run 7 days clean; formal model (10.4) results | External code and build review; retest of fixes; summary published | Describing encrypted projects as "end-to-end encrypted" in the stated scope |
| G3 | Controlled pilot | G1 and an internal pass of G2 inputs | Design partners under agreement; private vulnerability programme live; incident response drill | Beta label; no "end-to-end" wording ([decision 015](decisions/015-customers-formula-engine-and-planned-products.md)) |
| G4 | General availability | G2 passed; reproducible builds; SBOM and signing; runbooks; public disclosure policy updated | Release checklist signed | Public availability with the claims of 10.9 |
| G5 | Continuous | Always | Annual re-review and re-review on any cryptographic change; continuous fuzzing; metric dashboards | Maintaining claims |

### 10.3 Independent cryptographic review

| Item | Plan |
|---|---|
| Scope for G1 | Cryptographic design and protocol documents, key hierarchy, sealing construction, grants, epochs, membership log, recovery and escrow, comparison with alternatives, threat model |
| Scope for G2 | Reference implementations (Rust and TypeScript), WASM build, key storage in each client, build and release pipeline, vectors and fuzz results, the Phase 0 envelope encryption and KMS integration |
| Reviewers | Applied-cryptography firms or academics with published end-to-end encryption reviews (messaging, file sync or key management). At least one reviewer for protocol and one for implementation. Selection criteria, conflicts and independence recorded |
| Inputs | Specifications, source access, threat model, test vectors, a design-rationale session with the authors |
| Outputs | Written report with severity ratings, reproduction steps, recommended fixes; retest letter; permission to publish a summary or the full report |
| Effort and lead time | External reviewer effort and fees are not estimated here (unverified; quotes required). Plan for 6 to 10 weeks lead time to schedule and 3 to 6 weeks of review per gate as a planning assumption |
| Internal support | One engineer part time during review (see 12) |
| Disposition | No claim of "end-to-end encrypted" before G2. Unresolved high findings block G4 |

### 10.4 Formal verification options

| Option | Target | Properties | Cost and value | Recommendation |
|---|---|---|---|---|
| ProVerif or Tamarin model of the protocol | Epoch opening, grants, membership log, member removal, recovery, escrow use | Secrecy of epoch roots after removal; agreement on membership between honest members; authentication of grants; no rogue recipient without a signed log entry | 6 to 10 person-weeks including modelling the AAD binding abstractly; high value because composition errors are the usual failure | Do before G2; publish model |
| CryptoVerif or EasyCrypt for the sealing construction | Single-use key derivation, commitment, AAD | IND-CCA style and key-commitment bounds | High expertise cost; the construction is standard (HKDF plus AES-GCM) | Optional, reviewer's discretion |
| TLA+ or Alloy model of the migration and epoch state machines | `none` to `migrating` to `sealed`, abort, declassify, concurrent rotation | No lost updates, no state where plaintext and sealed writes both succeed | 2 to 3 person-weeks | Recommended before implementing migration |
| Rust verification (Kani, Creusot, Prusti) on parsers | Container and header parsers, size checks, ref lists | No panics, no out-of-bounds, bounded allocation | 3 to 5 person-weeks for the sealed header and typed v3 decode | Optional; fuzzing gives most value first |
| Verified cryptographic libraries (libcrux, HACL*) | HPKE and primitives | Library-level correctness | `hpke-rs` offers a libcrux backend for a limited suite ([docs](https://docs.rs/hpke-rs)); suite coverage must be checked | Evaluate at C3 |

### 10.5 Fuzzing and testing

| Target | Tool | Seed corpus | Gate |
|---|---|---|---|
| Sealed container parser and opener | `cargo-fuzz` (libFuzzer) | Vectors and mutated negatives | No crash or hang over 24 hours per release candidate |
| Canonical JSON plaintext parser (nesting, duplicate keys, UTF-8) | `cargo-fuzz`, Jazzer.js | Existing `shared_checkpoint` and typed fixtures | Same |
| Grant, epoch record and membership log verifiers | `cargo-fuzz` structure-aware | Valid and forked logs | Same |
| Commit v4 and typed v3 decoders | `cargo-fuzz` | Existing commit and typed fixtures | Same |
| TypeScript crypto package | Jazzer.js, fast-check | Vectors | Same |
| Differential: Rust versus TypeScript versus WASM | Property tests over random operation sequences | Parity corpus | 100 percent agreement |
| Known-answer and invalid-input vectors for primitives | Project Wycheproof where applicable (AES-GCM, HKDF, HMAC, Ed25519, X25519) | Published vectors | 100 percent |
| Mutation testing of crypto modules | `cargo-mutants` | n/a | Surviving mutants triaged |
| Storage scan for plaintext classes | CI job over an encrypted test project | n/a | Zero hits |
| Adversarial server harness | Test harness server that reorders, drops, replays, substitutes | n/a | All attacks detected |

Existing basis: property tests with `proptest` in `engine-core` (hash-chain tamper and
reorder, 1,000 cases each, `log.rs`). No fuzzing exists today. Continuous fuzzing through a
public service is possible for the open-source engine repository (eligibility not verified).

### 10.6 Disclosure and bug bounty

| Element | Current | Proposed |
|---|---|---|
| Disclosure channel | `security.txt` with email and GitHub private reporting for the engine repository; expires 2027-10-11 | Keep; add a PGP key or a dedicated address; document response times |
| Bounty | None ("No bug bounty is offered") | Invite-only programme at G3 through a platform or direct agreements; public programme after one stable release |
| Scope | n/a | Encrypted-project confidentiality and integrity bugs, key handling, client code delivery, WASM module, release signing |
| Safe harbour | None published | Counsel-reviewed text; no action against good-faith research within scope |
| Rewards | n/a | To be set by the owner; severity bands tied to the sealing, key and membership surfaces |
| Triage | n/a | Named owner, acknowledgement in 3 business days, status updates every 14 days (targets) |

### 10.7 Transparency and supply chain

| Measure | Specification | Status |
|---|---|---|
| Public design documents | This set, kept current, with a change log for cryptographic changes | Planned |
| Reproducible builds | CLI, WASM module and web bundle built in pinned containers; hashes published on a second channel | MISSING |
| Signed releases | Release artifacts signed with an offline key; verification instructions; key rotation procedure | MISSING |
| SBOM | CycloneDX or SPDX per release (Rust and npm) | MISSING |
| Dependency policy | `cargo-deny` and `cargo-audit`, `npm audit` in CI; exact pins and hashes for HPKE and curve libraries; review of any new cryptographic dependency | MISSING |
| Key and log transparency | Membership log heads as leaves in the witnessed log of [DESIGN-WITNESSES-AND-TRANSPARENCY.md](DESIGN-WITNESSES-AND-TRANSPARENCY.md), using its blinded tags so witnesses learn no project identifiers | Design exists (concurrent), not built |
| Client integrity | Signed bundle manifest verified by a loader or extension (clients document 3.4) | Option |
| Warrant canary | Counsel's decision; not recommended without advice | Open |

### 10.8 Certification mapping (scoping guidance, not legal advice)

| Framework or control | What encryption contributes | Still required |
|---|---|---|
| SOC 2 Trust Services Criteria (confidentiality C1.1, logical access CC6.1, transmission and disposal CC6.7) | Evidence for encryption of data in storage and for key management practices | Audit by a licensed firm; policies, access reviews, incident response, vendor management |
| ISO/IEC 27001:2022 Annex A (8.24 use of cryptography, 5.23 cloud services, 8.10 information deletion) | A documented cryptographic control and key lifecycle | Management system, risk assessment, certification audit |
| ISO/IEC 27017 and 27018 | Cloud and PII protection controls | Same |
| NIST SP 800-53 (SC-12, SC-13, SC-28) and SP 800-57 (key management) | Control implementation evidence | Selection of approved algorithms for the target profile; key management plan |
| FIPS 140-3 | Not met: `ring` is not a validated module. A validated build option (for example through `aws-lc-rs`) would be needed for US federal profiles (unverified, check at build time) | Validated module integration and testing |
| GDPR Art. 32(1)(a) encryption as an appropriate measure; Art. 34(3)(a) no notification to data subjects where data is unintelligible; Art. 17 erasure | Reduces exposure in a breach of the service; erasure semantics in 6 | Records of processing, DPIA, processor agreements; erasure caveats (devices, exports) |
| HIPAA Security Rule 164.312(a)(2)(iv) and breach safe harbour for unreadable data | Encryption may qualify content as "secured" if keys are not compromised | BAA, risk analysis, policies; metadata is not covered |
| CSA STAR, Common Criteria | Not planned | Out of scope |
| Bangladesh data protection requirements | Not assessed | Counsel (unverified) |

### 10.9 Claims ladder (public wording by evidence level)

| Level | Evidence required | Permitted statement (draft) |
|---|---|---|
| 0 (today) | None | "Not end-to-end encrypted. The service can read content. No encryption-at-rest configuration is claimed." |
| 1 | Phase 0 shipped and tested; backup restore drill passed | "Customer-managed encryption at rest: storage volumes, backups and sensitive columns are encrypted with keys you control. The running service can still read content." |
| 2 | Phase 1 beta at G3 | "Encrypted projects (beta): content is encrypted on your devices before upload. Not yet independently reviewed. The service still sees membership, structure and timing." |
| 3 | G2 passed; G4 | "End-to-end encrypted projects: the service stores ciphertext and cannot read content, under these conditions (list). It sees metadata (list). It does not protect against compromised devices or, in the browser, against a malicious application delivery." |
| Never | n/a | "Zero knowledge", "unhackable", "military grade", "quantum-proof" |

Any copy change to `/security`, `/privacy`, the FAQ and `llms.txt` follows this ladder and the
legal-pages review in [LEGAL-PAGES.md](LEGAL-PAGES.md).

### 10.10 Evidence package for customers

A security whitepaper (design summary and limits), the threat model, the review summary and
retest letter, SBOM and signed release hashes, vector files, a recovery and escrow guide, and
a statement of what is and is not protected for each client.

## 11. Residual risks accepted at Phase 1

1. Metadata visibility (section 5).
2. Browser code delivery (AT3).
3. Grants are not post-quantum (crypto 12.4).
4. Removed members keep prior content (crypto 6.6).
5. No forward secrecy (crypto 12.3).
6. Legacy plaintext after migration until erased.
7. Host editors hold plaintext.
8. Agent use sends plaintext to providers.
9. Availability depends on the service and, for recovery, on user hygiene.
10. Unaudited HPKE libraries until review.

## 12. Delivery plan (assurance workstream)

| # | Task | Output | Person-weeks (Verne) | Depends on |
|---|---|---|---:|---|
| A1 | Internal review of the five documents; decisions on open questions; G0 record | Decision record | 3 | None |
| A2 | Formal protocol model (ProVerif or Tamarin) and migration state machine model | Models, reports | 10 | G0 |
| A3 | Fuzzing and Wycheproof infrastructure, CI jobs, corpus management | CI, dashboards | 5 | Crypto C2 to C5 |
| A4 | Dependency policy, SBOM, signing, reproducible build pipeline for CLI, WASM, web | Pipeline and docs | 8 | None |
| A5 | Vulnerability programme: policy, safe harbour, triage process, private programme set-up | Policy, runbook | 3 | G3 |
| A6 | External review coordination, remediation and retest (G1 and G2), engineer time | Fix PRs, vectors | 10 | G0, G2 |
| A7 | Whitepaper, customer evidence package, claims review with counsel | Documents | 4 | G2 |
| A8 | Incident response drill (key leak, library defect, malicious bundle) | Drill report | 2 | Phase 1 RC |
| | **Total (Verne effort)** | | **45** | |

Assumptions: one security engineer full time during gates, two feature engineers at 20
percent during reviews. Confidence: medium (range 35 to 60 person-weeks). Excludes: external
reviewer fees and the cost of certification audits, legal fees, hardware procurement,
bug bounty rewards, the effort of building the features under review (other documents).

## 13. Security requirements and traceability

Requirements derived from the attacks and flows. Each is testable; the last column names the
evidence that closes it.

| ID | Requirement | Source | Evidence |
|---|---|---|---|
| SR-1 | No plaintext of a sealed class is written to any table, object, log or temporary file by the service for an encrypted project | A1, A2, ADV1 to ADV3 | Storage scan in CI; code review checklist |
| SR-2 | Every sealed object is bound by AAD to project, document, node, kind, epoch, revision, suite and child references | AT5 | Negative vectors per field |
| SR-3 | Each AEAD key and nonce is used for exactly one message; no API accepts a caller-supplied nonce | AT13 | API review; salt-reuse metric; fuzzing |
| SR-4 | Key commitment is verified before decryption in every implementation | AT12 | Negative vectors; cross-implementation matrix |
| SR-5 | Unknown versions, suites, kinds and flags fail closed | AT19 | Parser fuzzing; vectors |
| SR-6 | Epoch records form a verified chain whose signers were authorized by the signed membership log | AT1, AT6, AT7 | Log and epoch vectors; formal model |
| SR-7 | A member removal and the next epoch are accepted atomically | AT7 | Integration test; adversarial server |
| SR-8 | Clients pin project mode and log head and treat regressions as alerts | AT2, AT4, AT8 | Client tests with a malicious server harness |
| SR-9 | A fingerprint change for a member blocks grant issuance until the user decides | AT1 | UI test |
| SR-10 | The server rejects plaintext content for sealed projects and sealed content for plaintext projects | AT4 | Guard trigger tests |
| SR-11 | Grants carry HPKE info bound to project, epoch, scope and device, and are signed by an authorized member | AT6 | Grant vectors |
| SR-12 | Private keys are never sent to the service, never logged, and held non-extractable where the platform allows | A3 | Code review; network capture test; static analysis |
| SR-13 | The recovery key is never transmitted and is accepted only in the application origin or CLI | AT15 | UI and network tests |
| SR-14 | Constant-time comparison for commitments and MACs | AT14 | Code review; timing harness where feasible |
| SR-15 | Agent delegations are scoped to files, time-limited, logged in the signed log, and cannot approve | AT18 | Policy tests |
| SR-16 | Web, WASM and CLI artifacts are built from pinned inputs, hashed and signed | AT3, AT20 | Release checklist |
| SR-17 | Cryptographic dependencies are pinned by version and hash and reviewed on change | AT19 | CI policy |
| SR-18 | Malformed sealed events from a member cannot prevent other members from reading the rest of the history | AT9 | Fold isolation tests |
| SR-19 | Quotas bound grants, log entries and epochs per project | AT11 | Budget tests |
| SR-20 | Audit rows for encrypted projects contain identifiers and classes only, never sealed-class text | A10 | Audit scan |
| SR-21 | Backup and restore preserve ciphertext, grants and logs and do not require keys | A9 | Restore drill |
| SR-22 | Declassification and plaintext-copy actions require explicit confirmation and are logged | F8, protocol section 10 | UI and log tests |
| SR-23 | The client self-test refuses to seal if randomness or primitives fail | AT13 | Fault injection |
| SR-24 | No claim is published above the evidence of the claims ladder | TM-G2 | Release checklist |

## 14. Abuse, safety and legal handling

An encrypted project removes the operator's ability to inspect content. That has consequences
the business must decide on before offering it.

| Issue | Today | With encrypted projects | Needed |
|---|---|---|---|
| Illegal or abusive content | The operator can read and act | The operator cannot see content; it can still suspend accounts, disable a project and delete ciphertext, based on member reports or legal orders | Terms of service wording; abuse-report path where a member voluntarily discloses; counsel on obligations in the operator's jurisdiction |
| Legal compulsion (ADV9) | Content producible | Metadata and ciphertext producible; content only through a member or escrow holder | Law-enforcement request policy; transparency reporting; no technical backdoor commitment (counsel to confirm wording) |
| Copyright and takedown | Operator can locate content | Cannot locate by content; can act on project identifiers | Process on identifiers and member cooperation |
| Customer data-loss prevention and e-discovery | Server-side tools possible | Not possible server-side | Escrow with documented purpose, or client-side DLP integrations (not designed here) |
| Malware in uploads | Server could scan | Cannot scan sealed bytes | Client-side checks; host editor scanning; policy statement |
| Insider abuse within the customer | Admin views via admin tools | Admin cannot view; escrow admin can, with quorum and trace | Policy and training |
| Use of encryption to hide activity | Visible | Hidden content, visible metadata and volume | Rate limits and account verification remain |

None of these is decided here. They are listed because each affects whether and how Phase 1
is offered on the hosted service operated from Dhaka (operator facts in
[LEGAL-PAGES.md](LEGAL-PAGES.md)).

## 15. Privacy analysis

| Topic | Analysis |
|---|---|
| Roles | For encrypted projects the operator holds metadata personal data (emails, display names, membership, device keys, timing, IP addresses). Content personal data in sealed objects is not accessible to it. Customer organisations remain controllers of content |
| Data minimisation | Sealed names and the optional sealed project name reduce what the operator holds; logs retain IP addresses and user agents unless minimised |
| Access and portability requests | The operator can export metadata and ciphertext; content must come from the member or controller. Export tooling must say so |
| Erasure | Existing audited erasure removes service copies and records receipts; devices and exports persist. Pseudonymous public key bindings remain with historical authorship by design ([DEVICE-SIGNING-KEYS.md](DEVICE-SIGNING-KEYS.md)) |
| Retention | Backups hold ciphertext for their retention period (14 days today, same host); crypto-erasure of grants shortens effective retention for covered epochs |
| DPIA | Update to cover new key-handling processing, metadata analysis, escrow and agent delegation to third-party providers |
| Cross-border | Ciphertext location matters less; metadata location still matters; the self-hosted edition can keep both local |
| Special categories | Encryption helps keep special-category content from the operator; it does not replace controller obligations |
| Transparency to members | Escrow, agent delegations and legacy plaintext are visible in the project |

## 16. Detection and response

### 16.1 Signals

| Signal | Source | Meaning |
|---|---|---|
| Commitment mismatch rate above baseline | Client reports (opt-in, counts only) and server validation of headers | Tampering, a bad client build or storage corruption |
| Membership log or epoch chain verification failure | Client report | Fork, tampering or bug; highest priority |
| Fingerprint change events | Directory table | Possible key substitution or legitimate re-enrolment |
| Epoch opened without a preceding membership change, or by an unusual member | Server audit | Compromised manager key or misuse |
| Grants issued to recipient kinds `escrow` or `agent` | Server audit | Policy review |
| Plaintext write attempts to sealed projects | Guard trigger counters | Client bug or downgrade attempt |
| Burst of migrations or declassifications | Server audit | Possible account takeover |
| Salt prefix collisions | Header index | RNG fault |

### 16.2 Incident classes

| Class | Example | First actions |
|---|---|---|
| IC1 Key compromise (member) | Device or recovery key stolen | Revoke device; rotate epochs; review log; notify members |
| IC2 Service compromise | Attacker controls API or database | Assume ciphertext and metadata exposed; verify heads against pins and witnesses; rotate service secrets; customer notification; for browser clients assume code-delivery risk until the bundle is verified |
| IC3 Malicious client bundle | Modified web assets served | Pull bundle; publish hash incident record; force client refresh; advise rotation for sessions that loaded it |
| IC4 Library vulnerability | HPKE or curve library defect | Advisory kill switch for flagged builds; patched client; rotate grants if required |
| IC5 Metadata exposure | Logs or database dump leaked | Notify per obligations; ciphertext safe; assess metadata sensitivity |
| IC6 Availability | Service down or data loss | Restore from encrypted backup; clients keep local copies; verify heads after restore |

Runbooks, drill schedule and communication templates are part of task A8 in section 12.

## 17. Risks to the assurance plan

| # | Risk | Mitigation |
|---|---|---|
| AR1 | Review finds a design flaw late | G1 before implementation; formal model; iterate on vectors |
| AR2 | Reviewer availability delays G2 | Book at G0; keep a second candidate |
| AR3 | Commercial pressure to claim "end-to-end" early | Decision 015; claims ladder; owner sign-off |
| AR4 | Reproducible builds not achievable for WASM or web bundle | Pin toolchain and containers; accept signed builds with published hashes as the fallback |
| AR5 | Findings accumulate without reproduction tests | Rule: every finding gets a test or vector before closing |
| AR6 | Bug bounty noise overwhelms a small team | Invite-only first; strict scope; triage rota |

## 18. Open questions

| # | Question | Recommended default | Decider |
|---|---|---|---|
| OQ-TM-1 | Budget and firm selection for G1 and G2 | Two firms (protocol, implementation); quotes before G0 | Owner |
| OQ-TM-2 | Publish the full review report or a summary? | Publish the report with findings resolved | Owner with counsel |
| OQ-TM-3 | Run a public bug bounty at G4? | Invite-only at G3, public after one stable release | Owner |
| OQ-TM-4 | Which transparency log design to adopt for membership heads? | Use the witnessed log design if built; otherwise embed heads in receipts only | Engineering lead |
| OQ-TM-5 | FIPS 140-3 target for regulated customers? | Not in Phase 1; evaluate `aws-lc-rs` FIPS build at Phase 3 | Product owner |
| OQ-TM-6 | Acceptable wording for browser client until code-integrity controls exist | State the code-delivery limitation; recommend CLI for high assurance | Owner with counsel |
| OQ-TM-7 | Customer-facing metadata statement approval | Use the sentence in 5.3 | Owner with counsel |
| OQ-TM-8 | Warrant canary | Do not publish without counsel | Owner with counsel |

## Appendix A: glossary

| Term | Meaning |
|---|---|
| ADV | Adversary identifier in section 4 |
| AT | Attack identifier in section 6 |
| Gate | Assurance milestone with exit evidence (section 10.2) |
| Honest-but-curious | Follows the protocol but inspects what it can see |
| Residual risk | Risk that remains after controls, stated and accepted |
| STRIDE | Spoofing, Tampering, Repudiation, Information disclosure, Denial of service, Elevation of privilege |
| Witness | Independent party that cosigns log checkpoints after a consistency check |

## Appendix B: references

Retrieved 14 October 2026 unless stated. Items marked "from general knowledge" were not
re-fetched and must be verified at build time.

| Reference | URL | Note |
|---|---|---|
| RFC 9116 security.txt | https://www.rfc-editor.org/rfc/rfc9116 | From general knowledge |
| Project Wycheproof | https://github.com/C2SP/wycheproof | From general knowledge; the hpke-js README states it passes Wycheproof ECDH and X25519 vectors |
| hpke-js README (audit status) | https://github.com/dajiaji/hpke-js | Fetched |
| rust-hpke docs (audit status) | https://docs.rs/crate/hpke/latest | Search summary |
| hpke-rs (libcrux backend) | https://docs.rs/hpke-rs | Search summary |
| ProVerif; Tamarin; CryptoVerif; TLA+ | https://bblanche.gitlabpages.inria.fr/proverif/ ; https://tamarin-prover.com/ ; https://cryptoverif.inria.fr/ ; https://lamport.azurewebsites.net/tla/tla.html | From general knowledge |
| Kani; Creusot; Prusti | https://github.com/model-checking/kani ; https://github.com/creusot-rs/creusot ; https://www.pm.inf.ethz.ch/research/prusti.html | From general knowledge |
| cargo-fuzz; cargo-mutants; cargo-deny; cargo-audit | https://github.com/rust-fuzz/cargo-fuzz ; https://github.com/sourcefrog/cargo-mutants ; https://github.com/EmbarkStudios/cargo-deny ; https://github.com/rustsec/rustsec | From general knowledge |
| GDPR Articles 17, 32, 34 | https://eur-lex.europa.eu/eli/reg/2016/679/oj | From general knowledge; not legal advice |
| HIPAA Security Rule 45 CFR 164.312 | https://www.ecfr.gov/current/title-45/subtitle-A/subchapter-C/part-164/subpart-C | From general knowledge; not legal advice |
| Witness and transparency design (concurrent draft) | [DESIGN-WITNESSES-AND-TRANSPARENCY.md](DESIGN-WITNESSES-AND-TRANSPARENCY.md) | Not authoritative |
| Cryptography references and dates | [cryptographic design, appendix C](E2EE-CRYPTOGRAPHIC-DESIGN.md#appendix-c-references) | |
