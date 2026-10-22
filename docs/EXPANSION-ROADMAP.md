# Expansion roadmap: self-hosting, AI-agent connectors, end-to-end encryption

Status: design, not implemented. Date: 14 October 2026. Owner: Verne Ltd.

This page is the index and the sequencing view over the planning documents written on
14 October 2026. Nothing described in them is built unless it says so. The decisions that
govern them are in [decision 015](decisions/015-customers-formula-engine-and-planned-products.md)
(customers, formula engine, planned products, copyright line) and
[decision 013](decisions/013-private-application-licence.md) (private, proprietary workspace;
MIT engine).

## 1. Decisions in force

| Topic | Decision |
|---|---|
| Customers | Teams of any size, enterprise and academia, working on multiple documents. Three planning profiles: A small team or lab on one node, B department/university with SSO, C enterprise or regulated, HA, intranet-only or air-gapped. |
| First connector | Claude first; ChatGPT next; other clients after. Agent writes are review-gated, attributed and hash-chained. |
| Encryption | Customer-managed at-rest encryption (Phase 0) is not end-to-end encryption and is planned first; encrypted projects (Phase 1) only after demand evidence and external review. No "zero knowledge" claim before that. |
| Formula engine | `hyperformula` was replaced on 14 October 2026 by an in-house, dependency-free engine. See [FORMULA-ENGINE-REPLACEMENT.md](FORMULA-ENGINE-REPLACEMENT.md). |
| Licence | Workspace and website are proprietary and private; engine, API and CLI are MIT. Copyright line: Verne Ltd. |
| Receipt authority | Not installed in production; rotation support is built so enabling it is recoverable. See [RECEIPT-AUTHORITY-DECISION.md](RECEIPT-AUTHORITY-DECISION.md) and [REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md). |
| Marketplace | Publication parked. |

## 2. Document map

### Self-hosted edition (about 143 person-weeks, plus or minus 40 percent)
| Document | Lines | Contents |
|---|---|---|
| [SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md) | 1,834 | Master design: profiles, current-state tables, external-dependency inventory, multi-instance specification, add-in distribution, key custody, sizing, residency, STRIDE, rehearsal tooling, phases, validation plan, cost to serve |
| [SELF-HOSTED-DEPLOYMENT-GUIDE.md](SELF-HOSTED-DEPLOYMENT-GUIDE.md) | 1,265 | Reference topologies, sizing, network and TLS, configuration reference, install, backup, upgrade, rollback, disaster recovery runbooks |
| [SELF-HOSTED-IDENTITY-AND-ACCESS.md](SELF-HOSTED-IDENTITY-AND-ACCESS.md) | 1,094 | OIDC, SAML 2.0, SCIM 2.0, optional LDAP, SMTP, group-to-role mapping, service accounts, token model, academic federation, offboarding |
| [SELF-HOSTED-LICENSING-AND-DISTRIBUTION.md](SELF-HOSTED-LICENSING-AND-DISTRIBUTION.md) | 926 | Editions, signed images, offline bundles, Helm chart, offline licence files, metering without phoning home, EULA checklist, support model |

### AI-agent connectors (about 45 to 57 person-weeks)
| Document | Lines | Contents |
|---|---|---|
| [AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md) | 870 | Master design: identity, grants, OAuth, policy, review-gated writes, attribution layers, phases 0 to 4, risks |
| [AI-AGENT-PROTOCOL-SPEC.md](AI-AGENT-PROTOCOL-SPEC.md) | 1,539 | Compact node text format, eight tools with JSON Schemas, operation grammar, concurrency, errors, endpoints, transcripts |
| [AI-AGENT-TOKEN-EFFICIENCY.md](AI-AGENT-TOKEN-EFFICIENCY.md) | 706 | Strategies, harness and benchmark design, expected savings with method and confidence |
| [AI-AGENT-SECURITY.md](AI-AGENT-SECURITY.md) | 712 | Threat model (prompt injection, exfiltration, confused deputy, excessive agency), controls, privacy notes |
| [AI-AGENT-VENDOR-INTEGRATIONS.md](AI-AGENT-VENDOR-INTEGRATIONS.md) | 831 | Per-client integration: Claude, ChatGPT, Gemini, Copilot, IDE agents, generic REST; verified and unverified facts dated 14 October 2026 |

### End-to-end encryption (about 289 person-weeks, range 220 to 420, excluding external fees)
| Document | Lines | Contents |
|---|---|---|
| [END-TO-END-ENCRYPTION.md](END-TO-END-ENCRYPTION.md) | 716 | Master design, claims ladder, Phase 0 (at-rest) to Phase 3, go/no-go and demand evidence |
| [E2EE-CRYPTOGRAPHIC-DESIGN.md](E2EE-CRYPTOGRAPHIC-DESIGN.md) | 1,219 | Key hierarchy, HPKE grants, epochs, sealing, signatures over ciphertext, recovery and escrow, comparisons, vectors |
| [E2EE-PROTOCOL-AND-MIGRATION.md](E2EE-PROTOCOL-AND-MIGRATION.md) | 1,027 | Field classification, typed v3, commit v4, API and SQL, worked edit, migration, 34-row feature matrix |
| [E2EE-CLIENTS-AND-KEY-MANAGEMENT.md](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md) | 849 | Web, WASM assessment, Office, Apps Script, CLI, local MCP and consent, enrolment, member flows, escrow |
| [E2EE-THREAT-MODEL-AND-ASSURANCE.md](E2EE-THREAT-MODEL-AND-ASSURANCE.md) | 710 | STRIDE, adversaries, metadata inventory, assurance gates G0 to G5 |

### Remaining engine items (about 150 to 205 person-weeks in total)
| Document | Lines | Contents |
|---|---|---|
| [ENGINE-REMAINING-DESIGNS.md](ENGINE-REMAINING-DESIGNS.md) | 259 | Index, status, dependency graph, waves, cross-cutting formats and keys |
| [DESIGN-COMMIT-PRUNING.md](DESIGN-COMMIT-PRUNING.md) | 941 | Signed shallow-history boundaries (25 to 33 pw) |
| [DESIGN-CROSS-DOCUMENT-MERGE.md](DESIGN-CROSS-DOCUMENT-MERGE.md) | 828 | Moves between documents, renames, constraints (26 to 34 pw) |
| [DESIGN-NODE-CONFLICT-RESOLUTION.md](DESIGN-NODE-CONFLICT-RESOLUTION.md) | 729 | Automatic resolution policies for real conflicts (27 to 37 pw) |
| [DESIGN-WITNESSES-AND-TRANSPARENCY.md](DESIGN-WITNESSES-AND-TRANSPARENCY.md) | 734 | Independent witnesses and transparency log (19.5 to 26 pw) |
| [DESIGN-LARGE-FILE-ACCEPTANCE.md](DESIGN-LARGE-FILE-ACCEPTANCE.md) | 713 | Workloads, chunking, ingest and egress, pass/fail (37 to 52 pw) |
| [DESIGN-INSTALLED-HOST-ACCEPTANCE.md](DESIGN-INSTALLED-HOST-ACCEPTANCE.md) | 807 | Word, Excel, PowerPoint and Google editors run plan (owner 14 to 25 days, engineering 9 to 13 pw) |

## 3. Findings that cut across the documents

1. **Receipt authority is the common gate.** Shared pruning horizons, witness cosigning and
   receipt leaves all need an installed, rotatable authority. Rotation is now built
   (REF-AUTHORITY-ROTATION.md); installing the authority is still a custody decision.
2. **Agent authorship needs an event type.** Sealed attribution (layer C) is a new event type and
   so a compatibility decision; layers A and B ship without it. The agent and E2EE designs agree
   that encrypted projects only work with local agents that hold the keys.
3. **Object storage for original files.** Self-hosted profile C and large-file acceptance both
   break if originals stay in PostgreSQL `bytea`. One decision serves both.
4. **Determinism of merge output.** `richtext.rs` uses a 20 ms wall-clock diff deadline, so a
   merge result can depend on machine speed; this affects verification of v3 merges, a WASM
   engine and E2EE client-side merges. It must be made deterministic before any of those.
5. **Immutability triggers shape migrations.** In-place re-encryption and sealing are blocked by
   the append-only protections on purpose, so migration to encrypted projects uses successor
   documents.
6. **Multi-instance gaps.** The SSE registry and admission limits are per process; web refresh
   tokens race across replicas; rollback to N-1 needs relaxing the exact-match schema checks.
7. **Rename plus edit loses the edit.** Project comparison aligns by path, so a rename on one side
   and an edit on the other becomes a delete/edit conflict whose only resolution discards the
   edit. First task of the cross-document merge work.
8. **Changed-node publication gates large files.** Full-state publication extrapolates to about
   30 seconds and 17 times the memory at roughly 68 MB of state.
9. **Token savings are smaller than payload ratios.** Tool definitions dominate short tasks, so
   the realistic input saving is a few times, not tens of times, and output savings are large
   only for bulk edits. Marketing waits for the measured benchmark.
10. **Crypto library gaps.** `ring` has no static X25519, so HPKE needs another reviewed crate;
    WebCrypto X25519 support must be confirmed per browser at build time.

## 4. Suggested order

The documents give dependency detail; this is the product order.

| Order | Work | Why now | Gate |
|---|---|---|---|
| 1 | Agent connectors Phase 0 and 1 (read-only stdio MCP, then draft edits) | Smallest build, strongest differentiation, reuses device sessions and review | Benchmark harness exists before any savings claim |
| 2 | Self-host Phase 0 and 1 (hardened single-node bundle, OIDC/SMTP, manifest generator) | Opens the enterprise and academic profiles | First-customer validation plan in SELF-HOSTED-EDITION.md |
| 3 | Phase 0 at-rest encryption inside the self-host product | Answers most compliance asks without protocol change | Key custody decision |
| 4 | Self-host Phase 2 (HA), agent connectors Phase 2 (remote OAuth), ChatGPT | Scale and reach | Shared pub/sub, object storage decision |
| 5 | Determinism fix, WASM engine, then encrypted projects | Enables client-side merge, browser branch authoring and E2EE | Demand evidence and external review (G1, G2) |
| 6 | Engine items: node conflict resolution, then cross-document merge; pruning and witnesses once an authority is installed | Depth for large multi-document teams | Receipt authority installed |

## 5. Owner decisions queue (from the documents' open questions)

| Question | Source | Recommended default |
|---|---|---|
| Edition names and feature gates | SELF-HOSTED-EDITION Q1 | Community engine (MIT), Team, Enterprise |
| Where enterprise identity code lives (MIT API vs proprietary web tier) | Q2 | Proprietary web tier first; extract only what is generic |
| Ship a local copy of Office.js (licence unclear) | Q7 | Do not ship until licence reviewed; use the CDN where reachable |
| Object storage for originals | Q16 | S3-compatible with a local filesystem driver |
| Licensor entity (Verne Ltd. copyright, Verne Systems operator) | Q20 | Contract with the entity that holds the copyright |
| Seal agent authorship (new event type) or layers A and B only | AI-AGENT-CONNECTORS | Ship A and B, specify C |
| Agent identity representation (synthetic email vs nullable email) | AI-AGENT-CONNECTORS | Nullable email with an explicit actor kind |
| Organisation/tenant object for org-level policy and kill switch | AI-AGENT-CONNECTORS | Introduce a minimal organisation record with self-host |
| Reviewer login path for connector directories | AI-AGENT-VENDOR-INTEGRATIONS | Dedicated demo tenant with password-less test credentials |
| Adopt Phase 0 independently of Phase 1 | END-TO-END-ENCRYPTION OQ-1 | Yes |
| Budget and firms for external crypto review | E2EE-THREAT-MODEL OQ-TM-1 | Required before using the words end-to-end |
| Shared pruning horizons before an object collector exists | DESIGN-COMMIT-PRUNING | Local first; shared after the authority is installed |
| Log key custody, public or private transparency log | DESIGN-WITNESSES | Private log with one external witness first |
| iPad: test or exclude | DESIGN-INSTALLED-HOST-ACCEPTANCE | Exclude until PowerPoint API 1.4 exists there |

## 6. Build status

On 14 October 2026 receipt-authority rotation, passphrase-encrypted key files with optional OS key
storage, and transparent compression of local repository objects were built the same day as the
designs. Between 18 and 22 October 2026 the pieces below were built **in source**. Nothing here is
deployed: production runs the schema 41 release, and schemas 42 to 45
need the usual rehearsal and release. Each row links to its ledger entry in
[PROJECT-STATUS.md](PROJECT-STATUS.md), which says exactly what exists, what was verified and what was not.

| Roadmap item | State | What exists | What does not |
|---|---|---|---|
| Agent connectors, read path | Built, off by default | `engine-agent` library, read routes under `AGENT_GATEWAY=on`, migration 0042, `AgentAuth`, `engine-mcp` over stdio, Claude Code plugin files, setup guide | Never run with a real MCP client or a production copy |
| Agent connectors, policy | Built | Migration 0043: default-deny project policy, role floor, budgets, file overrides, kill switch, usage; independent review applied | Policy for write fields; file override editing screen |
| Agent connectors, screens | Built | Agent keys on Signing keys; "AI agents" in project settings (policy, usage, kill switch) | Run only with the gateway off; no copy review by a non-technical admin |
| Agent connectors, propose path | Built, off by default | `engine-agent` operation compiler, migration 0044 (propose, submit, withdraw, agent drafts, receipts), opt-in MCP write tools; migration 0045: approvals bound to the revision seen, an AI-proposals inbox, badges and review screens, adopt and discard | Release of schemas 42 to 45 (not rehearsed), OAuth, layer C, evals, filters by client or principal, MCP `status` using the new list route |
| Agent connectors, OAuth and remote MCP, ChatGPT | Designed only | none | everything |
| Self-hosted, Phase 0 | Partly built | API retention job, `*_FILE` secrets and startup validation (API and web tier), `DYNODOC_MODE` with the hosted assumptions behind validated configuration, env-file generator and checker, hosted-string audit, a multi-architecture container image | SMTP, bundle, `dynodoctl`, an edition image without the marketing routes (criterion 6 unmet: 47 files still name the hosted domain), add-in manifests, air-gap harness |
| Self-hosted, Phase 1 (identity) | Designed only | none | OIDC, SAML, SCIM, group mapping |
| End-to-end encryption | Phase 0 core only | `DYNOENC` at-rest envelope, key hierarchy, file key provider, vectors (not end-to-end; claims level 1) | Storage wrapper, KMS providers, backfill; all of Phases 1 to 3 |
| Commit pruning | P0 and P1 | Horizon record specification, policy loader, read-only planner and `horizon-plan` | Applier, schema 4, archive, sync, shared service |
| Node conflict resolution | N0 and N1 | Policy format, strategy trait, laws harness, evidence, `--explain`, a behavioural no-op | New strategies (N2 on), review gating |
| Witnesses and transparency | W0, W1 core, W2 verifier | Merkle log, tiles, signed checkpoints, verifier and policy, vectors | Outbox, worker, endpoints, witness service, sync integration; needs independent review |
| Cross-document merge | Designed only | none | everything |
| Large-file acceptance, installed-host acceptance | Designed only | benchmark corpus tools (PR #28) | The runs themselves (the host runs are the owner's) |
| Key files | Argon2id added | memory-hard derivation, `device rewrap` | hardware-token keys |
| Processors and operating systems | Built in CI, public engine repository | Build and full test run on Linux x86-64 and Arm, macOS Arm and Intel, Windows x86-64 and Arm (tier 2); real key stores on all three systems; prebuilt `engine-cli` and `engine-api` for six targets (unsigned); a `linux/amd64` + `linux/arm64` container image; two Windows key-file defects found and fixed | A published release or image (tag paths have not run), signing, notarisation, a timing test excluded on three platforms |
| Browsers and phones | Built, emulation only | Playwright matrix (Firefox, WebKit, Pixel and iPhone emulation), phone layouts, touch layer, an installable web app with a cache-nothing-private service worker | Real devices, Safari itself, two WebKit tests, three Firefox full-run flakes |
| Desktop client (Tauri 2) | Built, Windows-run only | `apps/desktop`: launcher, hardened navigation, deep links, device-key sidecar, two independent security reviews and fixes, 97 tests, unsigned Windows installers built | macOS and Linux runs, signing, auto-update, a CI run of the bundle jobs |
| Android and iPhone apps | Prepared | Mobile entry point and capability file, harden and manifest-check scripts, workflow jobs written | iOS anything, store accounts, signing, a CI run (see the Android row of the ledger once built) |

The owner decisions in section 5 are unchanged; the ledger entries add the new questions each build raised.
