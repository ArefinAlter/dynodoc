# Dynodoc self-hosted edition: master design

> **Status: design, not implemented.** 14 October 2026. Nothing in this document is
> shipped, tested or promised. Every capability marked MISSING or PARTIAL is a
> proposal. Where a claim about the current system is made, the code or document
> that supports it is cited. Companion documents:
> [deployment guide](SELF-HOSTED-DEPLOYMENT-GUIDE.md),
> [identity and access](SELF-HOSTED-IDENTITY-AND-ACCESS.md),
> [licensing and distribution](SELF-HOSTED-LICENSING-AND-DISTRIBUTION.md).

Owner decisions this design takes as given (14 October 2026):

- Target customers are teams of any size, enterprise and academic, working on
  multiple documents. The design covers three profiles explicitly (section 2).
- The application (SvelteKit workspace, admin console, add-ins) stays private and
  proprietary; the engine, HTTP API and CLI stay MIT
  ([decision 013](decisions/013-private-application-licence.md)). Copyright holder:
  Verne Ltd.
- `hyperformula` (GPL-3.0-only or commercial) was replaced on 14 October 2026 by an
  in-house, dependency-free formula engine (see FORMULA-ENGINE-REPLACEMENT.md). The browser
  bundle no longer contains a copyleft formula library (section 14, risk R8).
- Self-hosted edition, AI-agent connectors and end-to-end encryption are documented now
  and not built now. This document does not change code and does not claim that any of
  the three exists.

Contents

1. Purpose, goals, non-goals, personas, success metrics
2. Customer profiles A, B and C
3. Current state versus target
4. External dependencies and how each is replaced or removed
5. Architecture, trust boundaries and data flows
6. Multi-instance work, specified
7. Office and Google add-in distribution per customer
8. Receipt authority and device keys when the customer holds the keys
9. Sizing model and its measurement gaps
10. Data residency and regulatory mapping
11. Security and privacy analysis
12. Upgrade policy and rehearsal tooling
13. Delivery plan
14. Risks and mitigations
15. First-customer validation plan
16. Cost-to-serve analysis (pricing-neutral)
17. Open questions
18. Appendices: glossary, file index, references

---

## 1. Purpose, goals, non-goals, personas, success metrics

### 1.1 Purpose

Dynodoc today is operated by one party (Verne Systems, Dhaka) on one VPS for a pilot:
two documents and 36 events at the last release record
([schema 40 release](operations/2026-10-13-schema40-release.md)). A customer who must keep
documents, identities and keys inside its own boundary cannot use that service. The
self-hosted edition is the same product, run by the customer, on infrastructure the
customer controls, with identity, mail, keys, backups and updates under the customer's
authority.

"Self-hosted" in this document means one installation, one customer, one database. It
does not mean multi-tenant hosting by Verne on the customer's behalf, and it is not a
managed single-tenant cloud offering (that is a different operating model with its own
cost and compliance profile; see section 16).

### 1.2 Goals

| ID | Goal | Evidence it would be met |
|---|---|---|
| G1 | A customer can install, run, back up, restore and upgrade the product without Verne having network or data access. | Air-gap acceptance run (P0-9); restore drill report. |
| G2 | The customer chooses the identity system. Email sign-in links, OIDC, SAML 2.0 and SCIM 2.0 are supported; Google sign-in is optional, never required. | Interop matrix (P1-10). |
| G3 | The customer holds every secret that matters: session keys, database credentials, the receipt-authority key and the licence file. Verne holds none of them. | Key inventory (section 8) shows Verne custody = none. |
| G4 | The same codebase serves hosted and self-hosted modes. Self-hosted is a configuration of the product, not a fork. | `DYNODOC_MODE` switch (P0-1); one CI pipeline. |
| G5 | Three sizes work: single node for a lab, replicated for a department or university, highly available and offline for a regulated enterprise. | Profile acceptance checklists (deployment guide, section 13). |
| G6 | Updates are verifiable offline, rehearsed on a copy of the customer's data before activation, and reversible. | Signed bundles (P3-1, P3-2); productised rehearsal (P0-5, P2-5). |
| G7 | The Office add-ins reach users through the channels the customer already operates. | Distribution matrix (section 7). |
| G8 | The product states plainly what it does not guarantee: no compliance claim, no capacity claim without a measurement, no end-to-end encryption claim. | Sections 9 to 11. |

### 1.3 Non-goals

- A multi-tenant SaaS inside the same installation. The database has `organization`
  spaces ([`migrations/0015_access_lifecycle_and_public_views.sql`](../migrations/0015_access_lifecycle_and_public_views.sql)),
  which are an access-control hierarchy, not a tenant-isolation boundary.
- Compliance certification of any kind. Phase 4 prepares evidence; it does not claim
  an audit result that does not exist.
- End-to-end encryption. The server must read content to reconstruct, review, merge,
  search and detect copies ([product specification, "Storage and transfer contract"](PRODUCT-SPEC.md)).
  A client-encrypted mode is a separate design
  ([E2EE-CRYPTOGRAPHIC-DESIGN.md](E2EE-CRYPTOGRAPHIC-DESIGN.md),
  [E2EE-PROTOCOL-AND-MIGRATION.md](E2EE-PROTOCOL-AND-MIGRATION.md)) and would disable
  server-side features that depend on plaintext.
- Hosting the customer's identity provider, mail relay, database cluster or KMS. The
  design integrates with them and documents what they must provide.
- Feature parity between hosted and self-hosted for Google-dependent features. Google
  Docs/Sheets/Slides add-ons and Google sign-in need Google's cloud (section 4).
- SLAs. Section 16 and the licensing document describe a support model without
  contractual response times.
- Replacing the receipt-authority rotation work
  ([REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md)), the formula-engine
  replacement, the AI-agent connectors ([AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md))
  or the end-to-end encryption design ([E2EE-CRYPTOGRAPHIC-DESIGN.md](E2EE-CRYPTOGRAPHIC-DESIGN.md)).
  They are referenced where they interact with this design and are excluded from its
  estimates.

### 1.4 Personas

| Persona | Profile | Needs | Fails if |
|---|---|---|---|
| Lab manager / research coordinator | A | Install in an afternoon, no database knowledge, simple backups, email or Google sign-in. | Install needs Rust or Node toolchains; backups are a manual `docker compose exec`; upgrades need a rehearsal they cannot run. |
| Department IT administrator | B | SSO through the university or company IdP, group-driven access, monitoring, a defined upgrade window, backup drills. | Roles must be assigned by hand for hundreds of users; no way to see health without shell access. |
| Platform / SRE engineer | B, C | Helm chart, health and readiness probes, metrics, rolling upgrades, documented failure modes. | Replicas silently diverge (stale live streams); upgrades require full outage. |
| Security officer / CISO team | C | SBOM, signatures, no phone-home, customer-managed keys, audit export to SIEM, threat model, pen-test history. | Any default outbound connection; unverifiable artefacts; secrets that cannot be rotated. |
| Data protection officer / records manager | B, C | Where data lives, retention and erasure semantics, sub-processor list (none), regulatory mapping. | The product asserts compliance it cannot evidence. |
| Instructor / research PI | A, B | Their students or collaborators get access without per-person invitations; leavers lose access. | Offboarding is manual and incomplete. |
| Document author / reviewer | all | Add-in in Word/Excel/PowerPoint works on the first try, offline-tolerant. | Add-in install requires admin steps the user cannot do. |
| Verne support engineer | all | Diagnose from a redacted bundle without shell access. | Needs production data to reproduce. |

### 1.5 Success metrics (targets to validate, not measured today)

| Metric | Target | How measured |
|---|---|---|
| Time from empty VM to first successful sign-in (Profile A, IT generalist, documentation only) | median 60 minutes or less | Design-partner observation (section 15) |
| Restore drill: from backup file to verified service on a clean host | 4 hours or less for Profile A/B, documentation only | Drill report generated by `dynodoctl` (proposed) |
| Rehearsal pass rate: upgrades whose rehearsal passes on first attempt | 95% or more over the first 20 customer upgrades | Rehearsal reports |
| Default outbound connections in air-gap mode | zero | Egress-blocked CI run (P0-9) |
| Signature verification of every shipped artefact offline | 100% | Bundle verify step in CI |
| Add-in availability after go-live (Profile B) | 90% of intended users within five business days | Customer survey; usage report |
| Support effort after month three | Profile A: 1 ticket per month or fewer; Profile B: 4 or fewer | Support tracker |
| Open critical/high findings at release | zero | Security review gate |

---

## 2. Customer profiles A, B and C

The profiles are design targets, not product tiers. Section 3 of the
[licensing document](SELF-HOSTED-LICENSING-AND-DISTRIBUTION.md) maps them to editions.

| Dimension | A: small team / lab | B: department / university / mid-size | C: enterprise / regulated |
|---|---|---|---|
| Registered users | up to 50 | 50 to 5,000 | 5,000 to a stated ceiling of 50,000 per installation until measured (see section 9) |
| Operations skill | Low; one generalist, part-time | Dedicated IT team, on-call optional | SRE/platform team, change control, CAB |
| Topology | One node, Docker Compose | 2 to 6 nodes or small Kubernetes; external PostgreSQL | Kubernetes or VM HA, 3-node PostgreSQL, multiple zones/sites |
| Network | Internet or LAN | Campus network; limited egress through a proxy | Intranet only or air-gapped; no egress; internal CA |
| Identity | Email link, Google or one OIDC IdP | OIDC/SAML SSO, optional SCIM, group mapping | SSO mandatory, SCIM, email link disabled, service accounts, MFA enforced at IdP |
| Keys | Files on the node, two offline copies | Files from a secret store; escrow with two custodians | KMS/HSM-held receipt authority; secrets from Vault/KMS; separation of duties |
| Backup | Nightly dump to another disk or share | PITR with WAL archive; monthly restore drill | PITR plus cross-site replica; quarterly drill; evidence retained |
| Monitoring | Built-in status page, email alert | Prometheus/Grafana; alerts to chat/on-call | Metrics, logs and audit events into SIEM |
| Upgrade | Manual, minutes of downtime, rehearsal-lite | Scheduled window, rehearsal on a copy | Rolling where schema permits (needs P2-5), rehearsal mandatory, change records |
| Add-ins | Sideload or shared folder | Admin center or SharePoint catalog or GPO | Same, from internal catalog; Google add-ons unavailable if intranet-only |
| Compliance asks | Short questionnaire | DPA, security questionnaire, accessibility statement | SOC 2/ISO 27001-style evidence, pen test report, SBOM, export-control statement |
| Support expectation | Documentation, email | Email with diagnostics bundle, upgrade assistance | Named contact; security advisories; no remote access |

What each profile must never be forced to do:

- A must never need to understand PostgreSQL roles, `grant-runtime`, Paseto keys or
  Nginx buffers. The installer and `dynodoctl` hide them.
- B must never need Verne to touch the installation.
- C must never need any outbound connection, any Verne-operated service, or any
  cleartext secret on disk outside its secret store.

---

## 3. Current state versus target

Tags: **EXISTS** (works today; code path given), **PARTIAL** (works but with a named
limitation), **MISSING** (not present). "Today" means the repository at commit `994c429`
(14 October 2026) and the production record of schema 40.

### 3.1 Deployment and operations

| # | Capability | Tag | Evidence and limitation |
|---|---|---|---|
| 1 | Container definitions for API, web, converter, PostgreSQL with read-only root, dropped capabilities, `no-new-privileges`, memory/CPU/PID limits, internal networks | EXISTS | [`infra/demo/compose.yml`](../infra/demo/compose.yml) |
| 2 | Compose usable by a stranger | PARTIAL | Hardcodes `ORIGIN: https://admin.dynodoc.online` and `PUBLIC_WORKSPACE_URL` for the admin service, binds `127.0.0.1:3088/3089`, mounts `../../ops`, expects `/opt/dynodoc-demo`, builds from source on the host (`context: ../..`). |
| 3 | Published, signed, multi-arch images | MISSING | `.github/workflows/` has only `ci-rust.yml` and `ci-frontend.yml`; images are built on the VPS (`Dockerfile.api` uses `rust:1.96-bookworm`). |
| 4 | Reverse proxy and TLS templates | PARTIAL | [`nginx-app.conf`](../infra/demo/nginx-app.conf): correct CSP/HSTS/header buffers and safe access log, but hostnames, certbot paths and the shared-host assumptions are fixed. |
| 5 | Logical backup with archive validation and checksum | EXISTS | [`backup.sh`](../infra/demo/backup.sh): `pg_dump -Fc`, `pg_restore --list`, `.sha256`, 14-day retention. Same host, path `/opt/dynodoc-demo`, no encryption, no off-host copy. |
| 6 | Restore procedure | PARTIAL | Documented in [VERCEL-DEPLOYMENT.md](VERCEL-DEPLOYMENT.md) and [HANDOFF.md](HANDOFF.md) (restore into a new database); no script, no automated verification. |
| 7 | Point-in-time recovery | MISSING | No WAL archiving configured anywhere. |
| 8 | Release rehearsal on a production copy and scripted activation | PARTIAL | Process recorded in [operations/2026-10-13-schema38-release.md](operations/2026-10-13-schema38-release.md); `rehearse.py` and `activate.py` live in private release directories (`/opt/dynodoc-releases/<date>-<name>/`), not in the repository. Only [`package-source.py`](../infra/demo/package-source.py), [`check-connectors.py`](../infra/demo/check-connectors.py), [`collect-status.py`](../infra/demo/collect-status.py) and `backup.sh` are versioned. |
| 9 | Source packaging with manifest | EXISTS | `package-source.py` archives a committed revision with `SOURCE_MANIFEST.sha256`; excludes env files, keys, dumps. |
| 10 | Read-only delivery check | EXISTS | `check-connectors.py`: health, auth options, byte-equality of six packages and icons, Office frame headers, anonymous 401s. Default origin hardcoded; compares against a source checkout, which a customer does not have. |
| 11 | Host status collector | PARTIAL | `collect-status.py` calls `docker inspect` on fixed container names and also prunes `web_metric` rows older than 30 days; not portable to Kubernetes, and retention is silently coupled to a host cron. |
| 12 | Startup validation of secrets and database boundary | PARTIAL | API fails closed on missing/short `PASETO_LOCAL_KEY`, `WEB_SERVICE_KEY`, on `MIGRATION_DATABASE_URL`, and on privilege/migration mismatch ([`main.rs`](../crates/engine-api/src/main.rs), [`database.rs`](../crates/engine-core/src/database.rs)). The web process validates its whole configuration when it starts, `SESSION_SECRET` included, and reads every secret from `NAME` or `NAME_FILE` (P0-1 and P0-2 web half, 22 October 2026; `frontend/researcher/src/lib/server/config/`). `engine-cli` still reads plain variables. |
| 13 | Structured logging, metrics endpoint, trace export | PARTIAL | `tracing_subscriber::fmt` and `TraceLayer` only; no `/metrics`, no OTLP exporter, no JSON log mode. |
| 14 | Retention of expired authentication rows | MISSING | No pruning of `magic_link`, `refresh_token`, `auth_request_window` outside erasure paths (grep of `crates/`); `idempotency_key` is pruned on reuse; `web_metric` by the host collector. |

### 3.2 Identity and access

| # | Capability | Tag | Evidence and limitation |
|---|---|---|---|
| 15 | Email sign-in link (15 minutes, single use, hash stored, scanner-safe POST confirmation) | EXISTS | `crates/engine-api/src/auth/routes.rs`, `routes/auth/[...action]/+server.ts`. Delivery only through Resend (`api.resend.com`). |
| 16 | Google sign-in (OIDC authorization code with PKCE) | EXISTS | `routes/auth/[...action]/+server.ts` using `openid-client`; discovery URL fixed to `accounts.google.com`. |
| 17 | Generic OIDC | MISSING | `openid-client` and `jose` are already dependencies; configuration is the gap. |
| 18 | SAML 2.0 | MISSING | |
| 19 | SCIM 2.0 | MISSING | |
| 20 | LDAP | MISSING | |
| 21 | Group or claim to role mapping | MISSING | Memberships are manual (`space_member`, `document_access`). |
| 22 | Administrator model | PARTIAL | `ADMIN_EMAILS` allowlist matched on email (`admin.rs: is_admin`); no roles, no auditor. |
| 23 | Revocable sessions, per-login lineage, account generation | EXISTS | Schema 27; `auth/store.rs`. No per-session list or revoke UI. |
| 24 | Device keys, project-scoped sessions, per-project grants | EXISTS | Schemas 33, 38, 40; [DEVICE-KEY-GRANTS-API.md](DEVICE-KEY-GRANTS-API.md). |
| 25 | Non-human (service) identities | MISSING | An identity requires an email today (`identity.email not null`, `0004_identity.sql`). |
| 26 | Configurable token lifetimes | PARTIAL | `TOKEN_TTL_SECONDS` only (code default 86,400; compose default 3,600). Refresh lifetime is the constant `REFRESH_TTL_SECONDS` (30 days). |
| 27 | Invitation email | EXISTS | `lib/server/invitations.ts`; Resend only; per-process rate maps. |

### 3.3 Multi-instance behaviour

| # | Capability | Tag | Evidence and limitation |
|---|---|---|---|
| 28 | API replicas share no session state | EXISTS | PASETO local tokens validated against the database on every request (`auth/routes.rs: AuthContext`, `auth/store.rs: ensure_bound_session`). All replicas need the same `PASETO_LOCAL_KEY`. |
| 29 | Live event stream across replicas | MISSING | `Subscriptions` is a process-local `HashMap` of `tokio::sync::broadcast` senders (`stream.rs`); `publish` is called after commit in the handler that wrote. |
| 30 | Fleet-wide rate and quota limits | PARTIAL | Authentication and device-session issuance windows are in PostgreSQL (`auth_request_window`, `device_session`); admission and expensive-read budgets are process-local (`workloads.rs`). |
| 31 | Idempotency keys across replicas | EXISTS | PostgreSQL table, reserve-then-complete (`idempotency.rs`). |
| 32 | Snapshot and checkpoint writers safe under concurrency | EXISTS | `pg_try_advisory_xact_lock` (`snapshot.rs`), per-document advisory locks (`shared_checkpoint/postgres.rs`, `draft_storage.rs`). |
| 33 | Background copy indexer on exactly one replica | MISSING | `copies::spawn` runs in every API process (`main.rs`); no lease. |
| 34 | Migration serialisation | EXISTS | `engine-cli migrate` uses sqlx `Migrator::run`, which takes `pg_advisory_lock` keyed from the database name and runs each migration in a transaction (sqlx source, checked 14 Oct 2026). The API never migrates. |
| 35 | Running an N-1 binary against schema N | MISSING | `check_schema` requires the applied migration list to equal the binary's list exactly; `check_runtime` compares an exact table and privilege inventory. Every release record states the consequence: rollback = restore a dump. |
| 36 | Refresh-token rotation safe across several web replicas | MISSING | `refreshes` map in `lib/server/session.ts` is process-local; two replicas presenting the same old refresh token race and one gets `RefreshInvalid` (cookie cleared). |

### 3.4 Keys

| # | Capability | Tag | Evidence and limitation |
|---|---|---|---|
| 37 | Receipt authority private key supplied as a file, public key installed once | EXISTS | `DYNODOC_REF_SIGNING_KEY_FILE`, `server_authority.rs`, [compose.schema38.override.yml](../infra/demo/compose.schema38.override.yml). Not installed in production ([decision](RECEIPT-AUTHORITY-DECISION.md)). |
| 38 | Authority key in KMS/HSM | MISSING | `AppState.ref_signer` is an in-process `ring` `Ed25519KeyPair`. |
| 39 | Authority rotation and recovery | PARTIAL | Epoch-chain design and source contract for schema 41 in [REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md); not deployed and not part of this plan's estimates. |
| 40 | Paseto key rotation without ending all sessions | MISSING | One `PASETO_LOCAL_KEY`; rotating ends every session ([PROJECT-SCOPED-DEVICE-SESSIONS.md](PROJECT-SCOPED-DEVICE-SESSIONS.md), caveats). |
| 41 | Installation identity | EXISTS | Read-only `device_authority` UUID created by migration 0033. |

### 3.5 Add-ins

| # | Capability | Tag | Evidence and limitation |
|---|---|---|---|
| 42 | Six add-ins as test builds | EXISTS | `extensions/{word,excel,powerpoint}/manifest.xml`, `extensions/google-*`. Not listed on any marketplace ([ADDIN-PUBLICATION-AND-ACCEPTANCE.md](ADDIN-PUBLICATION-AND-ACCEPTANCE.md)). |
| 43 | Manifest and package for a customer origin | PARTIAL | Documented as a manual find-and-replace of `https://app.dynodoc.online` on six lines of each Office manifest (plus a `SupportUrl` line that names the marketing site) and in `Code.gs`/`appsscript.json` for Google ([extensions/INSTALL.md](../extensions/INSTALL.md)). Packages are static files built by `scripts/build-connectors.mjs`. |
| 44 | Office.js served locally | MISSING | Panes load `https://appsforoffice.microsoft.com/lib/1/hosted/office.js` (`routes/connectors/word/+page.svelte`, `static/connectors/commands.html`). |
| 45 | Manifest without `VersionOverrides` for SharePoint catalogs | MISSING | All three manifests use `VersionOverrides` for the ribbon group. |

### 3.6 Licensing, supply chain and storage

| # | Capability | Tag | Evidence and limitation |
|---|---|---|---|
| 46 | Licence file verification and metering | MISSING | |
| 47 | SBOM, third-party notices, signatures | PARTIAL | `/licenses` page lists direct dependencies and fonts; no SBOM or signed artefacts. |
| 48 | Originals stored in PostgreSQL | EXISTS | `document_upload.content bytea`, check 20 MiB in the schema, 3,000,000 bytes in the application (`file-limits`). Included in every backup. |
| 49 | Object-storage backend for large content | MISSING | Named as a plan item in [ENGINE-EVOLUTION-PLAN.md](ENGINE-EVOLUTION-PLAN.md) part 2 ("selected through a measured spike"). |
| 50 | Per-document and per-account storage budgets | EXISTS | Schema 34; 1 GiB / 250,000 objects per document, 4 GiB / 1,000,000 per account ([SHARED-STORAGE-BUDGETS.md](SHARED-STORAGE-BUDGETS.md)). |
| 51 | Restricted runtime database role with startup gate | EXISTS | Schema 29; [DATABASE-PRIVILEGES.md](DATABASE-PRIVILEGES.md). |
| 52 | Independent audit anchors or witnesses | MISSING | Open item in the evolution plan, part 6. |

---

## 4. External dependencies and how each is replaced or removed

This inventory was built by searching the web tier, API, CLI, converter, compose files
and extensions for every network destination (14 October 2026). A dependency is listed
if the code can contact it at run time, or if a browser or Office client is told to.

| # | Destination | Where in the code | Purpose | Self-hosted default | Air-gapped replacement or removal |
|---|---|---|---|---|---|
| D1 | `accounts.google.com`, `openidconnect.googleapis.com` | `routes/auth/[...action]/+server.ts` | Google sign-in | Off unless `GOOGLE_CLIENT_ID`/`SECRET` set | Remove; customer OIDC or SAML (P1-1, P1-8). The Google flow becomes one preset of generic OIDC. |
| D2 | `forms.googleapis.com`, `forms.google.com` | `routes/integrations/google/[action]`, `routes/api/google-forms` | Import a Google Form | Off | Removed; feature hidden when Google is not configured. |
| D3 | `api.resend.com` | `lib/server/invitations.ts: sendEmail`, `routes/auth/[...action]/+server.ts` | Sign-in links and invitations | Replaced by SMTP (P0-3); Resend stays an optional transport | Intranet SMTP relay, or no mail: operator-issued sign-in links (P0-3) and SSO only. |
| D4 | `appsforoffice.microsoft.com` (legacy), `officeapis.public.onecdn.static.microsoft`, `ajax.aspnetcdn.com` | `routes/connectors/*/+page.svelte`, `static/connectors/commands.html`, `lib/server/security.ts`, `nginx-app.conf` CSP maps | Office.js for the three panes | CDN, with the newer domain | Serve a local copy of Office.js and relax nothing else (P0-7). See licensing caveat in section 7.3. |
| D5 | Google Apps Script runtime and `script.google.com` services | `extensions/google-*` | Google add-ons | Works only if the Dynodoc origin is reachable from Google's servers | Impossible air-gapped or intranet-only (section 7.4). |
| D6 | `https://app.dynodoc.online/telemetry` | `lib/telemetry.ts` (fixed destination and a four-host allowlist), `routes/telemetry/+server.ts` | Anonymous page-load metrics | No-op on any other host (hostname allowlist), but CSP `connect-src` still lists the hosted origin (`svelte.config.js`) | Remove CSP entry and the route in self-hosted mode (P0-1); metrics, if any, go to the customer's own system. |
| D7 | `dynodoc.online` links, canonical URL, `IndexNow`, Search Console key file | `lib/seo/site.ts`, `login/+page.svelte`, `workspace/+page.svelte`, `HelpGuide.svelte`, `scripts/indexnow.mjs`, `static/*.txt` | Marketing and SEO | Not shipped in self-hosted mode (P0-1) | Same. |
| D8 | `https:` image sources inside documents | `svelte.config.js` CSP `img-src 'self' data: blob: https:` | Author-linked images | Allowed (browser fetches directly from the third-party host) | Tighten to `'self' data: blob:` in air-gap mode; document that linked images then do not render. |
| D9 | Customer-chosen research servers (ODK Central, KoboToolbox, REDCap, SurveyCTO, SurveyMonkey) | `lib/server/research.ts`, `research-vendors.ts` | Import questionnaires and responses | Allowed; requires a public IPv4 address (private ranges rejected by `publicIPv4`) | Off in air-gap mode; an explicit allowlist of private hosts is a proposed option (open question Q14). |
| D10 | Let's Encrypt ACME | host certbot (outside the repo) | TLS | Optional | Internal CA or customer-provided certificate. |
| D11 | Docker Hub (`postgres:16`, `node:24-bookworm-slim`, `debian:bookworm-slim`), crates.io, npm | `infra/demo/Dockerfile.*`, `compose.yml` | Build and run | Replaced by published images by digest | Offline bundle and internal registry mirror (P3-2). |
| D12 | Vercel | marketing site hosting | Public site | Not part of the bundle | Not applicable. |
| D13 | GitHub | `/source` redirect, CI | Public engine repository link | Redirect target `ENGINE_REPO_URL` | Remove route in self-hosted mode. |
| D14 | Fonts and icons | `static/fonts` (OFL) | UI | Local | Local; no change. |
| D15 | LibreOffice converter | `infra/converter` | Legacy `.doc` import | Internal, no egress, optional | Local container; optional bundle component. |

What is *not* a dependency, verified by the code search: analytics services, error
reporting, license servers, update checks, AI services, object storage, cache services.
The subprocessors page (`frontend/researcher/src/routes/(site)/subprocessors/+page.svelte`) states the same.
In self-hosted mode that page is replaced by a customer-editable notice, because the
hosted page names Verne Systems, Resend and Vercel and would be false for a customer.

Hosted-only strings that must be parametrised or removed (task P0-1) are indexed in
appendix B.

---

## 5. Architecture, trust boundaries and data flows

### 5.1 Components and responsibilities

| Component | Technology | Responsibility | State |
|---|---|---|---|
| Reverse proxy | nginx (bundled) or customer LB | TLS termination, header policy, request limits, safe access log, static fallbacks for CSP on error responses | None |
| Web tier ("web") | SvelteKit, Node 24, adapter-node | Pages, session cookie (encrypted), same-origin gateway `/api/engine/*`, sign-in flows, invitation mail, converter client, add-in panes and packages | Cookies only; process-local caches (section 6) |
| Admin surface | Same web build at `/admin` (or separate host) | Operations overview, records, account/document controls | None |
| API ("engine-api") | Rust, Axum 0.7, sqlx | Authorization, event log, snapshots, drafts, reviews, projects, device sessions, SSE | PostgreSQL; process-local registries (section 6) |
| CLI ("engine-cli") | Rust | `migrate`, `grant-runtime`, `check-runtime`, `verify`, `verify-snapshots`, `install-ref-authority`, local repository and `sync` | None |
| Database | PostgreSQL 16 | System of record: events, snapshots, shared objects, identities, access, audit, originals | All durable state |
| Converter | LibreOffice headless behind a 56-line HTTP shim | `.doc` to `.docx`, 3,000,000-byte cap, 2 slots, macros disabled | None |
| Operator tool (proposed) | `dynodoctl` | Install, configure, backup, restore-verify, rehearse, upgrade, bundle verify/import, support bundle | Local files |
| Add-in packages | Static XML and ZIP | Office manifests, Apps Script sources | Generated per origin (P0-7) |

### 5.2 Reference topology (Profile A)

```
                    users (browser, Office add-ins, engine-cli)
                                   |
                         443 TLS   |
                    +--------------v-------------+
                    |  nginx (bundled)           |  CSP/HSTS/limits, safe logs
                    +--------------+-------------+
                                   | http
                    +--------------v-------------+      +----------------+
                    |  web (SvelteKit, Node)     +----->+ converter      |  internal net
                    |  cookies, gateway, auth    |      | (optional)     |
                    +--------------+-------------+      +----------------+
                                   | http, service key + bearer
                    +--------------v-------------+
                    |  api (Rust)                |---- outbound: SMTP relay, IdP (443)
                    +--------------+-------------+     (web tier makes these calls)
                                   | tcp 5432, restricted runtime role
                    +--------------v-------------+
                    |  PostgreSQL 16             |  volume: data; backups: other disk/share
                    +----------------------------+
   operator only:  engine-cli migrate / grant-runtime (owner credential, separate process)
```

The outbound calls to the IdP and SMTP relay are made by the web tier, not the API. The
API makes no outbound network calls other than to PostgreSQL (verified: no HTTP client
dependency in `crates/engine-api/Cargo.toml`).

### 5.3 Reference topology (Profile C, air-gapped)

```
   Customer intranet (no route to the Internet)
   +------------------------------------------------------------------------+
   | Users: browsers, Office desktop, engine-cli                           |
   |        |                                                               |
   |   [internal LB / WAF, internal CA certs]                               |
   |        |                                                               |
   |   +----+-----+  +----------+   web x2..n   (stateless, shared secrets)|
   |   |  web  1  |  |  web  2  |                                          |
   |   +----+-----+  +----+-----+                                          |
   |        +------+------+                                                 |
   |   +----v-----+  +----v-----+  +----------+   api x3..n                 |
   |   |  api  1  |  |  api  2  |  |  api  3  |   LISTEN/NOTIFY fan-out     |
   |   +----+-----+  +----+-----+  +----+-----+   one job leader (lease)    |
   |        +------+------+------+------+                                   |
   |   +----v---------------------------+      +--------------------------+ |
   |   | PostgreSQL primary (sync repl) +----->+ standby + WAL archive    | |
   |   +--------------------------------+      +--------------------------+ |
   |                                                                        |
   |   IdP (SAML/OIDC)   SMTP relay   SIEM   KMS/HSM   internal registry   |
   |   Secrets: Vault / Kubernetes secrets / CSI                            |
   +------------------------------------------------------------------------+
   Updates: signed offline bundle -> verify -> import to registry -> rehearse -> activate
```

### 5.4 Trust boundaries

| Boundary | Between | Authentication | What crosses |
|---|---|---|---|
| TB1 | Internet or intranet and the proxy | TLS server auth; none for anonymous pages | HTTP requests |
| TB2 | Proxy and web | Network; `X-Forwarded-*` trusted from proxy only | Same |
| TB3 | Web and API | `x-dynodoc-service-key` (HMAC-compared) for issuance routes; bearer PASETO for user routes | Tokens, document JSON |
| TB4 | Web and converter | `X-Converter-Key` | `.doc` bytes (cap 3,000,000) |
| TB5 | API and PostgreSQL | Restricted runtime login; startup privilege gate | SQL |
| TB6 | Operator and PostgreSQL | Owner/migration credential, separate process | DDL, grants |
| TB7 | User device and API | Device key signature (project device session) or account token | Commits, objects |
| TB8 | Office pane (customer origin, inside Office web view) and Microsoft | Office host | Office.js; no Dynodoc data to Microsoft by design (pane talks to the customer origin only) |
| TB9 | Google Apps Script and the Dynodoc origin | `connector` key; origin whitelist | Change bundles; requires public reachability |
| TB10 | Customer and Verne | Out-of-band: signed bundles in, optional redacted support bundle out | Artefacts, licence file |

TB10 is the new boundary this edition creates. It is designed to be one-way by default:
Verne ships signed artefacts and licence files; nothing flows back unless the customer
chooses to send a support bundle.

### 5.5 Data flows that matter for self-hosting

1. **Sign-in (email link).** Browser posts email to web; web asks API for a link
   (`/auth/magic-link`, service key); web sends it through SMTP; user opens link;
   browser posts token to web, web calls `/auth/verify`; API returns access and
   refresh tokens; web stores them in an encrypted HttpOnly cookie.
2. **Sign-in (OIDC, target).** Browser to web `/auth/oidc/{provider}`; web redirects to
   IdP with PKCE and nonce; callback validates ID token, maps claims and groups; web
   calls `/auth/exchange` with the verified identity (service key); API links or creates
   the identity (sections 4 and 5 of the [identity document](SELF-HOSTED-IDENTITY-AND-ACCESS.md)).
3. **Edit and review.** Browser to `/api/engine/*` gateway; gateway adds the bearer
   token; API appends events in a transaction, publishes to subscribers, returns.
4. **Live updates.** Browser opens `/documents/{id}/stream` through the gateway; API
   replays from `Last-Event-ID` then streams new events (section 6.1 for the
   multi-instance change).
5. **Add-in capture.** Office pane (origin = customer) holds a file-scoped connector key
   in memory; calls `/api/connectors/*`; the gateway forwards to `/connector/*`.
6. **CLI sync.** `engine-cli sync` signs a one-use challenge with the device key,
   receives a one-hour project token, transfers missing objects and commits.
7. **Backup.** `pg_dump -Fc` of the whole database: content, history, identities,
   audit, originals and chain data.

---

## 6. Multi-instance work, specified

### 6.0 Scope and rules

Running more than one API process or more than one web process behind a load balancer
is required by Profiles B (optional) and C (mandatory). Today the code is correct for
exactly one API process and, for most paths, one web process. The inventory below was
produced by searching `crates/engine-api`, `crates/engine-core` and
`frontend/researcher/src` for process-global state (`Mutex`, `OnceCell`, module-level
`Map`, `tokio::spawn`, `static`).

Rules for every change in this section:

1. Single-process behaviour does not change. Each feature has a `local` mode, which is
   the default, and a shared mode selected by configuration. Profile A never needs the
   shared mode.
2. No change edits an applied migration. New objects arrive as new numbered migrations
   (0041 is taken by the authority-epoch work in progress, `0041_ref_authority_epochs.sql`;
   numbers used below are illustrative and are allocated at build time).
3. Event append-only guarantees and hash-chain fields are untouched.
4. A shared mode must fail visibly (readiness fails, metric increments), not silently
   degrade to stale data.

### 6.1 Process-global state inventory

| # | State | Location | Behaviour with N > 1 | Section |
|---|---|---|---|---|
| S1 | SSE subscription registry (`Subscriptions`, per-document `broadcast` channel, 256-event ring, 100-subscriber cap) | `crates/engine-api/src/stream.rs` | Subscriber on replica B never sees events committed through replica A | 6.2 |
| S2 | Admission semaphores and per-actor/project counters (`Workloads`: 64 bodies, 256 streams, 8 expensive reads; 120/min and 2 parallel per identity; 600/min and 4 parallel per project) | `crates/engine-api/src/workloads.rs` | Limits multiply by N; fairness is per replica | 6.3 |
| S3 | Copy-detection indexer loop (`copies::spawn`) and `CopyIndex` wake | `crates/engine-api/src/copies.rs`, `main.rs` | N loops race over the same stale documents | 6.5 |
| S4 | Web refresh de-duplication map (`refreshes`) | `frontend/researcher/src/lib/server/session.ts` | Concurrent refresh with the same cookie on two replicas: one is rejected and the cookie is cleared | 6.4 |
| S5 | Invitation rate maps (`perInviter`, `perRecipient`) | `frontend/researcher/src/lib/server/invitations.ts` | Limits multiply by N | 6.3 |
| S6 | OIDC discovery cache (`configuration` promise) | `routes/auth/[...action]/+server.ts`, `routes/integrations/google/[action]/+server.ts` | Per process, harmless; a failed discovery is retried | 6.6 |
| S7 | `device_authority` `OnceCell` | `lib.rs` | Read-only installation UUID; harmless | none |
| S8 | Snapshot "ensure recent" spawn | `engine-core/src/snapshot.rs` | Guarded by `pg_try_advisory_xact_lock` per document; safe | none |
| S9 | Shared-checkpoint publication | `shared_checkpoint/postgres.rs` | Per-document advisory lock; safe | none |
| S10 | Idempotency keys | `idempotency.rs` | PostgreSQL table; safe | none |
| S11 | Database pool (8 connections, 5 s acquire, 15 s statement, 5 s lock, 30 s idle-in-transaction) | `main.rs` | Hard-coded; PostgreSQL `max_connections` must cover N x 8 plus headroom | 6.6 |
| S12 | Receipt signer `Arc<Ed25519KeyPair>` | `lib.rs`, `main.rs` | Every replica needs the same key file; no cross-replica state | section 8 |
| S13 | Host status collector and `web_metric` pruning | `infra/demo/collect-status.py` | Host-specific; retention duty silently coupled to it | 6.5 |

### 6.2 Live event fan-out through PostgreSQL LISTEN/NOTIFY

**Current behaviour.** A write handler appends events in a transaction and, after
commit, calls `state.subscriptions.publish(document_id, event)` (nine call sites across five files in
`ops/apply.rs`, `workspace.rs`, `reviews.rs`, `discussions.rs`, `conflicts/apply.rs`).
The stream handler subscribes before pinning its backfill head, replays pages of up to
100 events, then reads from the in-process receiver. If the next received sequence is
not `cursor + 1` it ends the stream so the client can reconnect with `Last-Event-ID`.
Heartbeats are sent every 30 seconds. A revoked session is noticed only when the next
event is delivered (`take_while` in `stream.rs`).

**Failure with several replicas.** Replica B holds the subscriber; replica A handles the
write. B receives nothing, sends heartbeats forever and the editor shows stale content
until the browser reconnects for another reason. Nothing is lost in the database; the
live channel is silently wrong. This is the single most important defect for any
replicated deployment.

**Design (proposed).**

1. A new migration adds a statement-level `AFTER INSERT` trigger on `event` with a
   transition table, so one notification is sent per document per statement, inside the
   inserting transaction (delivered only on commit, in commit order):

   ```sql
   -- proposed migration 0042_event_notify.sql (illustrative number)
   create function event_notify() returns trigger
     language plpgsql set search_path = pg_catalog, public, pg_temp as $$
   begin
     perform pg_notify('dynodoc_event', s.document_id::text || ':' || s.max_seq::text)
       from (select document_id, max(seq) as max_seq
               from new_rows group by document_id) s;
     return null;
   end $$;
   revoke all on function event_notify() from public;
   create trigger event_notify_after_insert
     after insert on event referencing new table as new_rows
     for each statement execute function event_notify();
   ```

   The payload is a document UUID and a sequence number (about 40 bytes, far below the
   documented 8,000-byte limit). Event bodies are never in the payload.
2. Each API process holds one dedicated session connection that runs
   `LISTEN dynodoc_event`. It is not part of the 8-connection pool and must not pass
   through a transaction-mode pooler.
3. On a notification `(doc, seq)`: if the process has no subscriber for `doc`, discard it.
   Otherwise schedule a catch-up read for `(delivered[doc], seq]` using the existing
   `engine_core::log::read_range_page` and push the rows through the document's
   existing `broadcast` sender. Coalesce notifications per document for 20 ms.
4. The local fast path stays: a handler that committed an event may publish it directly,
   but only if `event.seq == delivered[doc] + 1`; otherwise it triggers the catch-up
   path. A per-document `delivered` cursor makes both paths idempotent.
5. On listener reconnect, for every document with local subscribers, run one catch-up
   read from `delivered[doc]` (notifications sent while disconnected are not queued for
   the session).
6. Add a periodic (60 s) entitlement re-check per open stream (`ensure_bound_session`
   and `role_for`), independent of event delivery. This closes the "idle stream keeps
   heartbeating after revocation" gap noted in
   [SECURITY-HARDENING.md](SECURITY-HARDENING.md) and is required by the offboarding
   design in the [identity document](SELF-HOSTED-IDENTITY-AND-ACCESS.md).

Configuration: `DYNODOC_EVENT_FANOUT=local|postgres` (default `local`). In `postgres`
mode readiness fails while the listener is disconnected.

**Constraints and caveats.**

- LISTEN needs a session-mode connection. PgBouncer in transaction mode does not
  support LISTEN (PgBouncer feature table, checked 14 October 2026). If the customer
  puts a transaction pooler in front of PostgreSQL, the listener connection must bypass
  it, or the fallback below is used.
- The notification queue is 8 GB in a standard installation; if it fills, transactions
  that call NOTIFY fail at commit, and a listener session that keeps a transaction open
  blocks queue cleanup ([PostgreSQL 16 NOTIFY](https://www.postgresql.org/docs/16/sql-notify.html)).
  The listener must never hold a transaction; monitor `pg_notification_queue_usage()`.
- A transaction that calls NOTIFY takes a database-wide lock during commit so that queue
  order equals commit order. Reports exist of this serialising commits under many
  concurrent writers; a later PostgreSQL change is reported to have addressed it (not
  verified for the 16 series; check at build time). Dynodoc's expected write rate is
  low, but the statement-level batching above is deliberate, and the load harness
  (P2-11) must include a write-rate sweep that finds the knee.
- Fallback mode `DYNODOC_EVENT_FANOUT=poll`: each process polls
  `select document_id, max(seq) ... where document_id = any($1)` for documents with
  local subscribers every 250 to 500 ms. Higher latency, no LISTEN requirement. The
  existing 100 ms SSE target ([README](../README.md) definition of done) is met only by
  the `postgres` mode (single-process local delivery is unchanged).
- The per-document cap of 100 subscribers applies per process. With N replicas a hot
  document can have up to 100 x N live streams. Document, do not change.

**Tests.** (a) Two real API processes against one database: write via A, assert delivery
on a B subscriber within 250 ms p95. (b) Kill and restore the listener connection during
a write burst; no sequence gap is delivered. (c) Duplicate delivery (fast path and
notification) yields each seq once. (d) Revoke a session while the stream is idle; the
stream ends within 60 s. (e) The existing `sse_delivers_a_new_event_within_100ms` test
still passes in `local` mode.

### 6.3 Shared rate limits and admission

**What must stay local.** The semaphores that bound resident response bodies (64),
streams (256) and expensive reads (8) protect one process's memory and CPU. They are
correct as process-local bulkheads and must not be turned into distributed locks.

**What should become shared.** The per-identity and per-project windows (120/min and 600/min,
parallelism 2 and 4) express fairness and abuse control. With N replicas the effective
limit is N times larger and uneven. Two options:

| Option | Mechanism | Accuracy | Cost | Use for |
|---|---|---|---|---|
| Share-of-limit | Each replica enforces `ceil(limit / replicas)`; `DYNODOC_REPLICA_COUNT` configured | Approximate; unfair when the balancer is uneven | None | Profile B |
| PostgreSQL window | `unlogged` table `rate_window(key, window_start, used)` updated with `insert ... on conflict do update ... returning`; active parallelism stays per replica | Exact for rate, approximate for parallelism | One extra round trip per expensive read (already authenticated and heavy) | Profile C |

The authentication windows already follow the second pattern
(`auth_request_window`, five attempts per 15 minutes per email hash) and the device
issuance windows follow it with an advisory lock; the new table reuses their approach.
`DYNODOC_RATE_LIMIT_BACKEND=local|share|postgres` (default `local`).

The web-tier invitation limiter (30 per inviter per hour, one per recipient and item per
10 minutes) moves to the same table or accepts the N-times looser bound; the limits are
abuse controls, not security boundaries.

Fleet-wide tenant fairness beyond these windows (weighted queues, per-project CPU
budgets) is not designed here; [ENGINE-EVOLUTION-PLAN.md](ENGINE-EVOLUTION-PLAN.md)
part 7 owns it.

### 6.4 Sessions: stateless tokens, no stickiness required, one race to fix

**API tier.** Access tokens are PASETO v4.local verified with the shared key and then
checked against the database on every request (`ensure_bound_session` joins `identity`
and `refresh_token`). There is no server-side session cache. API replicas therefore need
**no sticky routing**, only the same `PASETO_LOCAL_KEY`, `WEB_SERVICE_KEY` and database.

**Web tier.** The session is an encrypted JWE cookie (`dynodoc_session`, A256GCM, key
derived from `SESSION_SECRET`) holding both tokens. Web replicas need the same
`SESSION_SECRET` and no stickiness for steady-state requests.

**The race.** When the access token is within 60 s of expiry the gateway calls
`/auth/refresh`. The engine rotates single-use refresh tokens; presenting a rotated token
yields `RefreshInvalid`, after which `accessToken()` clears the session cookie. The web
tier de-duplicates concurrent refreshes with an in-process map. Two requests carrying the
same cookie that land on different replicas (a normal pattern for a page that issues
parallel `fetch` calls) produce one winner and one loser, and the loser's response
clears the cookie.

| Option | Description | Verdict |
|---|---|---|
| Sticky routing on the session cookie | Balancer hashes `dynodoc_session`; both requests go to one replica | Works today with no code change; fragile (cookie value changes at each rotation, so the hash changes mid-burst) and a poor fit for rolling restarts. Acceptable as a Profile B stopgap. |
| Reuse interval in the engine (recommended) | If a presented refresh token was rotated less than `REFRESH_REUSE_INTERVAL_SECONDS` ago and its successor is still unrotated, return **the same successor** with a freshly minted access token for the same `session_id`. Successor secrets become deterministic (`HMAC(key derived from the service key, parent hash)`), so the plaintext is re-derived and never stored. Outside the interval a rotated token is rejected exactly as today. | Small change in `rotate_bound_refresh`; no schema change (the partial unique index `refresh_token_active_session_idx` allows one active successor per session, which this preserves). Replay detection is weakened only inside the interval. Default 0 (today's behaviour) for A, 15 s for B and C. |
| Shared result cache in the web tier | Encrypted `(old_hash -> new tokens)` rows in PostgreSQL with a 30 s TTL | Stores bearer material at rest, however briefly. Not recommended. |

Cookie size: signed-in responses already need 16 KiB proxy header buffers (see
[WORKSPACE-502-FIX.md](WORKSPACE-502-FIX.md)). Any added claims (IdP session id, group
summary) must stay out of the cookie and in `login_session` (identity document, 12.2).

### 6.5 Background jobs and retention

| Job | Today | Target |
|---|---|---|
| Copy-detection indexer | Loop in every API process: selects up to 25 stale documents every 5 s or on wake, fingerprints, writes `document_fingerprint*`, notifies owners | Run on one replica using a lease: `job_lease(name primary key, holder, expires_at)` renewed every 10 s, or a session advisory lock `pg_try_advisory_lock` on a dedicated connection. Per-document work also uses `for update skip locked` so a lease handover cannot double-process. Config `DYNODOC_COPY_DETECTION=on|off` (off is a valid privacy choice for customers who do not want cross-document similarity). |
| Periodic snapshots | `spawn_ensure_recent` after append | Unchanged (advisory-lock guarded) |
| Retention | Host script deletes `web_metric` rows older than 30 days; nothing prunes expired `magic_link`, `refresh_token` (one row per rotation), `auth_request_window`, expired `device_enrollment_challenge` | A retention job on the lease holder: expired links, consumed or expired refresh rows older than a configurable window, expired windows, old `web_metric`. Never touches history tables (append-only guards would refuse). Metrics for rows pruned. |
| Status collection | `collect-status.py` + mounted JSON | API-native operations endpoint (`/admin/ops`) reading `pg_stat_*`, pool state, listener state, backup status file written by `dynodoctl`; the host script is not needed in Kubernetes. |
| Outbound mail queue | Synchronous send, 15 s timeout, failure returns 503 | Database outbox with retry and visibility in the admin surface (Phase 2 candidate; Profile A may keep synchronous send). |

The retention gap is also a sizing input: with a one-hour access token and a person
active eight hours a day, about eight refresh rows are written per person per active day.
For 2,000 people over 250 working days that is about four million rows a year in a table
nothing prunes (section 9).

### 6.6 Smaller items

- **Readiness and liveness.** `/healthz` returns 200 without touching PostgreSQL
  (`lib.rs: healthz`). Add `/readyz`: pool query, schema gate result, listener state, job
  lease state. Liveness stays cheap.
- **Graceful shutdown.** `main.rs` calls `axum::serve(...)` with no shutdown signal
  handler, so SIGTERM terminates the process immediately. Add a drain period: stop
  accepting, finish in-flight writes, close SSE with a `retry:` hint. Required for
  rolling upgrades.
- **Pool and timeouts.** Make pool size, acquire timeout and statement timeout
  configurable (`DB_POOL_MAX`, `DB_ACQUIRE_TIMEOUT_MS`, `DB_STATEMENT_TIMEOUT_MS`),
  keeping today's values as defaults. Document `max_connections >= replicas x pool + 20`.
- **Client IP.** The API sees only the web tier's address; `TraceLayer` logs do not carry
  the end user's address. Forward a validated `X-Forwarded-For` from the proxy chain to
  audit rows where the customer wants it (privacy trade-off; default off).
- **CORS.** `CorsLayer` allows any origin, method and header
  (`lib.rs: app`). Authentication is bearer-header only, so this is not exploitable by
  ambient cookies, but a self-hosted API port should not be exposed to browsers at all;
  the web gateway is the only intended browser path. Make the allowed origins
  configurable and default to none.
- **Telemetry route.** In self-hosted mode `/telemetry` and the `web_metric` table are
  inactive (D6). Keep the table (schema is append-only history of migrations).

### 6.7 Load balancer requirements

| Requirement | Reason | Source |
|---|---|---|
| HTTP/1.1 upstream, `Connection ""`, response buffering off for `/api/engine/*` and `/documents/*/stream` | SSE | `nginx-app.conf` |
| Idle and read timeout at least 120 s (the app sends a heartbeat every 30 s; production uses `proxy_read_timeout 3600s`) | SSE through proxies | `stream.rs`, `nginx-app.conf` |
| Response header buffer of at least 16 KiB (`proxy_buffer_size 16k; proxy_buffers 8 16k; proxy_busy_buffers_size 32k`) | Preload `Link` headers plus rotated cookies exceeded the default 4 KiB and caused 502 | [WORKSPACE-502-FIX.md](WORKSPACE-502-FIX.md) |
| Request body limit 16 MiB | Outer limit in the API (`RequestBodyLimitLayer`) and `BODY_SIZE_LIMIT=16M` in the web image | `lib.rs`, `Dockerfile.web` |
| Health checks on `/healthz` (web and API); readiness on `/readyz` once it exists | Rotation | section 6.6 |
| Connection draining of at least 60 s on instance removal | SSE and in-flight writes | |
| Preserve the `Host` header and send `X-Forwarded-Proto: https` | `ORIGIN` check and cookie `secure` flag are derived from the request URL | `hooks.server.ts`, `session.ts` |
| No path rewriting; no caching of `/api/engine`, `/auth`, `/connectors/*` (these set `private, no-store`) | Session and pairing material | `security.ts` |

### 6.8 Test strategy for section 6

A four-process harness (two web, two API, one PostgreSQL, one client driver) runs in CI
and is the acceptance gate for Profile B/C:

1. Cross-replica stream delivery and resync (6.2).
2. Refresh race with 50 concurrent requests per cookie across replicas (6.4); zero
   spurious logouts.
3. Limits: 300 requests in one minute for one identity across two replicas produce the
   configured number of 429 responses (6.3).
4. Lease handover: kill the lease holder, assert the indexer resumes within 30 s and no
   document is fingerprinted twice concurrently (6.5).
5. Rolling upgrade across an expand-only migration (6.9) with a client continuously
   writing; zero failed writes beyond those cut by a draining replica and retried with
   idempotency keys.

### 6.9 Migration locking, the schema window and N-1

**Facts.**

- `engine-cli migrate` runs `MIGRATOR.run(&pool)`. sqlx's PostgreSQL migrator takes
  `pg_advisory_lock` with a key derived from the database name (CRC-32 times a constant)
  and applies each migration inside a transaction unless it opts out (sqlx source,
  checked 14 October 2026). Two operators or two Helm hooks running `migrate`
  concurrently serialise safely.
- The API never migrates and refuses to start if its embedded migration list differs
  from the applied list, in either direction (`check_schema`), or if the catalog's tables
  and privileges differ from its allowlist (`check_runtime`). The release records state
  the consequence for every release since schema 25: an older image cannot be pointed at
  a newer schema, and rollback means restoring the verified dump.
- Already-running old replicas do not re-check; they keep serving against the newer
  schema until restarted. A rolling update across a migration therefore either fails
  closed on restart of an old replica or runs old code against a new schema.

**Target policy (proposed).**

| Class | Definition | Allowed in | Old binary on new schema |
|---|---|---|---|
| `expand` | Adds tables, nullable columns, defaults, indexes, triggers, new functions; no drop, rename, retype, tightening, or privilege removal | Any release | Supported for exactly one release (N-1) |
| `contract` | Drops or tightens something that release N-1 no longer needs | Release N+2 or later, after N-1 leaves the support window | Not supported |
| `breaking` | Anything else (data rewrite that old readers misread, event variant change) | Only with release notes and a mandatory restore-point | Not supported; rollback = restore |

Mechanics:

1. `migrations/COMPAT.toml` (a side file; applied migrations are immutable) maps each
   migration version to its class and `min_api`.
2. `check_schema` changes from "lists equal" to: every migration the binary knows is
   applied with a matching checksum **and** every extra applied migration is
   classified `expand` and within `accepts_forward = <version>` compiled into the binary.
3. `check_runtime` compares the inventory only for objects created by migrations the
   binary knows; objects created by accepted forward migrations are checked for
   "no PUBLIC grant, no ownership by the runtime login" but are not required to be in
   the allowlist.
4. CI builds the previous release's API and runs its integration suite against a database
   migrated by the current release (matrix N-1 on N). A migration that breaks it must be
   reclassified `breaking`, which blocks "rolling upgrade supported" in the release notes.
5. `dynodoctl upgrade` reads the class of every pending migration and chooses: rolling
   (all pending are `expand`) or stop-the-world with a backup restore point.

Profile A keeps stop-the-world upgrades (about a minute of downtime in the recorded
production releases) and does not need the window. Profile B may use it. Profile C
requires it for any claim of "upgrade without downtime".

**Rollback consequence.** With the window, rollback to N-1 after an `expand`-only
upgrade is a binary change with no data restore. Without it, rollback remains restore.

---

## 7. Office and Google add-in distribution per customer

### 7.1 What exists

Six add-ins exist as test builds ([extensions/README.md](../extensions/README.md),
[INSTALL.md](../extensions/INSTALL.md)): Word, Excel and PowerPoint as Office web
add-ins (XML manifest, task pane at `https://<origin>/connectors/{word,excel,powerpoint}`,
ribbon group through `VersionOverrides`, command frame `commands.html`), and Docs,
Sheets and Slides as Google Apps Script editor add-ons. Installed-host acceptance has
not been completed for any of them (run plan:
[DESIGN-INSTALLED-HOST-ACCEPTANCE.md](DESIGN-INSTALLED-HOST-ACCEPTANCE.md)), PowerPoint 2021 lacks the required API set
(PowerPointApi 1.4), and no marketplace listing exists
([ADDIN-PUBLICATION-AND-ACCEPTANCE.md](ADDIN-PUBLICATION-AND-ACCEPTANCE.md)). A
self-hosted edition cannot be described as having working add-ins before that
acceptance exists; the add-in channel is an engineering dependency of Profile B and C
add-in claims, not a packaging detail.

### 7.2 The per-customer manifest generator (P0-7)

Every origin string is currently baked into static files: six lines per Office
manifest carry the app origin (`IconUrl`, `HighResolutionIconUrl`, `AppDomain`,
`SourceLocation`, the line with the three icon resources, and the line with `Commands.Url`
and `Pane.Url`), a seventh (`SupportUrl`) names the marketing site, and `DYNODOC_ORIGIN`
plus `urlFetchWhitelist` carry it per Google host.

Proposed:

1. A manifest route `GET /connectors/packages/{host}.xml` renders from templates using
   `ORIGIN`. Variables: `origin`, `support_url` (customer help desk), `display_suffix`
   (to tell staging from production), `addin_id` (GUID derived from the installation UUID
   and host so that two customers never share an add-in id), `version` (bundle version;
   Office needs it raised to replace a cached copy).
2. An offline equivalent: `dynodoctl addin build --origin https://docs.example.org --out
   ./addins` writes the three XML manifests, the catalog-compatible variants (7.4) and
   three Google ZIPs, plus a `SHA256SUMS` file.
3. The check script becomes a product feature, `dynodoctl verify-delivery`: today
   `check-connectors.py` compares served bytes to a source checkout, which a customer
   does not have. The product variant compares served bytes to the generated package
   manifest of the installed bundle.
4. Manifest validation (`office-addin-manifest validate`, currently run through
   `npx` in `package.json`) is vendored into the bundle build so it does not need the
   network.

### 7.3 Office.js hosting

Facts (Microsoft Learn, accessed 14 October 2026): the simplest reference is the CDN;
the CDN host moved to `officeapis.public.onecdn.static.microsoft` and the older
`appsforoffice.microsoft.com` endpoint is described as legacy (the Dynodoc panes still
load the legacy URL); an add-in published to Microsoft Marketplace must use the CDN
reference, and "local references" are for internal, development and debugging scenarios.
There is no official offline distribution; the `@microsoft/office-js` npm package is a
copy of the CDN file, is described as no longer officially supported, and its licence
terms are not stated clearly enough in public documentation to be relied on without
review.

| Option | Client needs | Pros | Cons |
|---|---|---|---|
| CDN reference | Internet access from the user's machine to `*.static.microsoft` (and legacy hosts) | Always current; Microsoft's recommended path | Not available air-gapped; many intranets block it; Microsoft sees a request per pane open |
| Local copy shipped in the bundle | None | Works air-gapped; no third-party request | Licence question (counsel); copy goes stale; must be refreshed with each bundle; file set must include sibling resources |
| Customer-supplied copy | None | Customer accepts Microsoft's terms directly; Verne ships nothing it may not redistribute | Customer effort; version skew; support burden |

Recommendation: ship the CDN reference as the default, support a configured local path
(`OFFICEJS_LOCAL_PATH`), and decide shipping a copy only after counsel confirms the
terms (open question Q7). CSP changes accordingly: with a local copy, `script-src` and
`connect-src` for the panes drop all three Microsoft hosts.

### 7.4 Distribution channels matrix

Facts below are from Microsoft Learn pages accessed 14 October 2026 (links in appendix C).

| Channel | Works for | Requires | Limits that matter to Dynodoc |
|---|---|---|---|
| Microsoft 365 admin center, Integrated apps (upload custom manifest) | Microsoft 365 tenants; Windows, Mac and Office on the web | Eligible licences, Entra ID, Exchange Online mailbox; admin role | Up to 24 hours to appear; users in nested groups are not assigned; manifest changes that raise permissions need admin consent again; pane origin must be reachable from users, not from Microsoft. Not for on-premises-only organisations. |
| Centralized Deployment | Sovereign and government clouds | Same family of requirements | Documented as the path where the integrated apps portal is unavailable. |
| SharePoint app catalog (on-premises or online) plus Trusted Add-in Catalog (GPO or manual) | On-premises Office 2016 to 2024 and SharePoint Server; Windows | App catalog site; GPO "Trusted Catalogs" or per-user setting | **No `VersionOverrides`, therefore no ribbon commands**: a catalog-compatible manifest without the ribbon group is required (the pane opens from Home > Add-ins > More Settings > My Organization). Not supported on Mac. Not supported with the unified manifest. |
| Network share catalog | Windows desktop | UNC path in Trust Center | Microsoft documents it as for development and testing, "not supported for production add-ins"; usable at the customer's own risk for Profile A. Not for iPad, Mac or web. |
| Sideload (upload in Office on the web; `wef` folder on Mac) | Individuals and pilots | Per-user action | Per user; not manageable. |
| Microsoft Marketplace (AppSource) | Public | Partner Center, certification | Manifest origin is fixed by the publisher; a store listing cannot point to each customer's own origin. Not usable for self-hosted installations. |

Unified manifest: Microsoft documents it as unsupported for perpetual-licence Office on
Windows, and SharePoint catalogs do not support it. The add-in-only XML manifest remains
the target for self-hosted installations; a unified manifest for Microsoft 365 cloud
tenants is a later optimisation.

TLS: panes load over HTTPS. The certificate must chain to a root the Office host trusts:
the OS store for desktop Office, the browser store for Office on the web. A private CA
must be distributed to endpoints through the customer's usual mechanism (GPO, MDM).
Self-signed certificates fail.

### 7.5 Google editors

Apps Script `UrlFetchApp` runs on Google's servers, so every add-on request to the
Dynodoc origin originates from Google's network, not from the user's machine. A Dynodoc
origin that is reachable only on an intranet cannot be called. Requests come from a
published pool of Google IP ranges, which allows an allowlist on a public-facing
reverse proxy, but not a private address (checked 14 October 2026 against the Apps
Script reference and community reports).

| Customer situation | Google add-ons |
|---|---|
| Public HTTPS origin (Profile A/B with ingress) | Supported in principle through a per-customer package (`DYNODOC_ORIGIN`, `urlFetchWhitelist`) installed by a Workspace administrator as a private Marketplace listing, or per user as a test deployment |
| Intranet-only origin | Not possible |
| Air-gapped | Not possible; Google editors themselves need Google's cloud |

Data residency caveat: the add-on stores its capture queue and the file-scoped key in
Apps Script `UserProperties` (Google-hosted)
([addin-data-handling.md](listings/addin-data-handling.md)), so that data is outside the
customer's boundary even when the Dynodoc server is inside it. Profile C should treat
the Google add-ons as unavailable.

Per-customer Apps Script packaging is a generator extension of 7.2 (replace the origin,
whitelist and `INSTALL.md` text). Private Marketplace listing needs a Google Cloud
project, Marketplace SDK configuration and possibly OAuth verification for the scopes
used; whether the scopes (`documents.currentonly`, `script.container.ui`,
`script.external_request`) require verification is **unverified; check at build time**.

### 7.6 What is impossible air-gapped

| Capability | Why |
|---|---|
| Google Docs, Sheets and Slides add-ons | Google cloud on both ends |
| Google sign-in, Google Forms import | Google endpoints |
| Office on the web | Microsoft cloud |
| Microsoft Marketplace installation, centrally deployed from the Microsoft 365 admin center | Microsoft cloud tenant |
| CDN-delivered Office.js | No route to Microsoft; use a local copy |
| ACME certificates from a public CA | Use an internal CA |
| Resend mail | Use an internal relay or no mail |
| Public research servers | No route (private ones need the allowlist, Q14) |
| Cloud-hosted AI clients (hosted connectors, apps) | They must reach the installation from the vendor's cloud; only local stdio clients with a locally run model work ([AI-AGENT-VENDOR-INTEGRATIONS.md](AI-AGENT-VENDOR-INTEGRATIONS.md), section 13) |

What does work air-gapped: the web workspace and basic editors, Projects, review and
merge, Office desktop add-ins through SharePoint catalog, GPO or network share, the
CLI with local repositories and `sync` using device keys, the converter, signed bundles
and offline licence files.

### 7.7 Versioning and compatibility

The gateway rejects document API calls without `x-dynodoc-editor: office-v5`
(`routes/api/engine/[...path]/+server.ts`), coupling the web build to the API build; the
add-in panes are part of the web build, so they update with it. Manifests change only
for icon, command or permission changes; each change needs a higher `Version`, and a
permission change needs administrator consent again in the admin center. The bundle
release notes must say when manifests changed.

### 7.8 Acceptance

For each channel the acceptance record states: Office build and platform, manifest
variant, trust path for the certificate, whether the ribbon group appears, a
connect-capture-push-approve-apply scenario on a disposable file, revocation test (grant
expiry is seven days; revoke under Sync & connections). `ACCEPTANCE.md` in
`extensions/` is the worksheet.

---

## 8. Receipt authority and device keys when the customer holds the keys

Rotation and recovery of the receipt authority are designed separately
([REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md): an append-only epoch chain,
schema 41, not deployed) and are not designed here. This section defines custody, distribution and the interaction with
self-hosting, and states what must be true before an installation enables the authority.

### 8.1 Key and secret inventory

| Material | Purpose | Where it lives today | Rotation today | Holder in self-hosted edition |
|---|---|---|---|---|
| `PASETO_LOCAL_KEY` (32 bytes) | Encrypts and authenticates access tokens, project device-session tokens; MAC key derivation for device challenges | API environment | None; rotation ends all sessions | Customer secret store |
| `WEB_SERVICE_KEY` (at least 32 chars) | Web to API issuance and telemetry credential | API and web environments | None | Customer |
| `SESSION_SECRET` (at least 32 chars) | Encrypts web session and provider-state cookies | Web environment | None; rotation logs everyone out | Customer |
| `CONVERTER_KEY` | Web to converter | Web and converter environments | None | Customer |
| PostgreSQL owner and runtime credentials | Migration/operator and API logins | `.env`; role created manually | Rotate by new role plus `grant-runtime` (done once, 11 October 2026) | Customer |
| Receipt authority private key (Ed25519, PKCS#8 DER) | Signs branch-head receipts | File named by `DYNODOC_REF_SIGNING_KEY_FILE`, mounted read-only (not installed in production) | None; public key binding is immutable in `project_ref_authority` | Customer; never Verne |
| User device keys (Ed25519) | Sign commits; obtain project sessions | User machines; unencrypted DER files | Revoke and re-enrol | End users |
| Licence verification keys (proposed) | Verify licence files | Compiled into web build | Add key ids over releases | Verne issues; public keys only in bundle |
| Bundle and image signing keys (proposed) | Authenticate Verne artefacts | Verne release infrastructure | Planned overlap rotation | Verne; customers hold public keys |

### 8.2 What the receipt authority means in a customer installation

A receipt binds an authority UUID (the installation's `device_authority` row), project,
branch, commit address, generation, previous receipt and a declared time, signed by the
authority key ([REF-RECEIPTS.md](REF-RECEIPTS.md)). Clients pin the authority key through
an independently trusted channel and then verify contiguous proofs. In a self-hosted
installation:

- The customer is the authority. Verne cannot vouch for, recover or rotate the key, and
  must not be able to. The "independent channel" for a pin is the customer's own
  (onboarding page, configuration management, MDM profile), never the server's
  `GET /authority` response.
- A user who works with two installations keeps two pins; `engine-cli receipt` scopes
  journals by `(authority, project, branch)`, so they do not interfere.
- The installation UUID is created by migration 0033 and travels with the database. A
  restore for disaster recovery intentionally keeps the same authority. A staging or
  test copy made from a production backup is **the same authority**: it can sign
  receipts the production pins accept if it is given the key. Staging copies must not
  be given the production key, and should be re-identified (new installation identity)
  when that capability exists; until then the runbook says to remove the key mount and
  never enable shared writers on a copy.

### 8.3 Generation, custody and loss

Because the public key binding is immutable and rotation does not exist yet
([RECEIPT-AUTHORITY-DECISION.md](RECEIPT-AUTHORITY-DECISION.md); the epoch-chain design in
[REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md) would change this once it is
deployed), the rule for this
edition is: **ship with the authority not installed, exactly as production does, and
document enabling as a deliberate procedure with an escrow checklist.** Per profile:

| Profile | Generation | Storage | Escrow | If lost today |
|---|---|---|---|---|
| A | `openssl genpkey -algorithm ED25519 -outform DER` on the admin workstation, never on the server's backup path | File, mode 0400, uid 10001, mounted as a secret | Two offline copies (encrypted USB, password manager) held by two named people | New authority; every client must re-pin; prior receipts remain verifiable against the old key |
| B | Same, on a hardened workstation; ceremony recorded | Secret store (Vault, Kubernetes secret with envelope encryption) | Split with a threshold scheme to three custodians (optional) | Same |
| C | In the HSM/KMS (non-exportable) | KMS key reference; no file | HSM/KMS backup and quorum policies per the customer's key management policy | Same, plus HSM recovery procedure |

Once the epoch chain in [REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md) is deployed,
planned rotation with a dual-signed statement becomes available, and loss or compromise
becomes a recorded *recovery* epoch that every client must re-pin out of band. That is
still a visible break, not a transparent fix, which is why the default stays "not
installed" and why escrow remains part of the checklist.

Enabling checklist (carried over from the decision record, restated): named custodian,
two offline copies not on the database backup path, rehearsal on a production copy with
the key mounted read-only, written decision on client pin distribution, an accepted
statement that loss means a new authority (or rotation exists), and an install via
`engine-cli install-ref-authority PUBLIC_HEX` using the operator login only.

### 8.4 Signer abstraction for KMS and HSM (P2-10)

`AppState.ref_signer` is `Option<Arc<ring::signature::Ed25519KeyPair>>` and the key is
loaded from a PKCS#8 file (`server_authority::load_key`, 4,096-byte cap). Receipt
signing happens synchronously inside the ref-update transaction
(`project_history.rs`, around the `SignedReceipt::sign` call), together with the ref row
and audit row. A key held in a KMS therefore needs:

```text
trait RefSigner: Send + Sync {
    fn public_key(&self) -> [u8; 32];
    async fn sign(&self, message: &[u8]) -> Result<[u8; 64], SignError>;  // timeout 2 s
}
implementations (proposed): FileSigner (today), Pkcs11Signer, KmsSigner, ExternalSigner (UDS/HTTP)
```

- Startup check: `signer.public_key()` equals the installed binding (as today for the file).
- Failure mode: the signer call fails or times out; the ref update returns a retryable
  error and no unsigned fallback exists (unchanged invariant: "no unsigned ref fallback").
- Lock time: the project advisory lock and row locks are held for the signer round trip.
  Acceptable for a few tens of milliseconds; the harness (P2-11) records p99.
- Ed25519 availability differs by product. AWS KMS added `ECC_NIST_EDWARDS25519` for
  signing in November 2025; Google Cloud KMS lists `EC_SIGN_ED25519`; HashiCorp Vault's
  transit engine is reported to support an `ed25519` key type (**unverified; check at
  build time**); PKCS#11 `CKM_EDDSA` support varies by HSM vendor and firmware and must
  be confirmed per device. Where a device cannot do Ed25519, the `ExternalSigner` hook
  lets a customer front it with their own service.

### 8.5 Device keys

Device keys are the users' keys and are unaffected by who hosts the server, with these
self-hosted consequences:

- Enrolment binds a key to an account and to the installation UUID (challenge covers
  installation, account, key, nonce); a key enrolled in one installation is not valid in
  another. Keys have global uniqueness within an installation only.
- Keys are unencrypted files with no OS keystore support (stated limitation, schema 40
  record). Profile C should require full-disk encryption on endpoints and treat key
  files as credentials under their endpoint policy. Hardware-backed or OS-keystore
  storage is open work ([DEVICE-KEY-GRANTS-API.md](DEVICE-KEY-GRANTS-API.md)).
- Offboarding must revoke keys: SCIM deactivation and the administrator "revoke
  sessions" control already revoke keys because a key can mint a session without a login
  (device sessions record). The identity document makes this part of the deactivation
  workflow.
- Service accounts for automation use device keys with per-project grants; there are no
  personal access tokens by design.

### 8.6 What the customer-held-key model does not provide

- Verne cannot recover a lost key or password and cannot see content. This is a
  feature, and also a support boundary customers must accept in writing.
- Customer-held server keys do not make content end-to-end encrypted: the server
  process holds `PASETO_LOCAL_KEY` and the database holds plaintext document state.
- Receipts and signatures do not prove authorship or honesty of review logic; a
  compromised API plus its signing key can sign forks until independent witnesses
  exist ([REF-RECEIPTS.md](REF-RECEIPTS.md), closing paragraphs).

---

## 9. Sizing model and its measurement gaps

**We have no capacity numbers.** The production pilot holds two documents, 36 events and
nine identities; the largest database dump recorded is 290,618 bytes
([schema 40 release](operations/2026-10-13-schema40-release.md)). The only performance
evidence in the repository is a set of single-run, single-machine fixtures
([benchmarks](benchmarks/)) that the documents themselves label as "not a capacity
claim". [PRODUCT-SPEC.md](PRODUCT-SPEC.md) states that hundreds of MB per file, ten
collaborators per large document and hundreds of thousands of registered users are
target workloads, "not measured capacity today". This section therefore gives a model
whose structure is defensible and whose coefficients are explicitly one of three kinds:
**M** (measured once on one machine), **L** (a limit that exists in code or
configuration) or **A** (an assumption that must be replaced by measurement).

### 9.1 Workload descriptors

| Symbol | Meaning | Notes |
|---|---|---|
| U | Registered identities | Rows in `identity` |
| U_w | Identities active in a week | Sign-in or edit |
| U_c | Peak concurrent active sessions | Open editors plus open streams |
| D | Documents (not in trash) | Includes project files |
| E_d | Events per document per year | Event = one typed semantic operation |
| s | Mean materialized state size (JSON bytes) | Text-heavy documents; spreadsheets differ |
| N_s | Snapshots per document per year | Policy is best-effort per 500 events plus on deploy ([ENGINE-EVOLUTION-PLAN.md](ENGINE-EVOLUTION-PLAN.md), algorithm work) |
| O | Mean original upload size | Application cap is 3,000,000 bytes (`file-limits`); schema check is 20 MiB |
| u_o | Uploads (originals plus replacements) per document per year | |
| R_t | Token rotations per active person per active day | about 24 h / TTL while in use; with a one-hour TTL, up to 8 over a working day |
| W | Write rate at the busiest minute (events per second) | Drives lock and NOTIFY contention |

### 9.2 Storage model

```
DB_bytes(year) ~= D * ( E_d * c_event
                      + N_s * c_snap * s
                      + u_o * O )
                  + U_w * days_active * R_t * c_refresh      (if never pruned)
                  + D * c_finger
                  + overhead
```

| Coefficient | Value | Kind | Provenance |
|---|---|---|---|
| c_event (bytes per event including indexes) | about 1,550 | M | `event` relation 12.4 MB (table plus indexes) at revision 8,000 for the 8,000-block fixture ([shared-checkpoint-pg-20261010.json](benchmarks/shared-checkpoint-pg-20261010.json)); real documents with rich-text patches may differ in both directions |
| c_snap (fraction of state JSON stored per legacy snapshot) | about 0.45 | M | `snapshot` relation 2.9 MB for a 6.4 MB-JSON state after six checkpoints in the same fixture; repetitive synthetic text compresses unusually well, so real documents are worse |
| Shared checkpoint objects | 11.2 MB after six checkpoints for the same document (7.7 MB unique canonical bytes, 9,909 objects) | M | Same file. Production keeps shared writers off, so this is not in the default footprint |
| c_refresh (bytes per refresh-token row including indexes) | about 200 | A | Not measured |
| c_finger (copy-detection rows per document) | 5 to 20 KB | A | `document_fingerprint`, `_band`, `document_block_hash`; not measured |
| WAL multiplier on write bursts | the same fixture wrote 482 KB of WAL per legacy snapshot write and 11.8 MB for the first shared publication | M | Same file |
| Per-document and per-account encoded-object budgets | 1 GiB / 250,000 objects and 4 GiB / 1,000,000 | L | [SHARED-STORAGE-BUDGETS.md](SHARED-STORAGE-BUDGETS.md) |
| Table and index overhead, bloat, WAL retention | 1.3 to 2 times | A | Typical PostgreSQL planning range; not measured here |

Backups multiply the footprint: the existing `backup.sh` keeps 14 daily
custom-format dumps; each dump contains everything including originals. Retention cost is
approximately `retention_days x dump_size`, which is why daily full dumps stop being a
sensible strategy somewhere in Profile B (section 9.4).

### 9.3 Compute and memory model

What bounds work per API process today (all **L**):

| Resource | Limit | Source |
|---|---|---|
| Concurrent response bodies | 64 | `workloads.rs` |
| Concurrent SSE streams | 256, plus 100 subscribers per document | `workloads.rs`, `stream.rs` |
| Concurrent expensive reads (search, audit, snapshot, proof, checkpoint) | 8 | `workloads.rs` |
| Per identity | 120 expensive reads per minute, 2 active | `workloads.rs` |
| Per project (or unprojected document) | 600 per minute, 4 active | `workloads.rs` |
| Database connections per process | 8 | `main.rs` |
| Statement / lock / idle-in-transaction timeouts | 15 s / 5 s / 30 s | `main.rs` |
| Request body | 16 MiB outer, 3,000,000 bytes per original | `lib.rs`, `file-limits` |
| Web container (production) | 2 CPU, 1 GiB | `compose.yml` |
| API container (production) | 2 CPU, 2 GiB, 128 PIDs | `compose.yml` |
| Converter | 2 conversion slots, 45 s timeout, 1 CPU, 768 MiB | `infra/converter/server.py`, `compose.yml` |

What drives cost per request (qualitative, from the code and the evolution plan):
reconstruction of a document's full state is resident in memory for reads and for the
snapshot verification path; shared writes scan full state; search is account-scoped and
limited to 50 rows but not indexed ("LIMIT 50 bounds returned search rows, not scanned
nodes", [BROWSER-AND-WORKLOAD-SECURITY.md](BROWSER-AND-WORKLOAD-SECURITY.md)); the
copy indexer fingerprints any document changed more than 15 seconds ago. The one memory
measurement is a peak working set of 106 MB (Windows counter, including client buffers
and several reconstructed states) for the 8,000-block fixture. Arithmetic, not a
measurement: eight concurrent expensive reads each holding a 100 MB-class state is about
800 MB, consistent with the 2 GiB container limit but unproven for larger documents.

### 9.4 Illustrative results

The full arithmetic (inputs, per-term results, hardware starting points per profile and
leading indicators) is in the [deployment guide, section 6](SELF-HOSTED-DEPLOYMENT-GUIDE.md).
Summary of that arithmetic, from the assumptions stated there and not a capacity claim:

| | A: lab | B: department | C: enterprise |
|---|---|---|---|
| Growth per year before overhead | about 0.55 GB | about 33 GB | about 680 GB |
| With 1.3 to 2 times overhead | 0.7 to 1.1 GB | 43 to 66 GB | 0.9 to 1.4 TB |
| Dominant term | Originals; all small | Originals, then events | Originals in PostgreSQL, then events and snapshots |
| Starting hardware (provisional) | 1 VM, 4 vCPU, 8 GB | 3 to 4 nodes; primary 4 to 8 vCPU, 16 to 32 GB | 6+ nodes; primary 8+ vCPU, 32 to 64 GB |

Three conclusions follow from the model rather than from measurement: Profile A is
never storage-bound; at Profile B daily full dumps stop being a sensible backup method
(retained dumps reach hundreds of GB), which motivates physical backup with WAL
archiving (P2-9); at Profile C originals stored in `bytea` columns dominate dump,
restore and replica-rebuild time, which is the strongest reason to decide the
object-storage backend early (P2-12).

### 9.5 Measurement gaps and the plan to close them (P2-11)

| Gap | Why it matters | Experiment |
|---|---|---|
| U1: events per second one API process sustains per document and in total | Defines replica count | Write-rate sweep, 1 to 100 concurrent writers, hot and cold documents |
| U2: CPU and memory per reconstruction by document size (1, 6, 50, 100 MB-class) | Memory limits, expensive-read pool size | Reconstruction and snapshot-verification benchmark; peak RSS of the process, not of the host |
| U3: latency of review, merge and compare on large documents | UX acceptability | Scripted review of a 6 MB-JSON document |
| U4: live stream behaviour with 1,000 and 10,000 streams; reconnect storm after a proxy restart | Fan-out design | Harness with synthetic SSE clients |
| U5: PostgreSQL write amplification and WAL volume per event | Backup and replica sizing | Counters during U1 |
| U6: search cost by corpus size | Whether indexed search is needed | Query benchmark at 10k, 100k, 1M nodes |
| U7: copy-detection indexer CPU and lock impact | Whether it must be off by default at scale | Index 10k documents; measure editing latency alongside |
| U8: backup, restore and replica-rebuild times by database size (10, 100, 500 GB) | Backup strategy per profile | Timed drills |
| U9: converter throughput and memory | Sizing the optional converter | 100 `.doc` files of varied sizes |
| U10: NOTIFY contention knee (6.2) | Fan-out mode default | Writers with and without the trigger |
| U11: web gateway memory with 3 MB uploads and 16 MB bodies | Web replica size | Parallel uploads |
| U12: token and refresh-row growth at realistic TTLs | Retention defaults | Simulated login population |

Report template (from the product specification): concurrent active users, documents,
edits per second, object sizes, history length, cache state, p50/p95/p99 latency, peak
memory, storage and network amplification, deployment hardware and cost per workload.
A registration count or microbenchmark is not a capacity statement.

### 9.6 Known scaling smells

1. Originals in `document_upload.content` (PostgreSQL `bytea`): inflate dumps, replicas
   and restore times; included in every backup. The harness and gates for 100 to 500 MB
   files are designed in [DESIGN-LARGE-FILE-ACCEPTANCE.md](DESIGN-LARGE-FILE-ACCEPTANCE.md).
2. Full-state materialization and verification per document.
3. Unindexed search bounded only by `LIMIT 50` and a 2 s timeout.
4. Unpruned authentication rows.
5. Per-account serialisation of shared-object accounting ("a pilot correctness
   boundary, not a high-scale throughput claim", SHARED-STORAGE-BUDGETS.md).
6. Process-local admission and a database-wide lock in NOTIFY commits.

---

## 10. Data residency and regulatory mapping

This section maps where data lives and which regulatory questions customers are likely
to ask. **It is not legal advice and makes no claim that the product or any deployment
complies with any law.** Each row names a mechanism that exists or is proposed and the
gap that remains. Verne Systems' legal counsel must review it before it is used in a
customer-facing document (open question Q18).

### 10.1 Roles in a self-hosted installation

| Party | Role | Condition |
|---|---|---|
| Customer | Controller and operator of the installation | Always |
| Verne | Software supplier; not a controller or processor of installation data | Only while Verne has no access to the installation or its data |
| Verne | Processor (or sub-processor) | If Verne staff access content or personal data during support, remote administration or a hosted-by-Verne arrangement; a data processing agreement and transfer analysis then apply |
| Identity provider, mail relay, backup storage, KMS | Customer's processors | Chosen by the customer |
| Microsoft, Google | Independent providers of the editors the add-ins run in | Not Dynodoc subprocessors (`routes/(site)/subprocessors/+page.svelte`) |

### 10.2 Data inventory

| Data class | Location in the installation | Personal data | Leaves the customer boundary by default | Erasure and retention mechanism |
|---|---|---|---|---|
| Document content and history | `event`, `snapshot`, `document_version`, `draft_edit`, shared objects (`checkpoint_object`) | Often | No | Whole-document audited erasure (`/admin/erase`); append-only guards otherwise; backups retain until expiry |
| Original files | `document_upload.content` | Often | No | Same |
| Identity records | `identity` (email, display name, bio up to 280 characters, status) | Yes | No | Disable, then audited account erasure; authorship retained pseudonymously ("contributions_retained") |
| Sign-in material | `magic_link`, `refresh_token` (hashes), `connector_grant` (hash), `device_signing_key` (public keys), `device_session` | Pseudonymous | No | Revocation; pruning job (P0-8) |
| Access control | `access_space`, `space_member`, `document_access`, project settings | Yes (who has access) | No | Changed by administrators; audit rows retained |
| Operational audit | `operation_audit`, `erasure_receipt` | Yes (actor ids, targets) | No | Retained by design; erasure receipts commit to before/after digests without readable copies |
| Notifications, issues, comments | `notification`, `project_issue*` | Yes | No | Immutable comments; erasure rules per [PROJECT-ISSUES-API.md](PROJECT-ISSUES-API.md) |
| Copy-detection derivatives | `document_fingerprint*`, `document_block_hash`, `document_match` | Derived from content | No | Deleted with the document; optional feature off switch (6.5) |
| Anonymous page metrics | `web_metric` | No (random page id, page class, timing) | No (hosted mode sends to the operator's own endpoint; none in self-hosted) | Pruned at 30 days |
| Server logs | Proxy access logs (client IP, redacted private-link paths), API logs | IP addresses | No | Customer log retention |
| Backups | Dump files, WAL archive | Everything | Wherever the customer puts them | Customer retention; `acknowledge_backups` is required for admin erasure |
| Add-in local state | Office web view IndexedDB `dynodoc-provenance` (origin-scoped) on user devices; Apps Script `UserProperties` for Google add-ons | Content excerpts and observed edits | Google add-on data is stored by Google | User clears storage; Google data not under customer control |
| Public view links | `public_document_view` (token hash and a frozen team-version copy) | Possibly | Reachable by anyone holding the link | Expiry 7/30/90 days or revocation; a downloaded copy cannot be recalled |
| Email in transit | SMTP relay | Recipient address, link, item name | To the customer's relay | Relay policy |
| Support bundle (proposed) | Local file produced on request | Redacted configuration and counts | Only if the customer sends it | Customer decision |

### 10.3 GDPR (EU and UK) mapping

Authoritative text: Regulation (EU) 2016/679 (EUR-Lex). Article numbers below were
confirmed as standard references; none of the interpretations is a legal conclusion.

| Provision | Topic | What the product offers | Gap or customer duty |
|---|---|---|---|
| Art. 5(1)(c), (e) | Minimisation, storage limitation | Profiles exclude email; names and bios are visible only among project members; telemetry absent; retention job (proposed) | Customer defines retention for content; history is retained by design |
| Art. 5(1)(f), 32 | Integrity, confidentiality, security of processing | Hash-chained history and snapshot verification; restricted database role; encrypted cookies; TLS; no cleartext secrets in the database | Encryption at rest is the customer's disk or storage layer; no application-level encryption at rest; not end-to-end |
| Art. 15, 20 | Access and portability | Native download of supported files and history export per document; CLI export of project history | A per-person subject-access export across all documents is not built (proposed small tool) |
| Art. 16 | Rectification | Profile editing; document edits | History preserves prior values by design |
| Art. 17 | Erasure | Audited whole-document and account erasure with impact preview and blockers | Backups and exported copies persist until expiry; chain integrity means erasure is whole-resource, not field-level; the product does not promise instant erasure from backups |
| Art. 25 | Data protection by design and default | Private by default; project-scoped sessions; per-project key grants default to "granted" | Customers must configure retention and access policy |
| Art. 28 | Processor terms | None needed when Verne has no access | A DPA template for support access cases (P4-6) |
| Art. 30 | Records of processing | Data inventory (10.2) is a starting point | Customer's own record |
| Art. 33, 34 | Breach notification | Audit rows and logs support investigation | Customer process; Verne's duty is to tell customers of vulnerabilities (CRA, 10.5) |
| Art. 35 | DPIA | Threat model (section 11) is an input | Customer-owned |
| Ch. V, Art. 44 to 49 | International transfers | No transfer by default; nothing flows to Verne | Support access from outside the customer's jurisdiction is a transfer; Verne is in Bangladesh, and an EU adequacy decision for Bangladesh is not known to us (**unverified; check at build time**), so standard contractual clauses or on-site-only support would be needed |

### 10.4 FERPA-style academic records mapping

FERPA is a US statute (20 U.S.C. 1232g; 34 CFR Part 99). It is used here as a model for
academic-record regimes; other countries have analogues. Student work, grades and
feedback held in documents can be education records.

| Provision (34 CFR) | Topic | What the product offers | Gap or institution duty |
|---|---|---|---|
| 99.31(a)(1)(i)(B) | Outsourcing to a party under the institution's direct control as a "school official" | Self-hosting keeps records on institution infrastructure under its control; no Verne access by default | Institution's annual notice criteria and contract terms; the Department of Education guidance stresses direct control and written agreements |
| 99.10, 99.20 | Inspect, review and seek amendment | Per-document export and history; review workflow | Cross-document student export tool not built |
| 99.32 | Record of disclosures | Operational audit of administrative actions; project activity; sharing changes | Not a purpose-built disclosure log; needs a configured report (proposed) |
| 99.33 | Redisclosure limits | Role and project-level access; public links are explicit and expiring | Institution policy for public links; ability to disable them (proposed setting) |
| 99.37 | Directory information | Profile fields (name, bio) are visible only to shared project members | Institution must configure which fields exist |
| 99.31(a)(6) | Studies by or for the institution | Not applicable by default | Institution analysis |
| Retention | Records schedules | Retention is customer-defined | Semester-end archive and offboarding flows (identity document) |

### 10.5 Export-control and sanctions considerations

| Topic | Consideration | Action |
|---|---|---|
| US Export Administration Regulations (EAR) | The product includes cryptographic functionality (TLS, Ed25519, SHA-256, authenticated encryption in session cookies and tokens). Classification (Category 5, Part 2, mass-market, or not subject to the EAR for publicly available open-source code) depends on facts: US-origin content, how the encryption is used, and whether source code is publicly available. BIS guidance lists publicly available encryption source code as not subject to the EAR, subject to a notification requirement that sources disagree about after 2016 changes (**check the current 15 CFR 742.15(b) text**). | Counsel classifies the proprietary bundle and the MIT engine before first export; record the result |
| EU dual-use Regulation (EU) 2021/821 | Category 5 Part 2 (information security) for customers or reshipments inside the EU | Counsel review |
| Sanctions and embargoes | Licence files are issued to named customers; issuing to a sanctioned party is a transaction | Screening step in the licence issuance procedure (licensing document, 6.7) |
| Customer-side export of technical data | A customer's own documents may contain export-controlled technical data; self-hosting keeps that inside its boundary, whereas support access by Verne staff in another country could be an export | No Verne access by default; support bundles redacted and reviewed by the customer before sending |
| Bangladesh as the supplier's jurisdiction | Bangladeshi export and data rules for software and cryptography are not analysed here | Counsel review (**unverified**) |
| EU Cyber Resilience Act | Manufacturers of products with digital elements placed on the EU market must report actively exploited vulnerabilities and severe incidents from 11 September 2026; most remaining obligations apply from 11 December 2027 (Commission guidance, July 2026) | Counsel determines whether Verne is a manufacturer, importer or open-source steward for each component; PSIRT process (P4-2) |

### 10.6 What the product will not claim

No claim of GDPR, FERPA, HIPAA, SOC 2, ISO 27001, FedRAMP or similar compliance or
certification. No claim that erasure removes data from backups. No claim of
data-residency guarantees for the Google add-ons. No claim about export classification
until counsel has produced one.

---

## 11. Security and privacy analysis

### 11.1 Assets

| ID | Asset | Why it matters |
|---|---|---|
| AS1 | Document content, history and originals | Confidentiality and integrity of customer work |
| AS2 | Hash-chain and snapshot integrity | Provenance claims depend on it |
| AS3 | Identity and access state | Authorisation decisions |
| AS4 | Secrets: Paseto key, service key, session secret, DB credentials, authority key | Compromise equals impersonation or forgery |
| AS5 | Availability of the service and the live channel | Work stoppage |
| AS6 | Add-in channel (manifests, panes) | Code that runs inside Office |
| AS7 | Update and licence channel | Supply-chain integrity |
| AS8 | Device keys on endpoints | Commit signing and project sessions |

### 11.2 Actors

External attacker (network); malicious or compromised authenticated user; compromised
endpoint (stolen device key or token); malicious or compromised identity provider or
mail relay; customer system or database administrator; Verne personnel; compromised
Verne build or release infrastructure; other software on the same host.

### 11.3 STRIDE table

| Threat | Example | Existing controls | Added by this design | Residual |
|---|---|---|---|---|
| Spoofing: forged tokens | Forge a PASETO | v4.local AEAD; DB check per request; implicit assertions separate device tokens | Key ring for rotation (P2-10) | Single symmetric key compromise = full impersonation; rotate on suspicion |
| Spoofing: account takeover via email | IdP email reassigned or unverified; magic link intercepted | Single-use 15-minute links, scanner-safe POST; provider email verified check | `(issuer, subject)` link table, email-change policy, option to disable email links (identity doc) | Mail relay compromise when email sign-in is enabled |
| Spoofing: rogue SSO | Forged SAML assertion, signature wrapping | None (no SAML yet) | Vetted library, signed assertions required, replay cache, audience/destination checks, pen test | Library vulnerabilities |
| Tampering: history rewrite by a DBA | Update events table | Append-only triggers; restricted runtime role; chain verification CLI | Optional external anchoring of heads through witnesses ([DESIGN-WITNESSES-AND-TRANSPARENCY.md](DESIGN-WITNESSES-AND-TRANSPARENCY.md), design only) | A database owner can disable guards; undetected without an independent anchor |
| Tampering: malicious update | Trojaned bundle | None (builds on host) | Signed artefacts, SBOM, offline verification, digest pinning (P3-1, P3-2) | Compromise of the signing key; mitigated by overlap rotation and customer pinning |
| Tampering: manifest or pane substitution | Attacker serves a different `commands.html` | HTTPS; CSP; byte-equality delivery check | Manifest generated per origin, `verify-delivery` | Rogue internal CA |
| Repudiation | A user denies an approval | Operational audit rows; signed commits and receipts for project history | SIEM export (P2-7) | Declared time is not trusted time; no external timestamp |
| Information disclosure: cross-document leakage | Search across accounts | Role checks per request; account-scoped search; profile email omitted | None new | Application logic is trusted; no row-level security in SQL |
| Information disclosure: secrets in logs and backups | Tokens in access logs | Safe log format (`[private-link]`), no Authorization logging | Support-bundle redaction; `_FILE` secrets | Customer log pipelines |
| Information disclosure: client-side | Linked remote images in documents | CSP | `img-src` tightening in air-gap mode | Third-party images in connected mode |
| Information disclosure: add-in data at rest | IndexedDB ledger on a shared PC | Per-origin; key not stored in pane | Document; recommend profile policy | Endpoint security |
| Denial of service | Slow readers, expensive reads, upload floods | Admission limits, deadlines, container limits, Nginx rate limits | Shared windows (6.3), job lease, readiness | Limits are process-local until 6.3 lands; no capacity proof |
| Denial of service: NOTIFY queue | Listener stalls and queue fills | N/A today | Dedicated listener without transactions, monitoring, poll fallback | A stuck session can still cause commit failures |
| Elevation of privilege: admin allowlist | Email match | `ADMIN_EMAILS` exact match | Role model replacing the allowlist (P1-3); separation of operator and auditor | Misconfigured group mapping |
| Elevation of privilege: group sync | IdP admin adds a user to a mapped group | N/A | Mapping rules with explicit targets, ceilings (never above `manager` for synced groups unless configured), audit rows | IdP is trusted for the mapped scope |
| Elevation of privilege: stolen device key | Copied key file | Per-project grants, scoped one-hour sessions, revocation | OS keystore (open) | Unencrypted key files |

### 11.4 Privacy analysis

- Collection is minimal and mostly functional. The one cross-user disclosure that is
  surprising is copy detection: when an uploaded file resembles another person's
  document, the owner of the original is told who uploaded what and how similar it is
  ([`copies.rs`](../crates/engine-api/src/copies.rs) module comment). Customers must be
  able to switch it off (6.5) and the UI already tells the uploader beforehand.
- Profiles are visible only among members of shared projects, and responses exclude
  email (decision 013 workspace record).
- No analytics leave the installation. Anonymous page metrics are inactive in
  self-hosted mode.
- Support access is explicit and customer-initiated.

### 11.5 Residual risk list

R-S1 A database owner can rewrite history and the chain is only checkable against an
independently retained commitment. R-S2 Single symmetric Paseto key. R-S3 Unencrypted
device key files. R-S4 No application-level encryption at rest and no end-to-end
encryption. R-S5 Process-local admission until 6.3. R-S6 Trust in IdP group claims once
mapping exists. R-S7 A malicious or mistaken administrator can use audited erasure.
R-S8 Office add-in panes are test builds without installed-host acceptance.

### 11.6 Explicitly not claimed

Compliance or certification of any kind; FIPS 140 validated cryptography (the engine
uses `ring`; whether a validated module is needed is a Phase 4 investigation); resistance
to a compromised database host; protection from a compromised endpoint; formal
verification of merge or access logic; capacity or availability figures.

---

## 12. Upgrade policy and rehearsal tooling

### 12.1 Policy (proposed)

| Item | Policy |
|---|---|
| Release train | Stable releases with semantic versions `MAJOR.MINOR.PATCH`; security fixes as patch releases. Target cadence: quarterly minor releases (assumption; depends on team size). |
| Support window | The current minor and the previous minor receive security fixes for 12 months after the release of the next minor. Older releases receive nothing. |
| Upgrade path | Any supported release to the latest. A customer more than two minors behind must hop through an intermediate release identified in the release notes; migrations are cumulative and append-only, so the number of migrations is not itself the limit, the rehearsal is. |
| Schema compatibility | Expand-then-contract with N-1 compatibility once P2-5 lands (6.9). Before P2-5 every migration is treated as `breaking`: upgrade means maintenance window and restore point. |
| Downgrade | Not supported by reversing migrations. Supported by binary rollback only when all applied migrations since the target are `expand`; otherwise by restoring the verified pre-upgrade backup. |
| Rehearsal | Required before activation for Profile B and C; offered as automatic "rehearsal-lite" in Profile A. |
| Notices | Release notes state migrations, class, estimated migration time on a reference dataset, behavioural changes (for example the schema 40 key-grant default), manifest changes, configuration changes and removed features. |
| Emergency fixes | Patch releases never include `contract` or `breaking` migrations. |

### 12.2 What the rehearsal approach is today

Reconstructed from the release records for schemas 29, 38, 39 and 40
([operations/](operations/)) and the operator notes. The scripts themselves are
`rehearse.py` and `activate.py`, kept in private per-release directories on the VPS
(`/opt/dynodoc-releases/<date>-<name>/`); only helpers are in the repository.

**Preflight**

1. CI green in both repositories; `package-source.py <dest> [rev]` packs a committed
   revision with `SOURCE_MANIFEST.sha256` and `SOURCE_REVISION`.
2. Copy to the server, verify SHA-256, extract to `<release>/candidate`, run
   `sha256sum -c SOURCE_MANIFEST.sha256`.
3. Build candidate images tagged `release-<sha7>`; tag the running images
   `before-<release>` so a compatible rollback target exists.
4. Record aggregate inventory: counts of documents, events, uploads, identities, and
   event/snapshot digests, plus the applied schema fingerprint.

**Rehearsal on a production copy** (isolated; labelled containers; internal network)

5. `pg_dump -Fc` of production is restored into a throwaway PostgreSQL 16 container on
   an `internal: true` Docker network. Two harness pitfalls were found and are
   recorded: the dump GRANTs to the live runtime role, so that role must exist before
   restore; and database-level ACLs are not in the dump, so
   `REVOKE ALL ON DATABASE ... FROM PUBLIC; GRANT CONNECT ...` must be re-applied or
   `grant-runtime` refuses ("runtime role has DDL, temporary-object ...").
6. With the candidate image and operator credentials: `engine-cli migrate`,
   `grant-runtime <live role>`, `verify-snapshots --all`, `verify <document>` for each
   document, `check-runtime` with the runtime credentials.
7. Compare the aggregate inventory and digests before and after.
8. Start the candidate API on the restricted login: expect 401 for anonymous
   project/profile/admin routes and the device-session routes mounted; start the
   candidate web image and fetch public pages. For earlier releases the rehearsal also
   ran the *previous* API image against the migrated copy (the schema-compatible
   rollback image), synthetic login, canonical writes, snapshots, recovery, audit,
   erasure and logout revocation.
9. Remove every labelled container, network, volume and temporary credential.

**Activation**

10. Assert the preflight fingerprint equals the live one.
11. Nginx switched to maintenance 503 on every site; web, admin, API stopped.
12. Fresh `pg_dump -Fc`, `pg_restore --list`, SHA-256; the file is the restore point.
13. Operator migration, `grant-runtime`, `verify-snapshots --all`, `verify`,
    `check-runtime`; fingerprint unchanged apart from the schema version.
14. `docker compose up -d --no-deps --no-build --pull never api web admin` with a
    release override naming the pinned image IDs; the API environment is checked to
    contain no migration URL, no shared-writer flag and no signing key.
15. Nginx restored last; 22 to 23 live checks (health, login, headers, static files,
    redirects, Office panes, manifest, icons, `security.txt`).
16. Record evidence in `docs/operations/<date>-<release>.md`; keep the private directory.

Observed downtime: one to two minutes of maintenance responses.
A guarded fallback in `activate.py` restores the previous source tree from the
retained archive (projects release record).

### 12.3 Productisation (P0-5, P2-5)

| Stage | Command (proposed) | Notes |
|---|---|---|
| Verify the bundle | `dynodoctl bundle verify FILE` | Signature, manifest digests, platform, compatibility with the installed version |
| Preflight | `dynodoctl upgrade --plan` | Reads installed version, pending migrations and classes, free disk, backup age, licence validity; prints what will happen |
| Rehearse | `dynodoctl rehearse` | Steps 5 to 9 using the customer's data, in containers on the same or another host; writes `rehearsal-report.json` and a one-page PDF/Markdown for the change record. Requires a container runtime and disk for a database copy; for Profile C may run on a staging cluster using a restored backup |
| Activate | `dynodoctl upgrade` | Steps 10 to 16; automatic restore point; pre-staged images; verifies |
| Roll back | `dynodoctl rollback` | Binary rollback if all migrations are `expand`; else guided restore |
| Post-check | `dynodoctl verify-delivery`, `dynodoctl check` | Productised `check-connectors.py` and `check-runtime` |

Required changes to the existing scripts: move them into the repository with unit tests
(only `package-source.py` has one: `test_package_source.py`), parametrise hostnames,
container names and paths, make output machine-readable, and encode the two rehearsal
pitfalls as automated pre-conditions. The role-and-ACL replication in step 5 becomes a
documented function (`dynodoctl rehearse` creates the role and applies the ACL from the
live catalog) so it cannot be forgotten.


### 12.4 Observability and failure modes (summary)

Detail, metric names and alert thresholds are in the
[deployment guide](SELF-HOSTED-DEPLOYMENT-GUIDE.md), sections 10 and 11.

| Failure | Observable effect today | Target behaviour |
|---|---|---|
| PostgreSQL unreachable | API requests fail with 500/503; `/healthz` still 200 (liveness only) | `/readyz` fails; web shows a maintenance notice; no data loss |
| One API replica lost | Balancer drops it (health check); in-flight SSE streams reconnect with `Last-Event-ID` | Same, with draining on planned removal |
| NOTIFY listener lost | Not applicable today | Readiness fails; poll fallback; resync on reconnect |
| IdP unavailable | SSO sign-in fails; existing sessions continue until token expiry | Same; email link or operator link as break-glass if enabled |
| SMTP relay unavailable | Sign-in link send returns 503 | Outbox with retry (candidate); SSO unaffected |
| Converter down | Legacy `.doc` import returns 503 with an explanatory message | Same; optional component |
| Disk full on database host | PostgreSQL stops accepting writes | Disk alerts at 80% and 90%; documented recovery |
| Clock skew | Token `nbf`/`exp` failures; device challenge expiry | Documented NTP requirement; air-gapped sites use internal time source |
| Licence expired (proposed) | Not applicable | Banner, then grace behaviour; never data loss (licensing doc) |
| Hostname or origin change | Cookies, pairing grants, manifests, add-in local ledgers (origin-scoped) invalid | Runbook (deployment guide, 9.9) |
---

## 13. Delivery plan

### 13.1 Estimation basis (read before the numbers)

- **Unit.** One person-week (PW) is one engineer for one calendar week of focused work
  on this codebase, including unit and integration tests and the documentation the task
  itself needs.
- **Team assumption.** Engineers who already know the repository. A newcomer needs
  roughly 4 to 8 weeks before the table applies. The current team size is not known to
  the author of this document; calendar durations assume three engineers at about 75%
  planning utilisation (2.25 PW per calendar week). Change the divisor, not the PW.
- **Confidence.** H: well-understood work in familiar code; estimate within +/-25%.
  M: some design uncertainty; +/-40%. L: depends on external parties or unknown
  behaviour; +/-60% or worse.
- **Excluded from every estimate:** the receipt-authority rotation work (separate), the
  formula-engine replacement (done 14 October 2026), AI-agent connectors, the
  end-to-end encryption design, marketplace listings and installed-host acceptance of
  the six add-ins ([ADDIN-PUBLICATION-AND-ACCEPTANCE.md](ADDIN-PUBLICATION-AND-ACCEPTANCE.md)),
  UI redesign, external legal fees, external penetration-test and audit fees,
  certification fees, cloud and hardware costs, customer onboarding and support effort,
  and management overhead.
- **Not independent.** Estimates assume the earlier phase's interfaces are stable. A
  design-partner finding can invalidate downstream tasks.

### 13.2 Phase 0: harden the single-node bundle (Profile A)

Goal: a stranger can install, back up, restore and upgrade on one machine, offline if
necessary.

| ID | Task | PW | Conf. | Depends on | Notes |
|---|---|---|---|---|---|
| P0-1 | Parametrise hosted assumptions: `DYNODOC_MODE`, origins, brand, CSP `connect-src`, telemetry, SEO, `(site)` pages and operator facts hidden or replaced by a configurable organisation notice | 2.0 | H | none | Appendix B lists the files |
| P0-2 | Configuration schema, startup validation of every secret in web and API, `*_FILE` secret loading, env file generator | 1.5 | H | none | Includes the lazy `SESSION_SECRET` check |
| P0-3 | SMTP transport (STARTTLS/implicit TLS, auth), mail templates, outbox deferred; Resend optional; operator-issued sign-in links; ability to disable email sign-in | 2.0 | H | P0-2 | Library choice in the identity document (section 9) |
| P0-4 | Bundle: Compose project, bundled nginx with TLS modes (ACME, provided certificate, evaluation self-signed), health checks, volume layout, loopback-only exposure | 3.0 | M | P0-1, P0-2 | Reuses existing nginx maps |
| P0-5 | `dynodoctl` v1: install, configure, status, backup, restore-verify, upgrade, rehearse-lite, support bundle, `verify-delivery`, `addin build` | 5.0 | M | P0-4, P0-7 | Single static binary or Python zipapp; decision in deployment guide |
| P0-6 | Image pipeline: multi-arch builds, pinned base digests, no host compilation, SBOM generated at build | 2.0 | M | none | Signing is P3-1 |
| P0-7 | Add-in per-origin manifest generator route and offline builder, catalog-compatible manifest variant, local Office.js path, Google package generator | 3.0 | M | P0-1 | Section 7 |
| P0-8 | Retention jobs in the API (expired auth rows, `web_metric`, idempotency), configurable windows | 1.0 | H | none | Section 6.5 |
| P0-9 | Air-gap acceptance harness: egress-blocked network, assertion of zero outbound attempts, full sign-in/edit/backup/restore scenario | 2.0 | M | P0-4 | Becomes CI gate |
| P0-10 | Profile A documentation and runbooks, derived from the deployment guide | 1.5 | H | P0-4, P0-5 | |
| | **Phase 0 total** | **23.0** | | | about 10 calendar weeks at 2.25 PW/week |

**Acceptance (Phase 0).**

1. On a clean Linux VM with no outbound network, using only a signed bundle,
   `dynodoctl install` produces a running installation; first admin signs in with an
   operator-issued link; a document is created, edited and reviewed.
2. The packet capture in P0-9 shows no attempt to resolve or connect to any address
   other than the installation's own services.
3. Backup, delete-the-volume, restore, `verify-snapshots --all` and `verify` pass, and
   `dynodoctl` prints a restore report.
4. `dynodoctl upgrade` from release N to N+1 with rehearsal-lite succeeds on a dataset
   of 1,000 documents; rollback by restore succeeds.
5. Three Office manifests generated for a test origin validate with the vendored
   validator, and `verify-delivery` passes against the running installation.
6. No file in the shipped images contains `dynodoc.online` outside of third-party
   notices (automated check).

**Test strategy.** Unit tests for the config schema and manifest renderer; the existing
Rust and Playwright suites run in both modes; the harness above; a second clean-VM run
on arm64.

**Rollout and migration.** Hosted production continues unchanged; `DYNODOC_MODE`
defaults to `hosted`. No schema change is required in Phase 0 (P0-8 is code only). The
bundle is offered only to design partners (section 15).

**Implementation status (20 October 2026): P0-2 API half and P0-8 only.** Source and
tests in the application repository; nothing here is deployed or released, and every
other Phase 0 task is untouched.

- *P0-2, API half, built.* `crates/engine-api/src/secrets.rs` loads a secret from `NAME`
  or `NAME_FILE` (both non-empty is an error; plain variables behave exactly as before;
  files are size-bounded to 16 KiB, UTF-8, trailing line breaks trimmed, refused on Unix
  when group or others can access them unless `DYNODOC_ALLOW_OPEN_SECRET_FILES=true`).
  `crates/engine-api/src/config.rs` validates `PASETO_LOCAL_KEY`, `WEB_SERVICE_KEY`,
  `DATABASE_URL` (shape only), the refusal of `MIGRATION_DATABASE_URL` (plain or `_FILE`)
  and the retention settings before any database connection, and stops with one error
  listing every problem; no message contains a secret value. The API reads no other
  secret: the receipt-signing key was already a file (`DYNODOC_REF_SIGNING_KEY_FILE`).
  Section 3.1 row 12 is closed for the API only.
- *P0-2, not built (as of 20 October; the web process and the generator were built on 22
  October, see the next paragraph).* `engine-cli` (still reads `DATABASE_URL` and
  `MIGRATION_DATABASE_URL` from plain variables) and the compose wiring of the new
  variables. The Unix permission check has no Windows equivalent; its tests, the
  start-up tests and the retention tests were also run on Linux (container, pinned and
  current stable toolchains).
- *P0-8, built.* `crates/engine-api/src/retention.rs`: a loop in every API process that
  prunes in bounded batches under a PostgreSQL session advisory lock held on a
  dedicated, per-pass connection (one replica at a time; a crashed holder releases the
  lock with its session). Settings `RETENTION_JOBS`, `RETENTION_INTERVAL_SECONDS`,
  `RETENTION_BATCH_ROWS`, `RETENTION_AUTH_DAYS`, `RETENTION_METRICS_DAYS` (defaults and
  bounds in the deployment guide 7.1). Per-table counters, pass, skip and failure counts
  appear in `GET /admin/overview` under `retention` (per process); info log lines carry
  counts only. The runtime login already holds SELECT, UPDATE and DELETE on the six
  tables, so **no migration** was added and `check-runtime` is unchanged.
- *P0-8, deviations from section 6.5.* A rotated refresh token that has not expired is
  kept (it can still end its login through `revoke_login`); only expired or revoked rows
  past the window go. Request windows are pruned as soon as their 15-minute window ends
  (a pruned and a reset row behave the same). The job runs by default in every mode,
  including hosted, so the first pass after an upgrade removes the backlog.
- *P0-8, not built.* A lease table or `/readyz` job state (the advisory lock is
  per-pass, so no lease state exists to report), a Prometheus endpoint, removal of the
  host-side `web_metric` prune in `collect-status.py`, an index on `refresh_token.expires_at`
  (a table scan per batch, acceptable at the sizes in section 9 and not worth a
  migration today), and the copy-indexer lease of row 33.

**Implementation status (22 October 2026): P0-1 and the web half of P0-2.** Source and
tests in the application repository; nothing here is deployed or released, and the hosted
production release is unchanged. Settings and defaults are in the deployment guide 7.2.

- *Configuration, built.* `frontend/researcher/src/lib/server/config/`: `secrets.js` (the
  API's `NAME` / `NAME_FILE` rules and limits, and its error format), `schema.js` (every web
  setting, one aggregated error, typo warnings), `hosted.js` (the hosted hostnames, kept in
  one place), `index.ts` (read once, frozen; the `init` hook in `hooks.server.ts` runs it
  before the server listens). `DYNODOC_MODE=hosted` (default) or `selfhosted`;
  `DYNODOC_ALT_ORIGINS`, `DYNODOC_BRAND_NAME`, `DYNODOC_ORG_NAME`, `DYNODOC_ORG_NOTICE` or
  `DYNODOC_NOTICE_FILE`, `DYNODOC_SUPPORT_URL`, `DYNODOC_TELEMETRY`, `AUTH_EMAIL_LINK`. The
  five web secrets take `*_FILE`. `SESSION_SECRET` is checked at start (length, blankness,
  repeated pattern, documented placeholders) in both modes when it is set, and is required in
  self-hosted mode; see the hosted allowance below.
- *Self-hosted mode, built.* The `(site)` pages, `/sitemap.xml`, `/llms*.txt` and `/source`
  answer 404 (no redirect); `/` goes to sign-in or the workspace; no indexing anywhere
  (header, meta tags, `robots.txt`, no canonical or social tags); the CSP `connect-src` comes
  from the configured origins; page metrics are off, the browser code makes no request and
  `/telemetry` answers 404; the workspace-host redirect and the `app.dynodoc.online` root
  redirect do not apply; the sign-in page and the workspace footer show the organisation
  notice instead of the privacy and terms links; email sign-in can be switched off.
- *Hosted mode, unchanged.* Pinned by `hooks.test.ts` (headers and redirects per host),
  `hosted-pin.spec.ts` (the policy string, the 19 public pages with their canonical and index
  tags, the telemetry origins), and a before-and-after comparison of 95 responses (5 hosts, 18
  paths, 5 telemetry requests: status, headers and head tags) between the unchanged and the
  changed build, which differed in nothing but asset hashes. One deliberate allowance: a hosted
  process with no `SESSION_SECRET` and no `WEB_SERVICE_KEY` starts with warnings, because the
  public homepage host on Vercel runs with no secret ([VERCEL-DEPLOYMENT.md](VERCEL-DEPLOYMENT.md));
  a secret that is set but unusable stops either mode.
- *Environment files, built.* `frontend/researcher/scripts/dynodoc-env.mjs` with
  `generate-env` (random 32-byte secrets, one 0600 file each, a commented `.env`, refuses to
  overwrite, prints no secret, lists what to back up) and `check-config` (the server's
  validator on an existing directory, no server started, exit code and `--json` for
  `dynodoctl`).
- *Hosted-string audit, built.* `frontend/researcher/scripts/audit-hosted-strings.mjs` scans
  the build for the hosted hostnames, operator facts, analytics and third-party hosts and
  classifies each occurrence by whether a self-hosted installation can load the file. In the
  current build, 47 files outside third-party notices still contain `dynodoc.online`, so
  acceptance criterion 6 does **not** hold yet: 22 are the compiled `(site)` pages (11 browser
  files, 11 server files; unreachable in self-hosted mode, shipped in the build), 14 are
  source maps of those, 3 are server files every mode loads (the hosted preset, the hosted
  search-engine constants, the hosted `robots.txt` body; dormant in self-hosted mode), and 8
  are static files (the add-in packages built for `app.dynodoc.online`, `security.txt`, the
  social image). No browser script that every page loads names a hosted host or an operator
  fact. `--check` (run in CI) fails on anything not in `scripts/hosted-strings-known.json`;
  `--strict` applies the criterion as written. Closing the rest needs a build that leaves the
  `(site)` routes out (P0-6) and per-installation add-in packages (P0-7).
- *Not built.* Product name in the logo images, the installable-app name (`site.webmanifest`)
  and page titles outside sign-in, workspace, shared link and administration; the Office.js
  CDN (P0-7); SMTP and operator-issued sign-in links (P0-3); the Compose wiring of the new
  variables and a web image that contains the check tool (P0-4, P0-5, P0-6); `engine-cli`;
  Windows has no secret-file mode check (as in the API). The hosted `infra/demo/*`,
  `extensions/*` and `static/connectors/*` files in Appendix B are untouched.
- *Verified here.* `vitest`: 73 files, 743 tests pass and 4 (Unix file modes) are skipped on
  Windows; 133 of them are new (10 new files). The same Unix cases ran as plain checks on Linux
  as a non-root user (35 assertions). Chromium end-to-end against an isolated stack with
  three web processes on a disposable PostgreSQL database (hosted; self-hosted with secrets
  from files, the API too; self-hosted with email sign-in off): 21 new specs pass
  (`selfhosted.spec.ts` 12, `selfhosted-noemail.spec.ts` 2, `hosted-pin.spec.ts` 7) and 94 of
  the existing hosted specs pass (home, login, telemetry, seo, workspace redirect, projects,
  security, pwa, session revocation, repository, product, ux, browsers, agent proposals); in
  a second pass over the final build 64 of 65 passed, and the one that failed
  (`workspace.spec.ts`, a reviewer's suggestion) passed three times in a row alone. Mutation
  checks: accepting a placeholder secret, changing the hosted CSP and turning metrics on in
  self-hosted mode each fail unit tests, and the last two also fail the end-to-end specs; a
  hosted string put back in shared browser code fails the audit. GitHub Actions could not run
  (billing); `ci-frontend.yml` now runs the audit and starts the two self-hosted servers, and
  has not been executed.

### 13.3 Phase 1: identity

Goal: a university or company connects its identity provider and maps groups to roles;
leavers lose access without manual steps. Detail in the
[identity document](SELF-HOSTED-IDENTITY-AND-ACCESS.md).

| ID | Task | PW | Conf. | Depends on | Notes |
|---|---|---|---|---|---|
| P1-1 | Generic OIDC relying party: multiple providers, discovery, PKCE, nonce, claims mapping, RP-initiated and back-channel logout hooks; Google as a preset | 3.0 | H | P0-2 | `openid-client` and `jose` already in use |
| P1-2 | External identity link table, linking and email-change policy, engine API extension | 2.5 | M | none | New migration; MIT API surface |
| P1-3 | Administrator role model replacing the email allowlist; auditor role | 2.0 | M | P1-2 | `ADMIN_EMAILS` kept as bootstrap |
| P1-4 | Group-to-role mapping engine with source-tagged memberships | 3.0 | M | P1-1, P1-2 | |
| P1-5 | SCIM 2.0 server: Users, Groups, discovery; PATCH, filters, ETag; token management | 5.0 | M | P1-2, P1-4 | |
| P1-6 | Session model: configurable lifetimes, absolute cap, `login_session`, list and revoke, back-channel logout | 3.0 | M | P1-1 | |
| P1-7 | Offboarding: deactivation sweep, ownership-transfer report, periodic stream re-check | 2.0 | M | P1-6 | SCIM deactivation (P1-5) calls the same sweep; the re-check does not depend on P2-1 |
| P1-8 | Native SAML 2.0 service provider | 4.5 | L | P1-2 | Library-dependent; federation metadata handling |
| P1-9 | Service accounts: non-human identities, admin-driven key enrolment | 2.0 | M | P1-3 | |
| P1-10 | Interoperability matrix: Keycloak, Microsoft Entra ID, Okta, Google Workspace, Shibboleth IdP, AD FS | 3.0 | L | P1-1, P1-8 | Needs test tenants |
| P1-11 | LDAP bind and group sync (conditional on design-partner demand) | 3.0 | M | P1-4 | Excluded from the total |
| P1-12 | Security review of the authentication surface and fixes | 2.5 | M | P1-1 to P1-9 | External test fee excluded |
| | **Phase 1 total (without P1-11)** | **32.5** | | | about 14 calendar weeks |

**Acceptance (Phase 1).** SSO sign-in through each IdP in the matrix; group change at the
IdP reflected in access within the documented bound (SCIM push: seconds; claim-only:
next sign-in or token refresh); a SCIM deactivation ends web, device and connector
access within one minute; email-based takeover tests fail; SAML negative tests (wrapped
signature, replay, wrong audience, stale assertion) all fail closed.

**Test strategy.** Containerised IdPs in CI (Keycloak, SimpleSAMLphp or a Shibboleth
IdP image), recorded vendor-tenant runs for Entra and Okta, property tests for the
mapping engine, a SCIM conformance run with a published test client.

**Rollout and migration.** Additive migrations only. Existing accounts keep working;
linking occurs on first SSO sign-in by verified email when the provider allows it.

### 13.4 Phase 2: high availability and operations

| ID | Task | PW | Conf. | Depends on | Notes |
|---|---|---|---|---|---|
| P2-1 | Event fan-out via LISTEN/NOTIFY, resync, metrics, poll fallback | 3.0 | M | none | Section 6.2 |
| P2-2 | Shared rate and quota windows | 2.5 | M | none | Section 6.3 |
| P2-3 | Refresh reuse interval; web replica test | 1.0 | H | none | Section 6.4 |
| P2-4 | Job lease and leader for the copy indexer and retention | 1.5 | H | P0-8 | Section 6.5 |
| P2-5 | Schema compatibility window (N-1): classes, gate redesign, CI matrix | 5.0 | M | none | Section 6.9; touches the startup security gate, review required |
| P2-6 | Metrics, structured logs, trace export, dashboards and alert pack | 3.5 | H | none | |
| P2-7 | Audit and SIEM export (syslog, JSON lines, CEF; webhook), audit read API | 3.0 | M | P1-3 | |
| P2-8 | Helm chart and reference HA topology, hooks, NetworkPolicy, PodDisruptionBudget, kind/k3s tests | 4.5 | M | P2-5, P0-6 | |
| P2-9 | Physical backup and PITR tooling and drills | 3.0 | M | P0-5 | pgBackRest or equivalent; customer chooses |
| P2-10 | Signer abstraction (file, PKCS#11, KMS, external), Paseto key ring, secret providers | 4.0 | M | P0-2 | Excludes rotation feature |
| P2-11 | Load and soak harness, first capacity report for three workload shapes | 5.0 | L | P2-1, P2-6 | The first real numbers |
| P2-12 | Originals storage: object-store spike and decision record | 3.0 | L | P2-11 | Implementation excluded |
| P2-13 | Rolling-upgrade, failover and chaos test suite | 2.5 | M | P2-5, P2-8 | |
| P2-14 | Graceful shutdown, `/readyz`, connection draining, configurable pool | 1.5 | H | none | Section 6.6 |
| | **Phase 2 total** | **43.0** | | | about 19 calendar weeks |

**Acceptance (Phase 2).** Section 6.8 harness passes; a 3-node PostgreSQL failover under
write load loses no acknowledged write and the API recovers within the documented bound
(bound to be measured, not promised); PITR restore to a chosen timestamp passes
verification; rolling upgrade across an `expand` migration completes with zero failed
requests after retry; capacity report published with the template in 9.5.

**Rollout and migration.** The `event_notify` trigger migration is applied to hosted
production through the existing rehearsal route. `DYNODOC_EVENT_FANOUT` defaults to
`local`, so hosted behaviour does not change until switched.

### 13.5 Phase 3: distribution and licensing

Detail in the [licensing document](SELF-HOSTED-LICENSING-AND-DISTRIBUTION.md).

| ID | Task | PW | Conf. | Depends on | Notes |
|---|---|---|---|---|---|
| P3-1 | Release pipeline: signing with KMS/HSM key, SBOM, provenance attestations, vulnerability scanning, third-party notices generation | 5.0 | M | P0-6 | Pull forward if the first partner is Profile C |
| P3-2 | Offline bundle builder, verifier and importer; registry mirroring guide | 3.0 | M | P3-1 | |
| P3-3 | Licence file format, verifier, issuer tool, UI and grace behaviour | 4.0 | M | none | |
| P3-4 | Usage report and metering views | 2.0 | H | P1-6 | Needs `last_active_at` |
| P3-5 | Support bundle with redaction and customer review step | 2.0 | H | P0-5 | |
| P3-6 | Dependency licence audit and policy gate in CI | 1.5 | H | none | Excludes replacing libraries |
| P3-7 | Release and documentation portal (static; signed index; per-version docs) | 2.0 | H | P3-1 | |
| P3-8 | Opt-in telemetry file export; policy text | 1.0 | H | P3-4 | |
| | **Phase 3 total** | **20.5** | | | about 9 calendar weeks |

**Acceptance (Phase 3).** A bundle built in CI verifies offline on a clean machine with
only the published public key; tampering any file fails verification; a licence file
with expired validity puts the installation into the documented grace state without
data loss; the SBOM lists every component in every image; the third-party notices file
is generated and reviewed.

### 13.6 Phase 4: certification readiness

| ID | Task | PW | Conf. | Depends on | Notes |
|---|---|---|---|---|---|
| P4-1 | Control mapping, policy set and evidence automation (SOC 2 and ISO 27001 aligned) | 6.0 | L | P2, P3 | Auditor and tooling fees excluded |
| P4-2 | PSIRT process, advisory publication, CRA reporting runbook | 2.5 | M | P3-1 | |
| P4-3 | External penetration test coordination and remediation | 5.0 | L | P1, P2 | Remediation effort only |
| P4-4 | Accessibility conformance audit and remediation (WCAG 2.2 AA) | 4.0 | M | none | Existing statement is a goal |
| P4-5 | Cryptographic module investigation (validated module need, library options) and decision | 2.0 | L | none | |
| P4-6 | Data-protection pack: DPA template, transfer analysis, retention schedules | 2.0 | M | counsel | Counsel fees excluded |
| P4-7 | Reproducible builds and attestation hardening | 2.5 | L | P3-1 | |
| | **Phase 4 total** | **24.0** | | | about 11 calendar weeks |

Phase 4 prepares evidence. It does not produce a certificate; an external audit does,
on its own timeline and cost.

### 13.7 Roll-up, milestones and critical path

| Milestone | Content | Cumulative PW | Calendar (3 engineers, 2.25 PW/week) | What can be offered |
|---|---|---|---|---|
| M1 | Phase 0 complete | 23.0 | about week 10 | Profile A design partner, internet or air-gapped, email links |
| M2 | P1-1, P1-2, P1-3, P1-4, P1-6, P1-7 | 38.5 | about week 17 | Profile B design partner with OIDC SSO and group mapping, single or two replicas with sticky routing |
| M3 | Rest of Phase 1, plus P2-1, P2-2, P2-3, P2-4, P2-6, P2-9, P2-14 | 71.5 | about week 32 | Profile B production use; SAML and SCIM |
| M4 | Rest of Phase 2, plus P3-1, P3-2, P3-3 | 110.5 | about week 49 | Profile C intranet/air-gapped pilot with signed bundles, licences, HA, KMS signer |
| M5 | Rest of Phase 3, Phase 4 | 143.0 | about week 64 | Certification-readiness evidence; general availability |

Total **about 143 PW** (conditional LDAP adds 3.0): realistically 100 to 200 PW given
the stated confidence. Four engineers at the same utilisation (3.0 PW per week) would
reach M5 in about 48 weeks if tasks parallelise cleanly, which they only partly do:
P2-5, P1-2 and P2-1 are on the critical path and gate many others. A smaller team does
not scale linearly down: Phase 4 and parts of Phase 3 are largely independent of
engineering headcount and are bounded by external parties.

Critical path to a Profile C pilot: P0-1/2/4/5/6, P1-2, P2-1, P2-5, P2-8, P2-10, P3-1,
P3-2. Items that can start immediately and in parallel: P0-1, P0-2, P0-6, P0-8, P2-3,
P2-6, P2-14, P3-6.

---

## 14. Risks and mitigations

| ID | Risk | Likelihood | Impact | Mitigation | Owner |
|---|---|---|---|---|---|
| R1 | Cross-replica live-stream bugs produce silently stale editors | M | H | Section 6.2 design, harness 6.8, readiness fails on listener loss, poll fallback | Engineering lead |
| R2 | N-1 compatibility weakens the startup security gate or hides a breaking migration | M | H | Class file reviewed in CI, N-1-on-N test matrix, gate changes security-reviewed (P2-5) | Engineering lead |
| R3 | SAML implementation flaws (signature wrapping, replay) | M | H | Vetted library only, no custom XML-DSig, negative tests, external review (P1-12) | Engineering lead |
| R4 | Add-in channels change or fail on customer estates (unified manifest transition, Office.js terms, perpetual Office limits) | H | M | Catalog-compatible manifest, local Office.js option, per-channel acceptance, early partner tests | Product owner |
| R5 | Support burden from low-skill Profile A customers exceeds capacity | H | M | Self-service tooling, strict support model, restore and upgrade automation | Owner |
| R6 | Licence enforcement circumvented (MIT engine, root on the host) | H | L | Contract and audit rights; technical enforcement only as friction; no data lock-in or lock-out | Owner |
| R7 | EU Cyber Resilience Act obligations not met (reporting from 11 September 2026) | M | H | Counsel determination of Verne's role; PSIRT process (P4-2) before first customer | Owner + counsel |
| R8 | A copyleft dependency re-enters the proprietary bundle | M | H | `hyperformula` was removed on 14 October 2026; P3-6 gate fails the build on copyleft in the browser bundle | Engineering lead |
| R9 | Originals in PostgreSQL make backups and restores unmanageable at Profile C scale | H | H | P2-12 decision before Profile C GA; per-account and per-document budgets (exist) | Engineering lead |
| R10 | Receipt-authority key lost or leaked in a customer installation | L | H | Default not installed; enablement checklist; wait for rotation; escrow | Owner |
| R11 | A restored or cloned copy shares the installation identity and is mistaken for production, or receives the authority key | M | H | Runbook rules (8.2); future re-identification capability | Engineering lead |
| R12 | Customers expect end-to-end encryption | M | M | Section 1.3, 8.6, 11.6 and sales material state it is not provided | Owner |
| R13 | Capacity unknown; first Profile B/C customer exceeds it | H | H | Section 9 measurement plan; pilot limits in contract; early signals (deployment guide, 6.4) | Engineering lead |
| R14 | Scope exceeds team capacity; hosted product stalls | H | H | Phasing with design-partner gates; one codebase; decide Phase 2 scope after M2 | Owner |
| R15 | Google add-ons unusable in the markets that prefer on-premises | M | M | State plainly; focus on Office desktop; revisit | Product owner |
| R16 | Sanctions or export-control violation through licence issuance or support | L | H | Screening step; counsel classification (10.5) | Owner + counsel |
| R17 | Hosted and self-hosted drift into two products | M | M | `DYNODOC_MODE`; single CI; no self-hosted-only schema | Engineering lead |
| R18 | Verne's jurisdiction and time zone (UTC+6) limit support coverage and customer trust in some sectors | M | M | Documented support hours; no remote access model; documentation-first | Owner |

---

## 15. First-customer validation plan

### 15.1 Partners

Two or three design partners, one per profile if possible: a research lab or small
company (A), a university department or a mid-sized organisation with SSO (B), and a
regulated or network-restricted organisation (C). Pilot terms (duration, support
expectations, feedback commitments, no-fee or reduced-fee status) are an owner decision
(Q1, Q5).

### 15.2 What we need from them

| Need | Detail |
|---|---|
| Environment facts before install | OS and virtualisation, Kubernetes or not, PostgreSQL standard, reverse proxy/WAF, internal CA, outbound policy, mail relay, IdP product and protocol, Office versions and licensing (M365 or perpetual; Windows/Mac share), Google Workspace use, endpoint management |
| Identity facts | Group structure and naming, which groups should map to which roles, expected account lifecycle, MFA policy, federation membership (eduGAIN) |
| Documents | Typical file types and sizes, number of collaborators per document, retention rules; a synthetic or anonymised corpus for rehearsal |
| Security and legal | Their security questionnaire, DPA/EULA redlines, export-control constraints, data classification |
| Time | A named technical contact, weekly 30-minute call, one backup-restore drill, one upgrade rehearsal, one add-in rollout |
| Evidence (opt-in) | A redacted support bundle at agreed points: counts of users, documents, events, database size, resource use |

### 15.3 Hypotheses and measures

| ID | Hypothesis | Method | Pass threshold | Decision it informs |
|---|---|---|---|---|
| V1 | An IT generalist can install Profile A from documentation | Observed install, timed | Median 60 min or less; 0 database-knowledge steps | Phase 0 UX; installer scope |
| V2 | The role ladder (viewer, contributor, reviewer, editor, manager, owner) and project policy floors express the customer's access model | Map their groups on paper, then in the product | At least 90% of groups map without new roles | Group mapping design (identity doc) |
| V3 | Customer IT can distribute add-ins through a supported channel | Pilot rollout to 10 users | 90% installed within 5 working days; ribbon appears or the pane is reachable | Section 7 priorities; catalog manifest |
| V4 | Backup and restore drill meets the customer's recovery expectation | Timed drill from their backup storage | At or under their stated target (record the target; we promise none) | Backup tooling (P2-9) |
| V5 | Upgrade rehearsal fits their change process | Rehearsal on their data; change ticket from the report | Passes first time; report accepted by their change board | Rehearsal report format |
| V6 | Single-node capacity covers their real workload for a quarter | Opt-in counts plus resource use | No sustained 429/504; disk growth within plan | Sizing coefficients (section 9) |
| V7 | SSO onboarding takes under a day with their IdP | Timed, from metadata to first group-mapped login | 8 working hours or less | Identity UX and docs |
| V8 | Offboarding via IdP deactivation leaves no live access | Test: deactivate a user with open sessions, a device key, a connector grant | All revoked within 1 minute | Offboarding design |
| V9 | The offline licence workflow fits procurement | Walk through a renewal and a seat change | No step requires connectivity; owner accepts evidence format | Licensing design |
| V10 | Their security review can be satisfied from shipped artefacts | Submit SBOM, signatures, threat model, config guide | At most 10 open follow-up questions that need a new artefact | Phase 4 priorities |
| V11 | Support can be given from a redacted bundle without remote access | Real incidents | At least 80% resolved without further data | Support model |
| V12 | The data inventory and regulatory mapping answer their DPO | DPO review | No blocking gap that needs code | Section 10 |

Each hypothesis has an owner, a date and a recorded result in a shared log. A failed
hypothesis changes the plan before the next phase starts; Phase 2 scope is decided
only after M2 evidence.

---

## 16. Cost-to-serve analysis (pricing-neutral)

No price is proposed or implied. The model shows what supporting the edition costs,
by driver, so that any price can be tested against it. Hours below are **assumptions to
be replaced with tracked data from design partners**; none was measured.

### 16.1 Fixed costs (do not scale with customer count)

| Component | Driver | Assumed size |
|---|---|---|
| Release engineering and CI matrix (Compose, Kubernetes, IdP containers, two architectures) | Number of supported platforms and releases | 0.3 FTE |
| Security response (vulnerability intake, patches, advisories, CRA reporting) | Dependency count; supported versions | 0.2 FTE |
| Documentation and customer-facing material | Release cadence | 0.15 FTE |
| Signing and distribution infrastructure | Registry, KMS/HSM, portal | small recurring fee; amount unknown |
| Test estate | IdP developer tenants, Microsoft 365 developer subscription and Office builds, Windows and Mac machines, CI minutes | recurring; amount unknown |
| Compliance programme | Policies, annual audit, annual penetration test, tooling | external fees; quotes required |
| Legal | EULA and DPA maintenance, export classification, jurisdictional review | external fees; quotes required |

### 16.2 Variable cost per customer (hours per year)

| Driver | A | B | C |
|---|---|---|---|
| Onboarding (documentation call, installation help, add-in rollout) | 6 | 60 | 200 |
| Ongoing support (tickets, diagnostics bundle review) | 24 | 96 | 360 |
| Upgrade assistance (four releases a year) | 4 | 16 | 64 |
| Security questionnaires and reviews | 2 | 16 | 120 |
| **Total (assumed)** | **36** | **188** | **744** |

Distribution cost per release per customer is the image bundle size times downloads;
the bundle size is unknown until P3-2 and the converter (LibreOffice) is likely the
largest component; make the converter optional to keep Profile A small.

### 16.3 Per supported version

Every additional supported minor adds a backport, a test cycle and advisory text for
each security fix. With two supported minors and an assumed 8 security patches a year,
at an assumed 24 hours per patch per version, that is about 380 hours a year; widening
the window to three minors adds about 190 hours. This is why the support window in 12.1
is two minors.

### 16.4 Break-even form

```
required_net_revenue_per_year  >=  F  +  sum over profiles p ( N_p * v_p * h )
```

`F` is the fixed total in 16.1 in currency; `N_p` the customers in profile `p`; `v_p` the
variable hours from 16.2; `h` the loaded hourly cost. Example in hours only: 100 Profile
A customers consume about 3,600 hours (2 FTE at 1,800 hours); 20 Profile B about 3,760
hours (2.1 FTE); 5 Profile C about 3,720 hours (2.1 FTE). The observation that matters:
**Profile A is viable only if support per customer is near zero**, which is a product
(documentation, automation, restore and upgrade tooling) requirement, not a pricing
detail.

### 16.5 What this excludes

Sales and marketing, customer success outside support, custom development, hosting
that Verne might operate for customers (a different cost structure with its own risk),
and the cost of the engineering phases themselves (section 13).

---

## 17. Open questions

Each has a recommended default and a decider. Questions specific to a companion document
appear there with their own prefix.

| # | Question | Recommended default | Decider |
|---|---|---|---|
| Q1 | Edition names and which capabilities are gated | Community (engine only), Team (A), Organization (B), Enterprise (C); gate SAML/SCIM/LDAP, HA features, SIEM export, KMS signer, air-gap bundle | Owner |
| Q2 | Open-core boundary: where enterprise identity code lives | Proprietary web tier plus thin MIT API extension points (link table, group-sync endpoints) | Owner with counsel |
| Q3 | Licence enforcement behaviour | Fail-soft: never block read, export, verify, backup or restore; block only new seats or projects after grace | Owner |
| Q4 | Seat definition | Distinct identities with an authenticated action in the trailing 90 days | Owner |
| Q5 | Support commitments | No contractual SLA in v1; business-hours best effort in UTC+6 with documented severities | Owner |
| Q6 | Supported platforms | Linux x86-64 and arm64; Docker Engine with Compose v2; Kubernetes within the last three minor versions; PostgreSQL 16 only until another version passes the suite | Engineering lead |
| Q7 | Ship a local copy of Office.js | Not until counsel confirms redistribution terms; support a configured path; customer-supplied copy as the documented air-gap route | Counsel, then owner |
| Q8 | Authority key in customer installs | Not installed by default; enable by checklist after rotation exists | Owner |
| Q9 | Offline bundle format | OCI image layout plus signed manifest | Engineering lead |
| Q10 | Bundled or external PostgreSQL | Bundled for A; external recommended for B and C | Engineering lead |
| Q11 | Publish bundle tooling sources | Private repository, public documentation | Owner |
| Q12 | SAML: native or via a bridge first | OIDC native first; supported SAML-to-OIDC bridge configuration at M2; native SAML at M3 | Engineering lead |
| Q13 | Replace `ADMIN_EMAILS` with roles | Yes; keep it as first-run bootstrap only | Engineering lead |
| Q14 | Private research hosts | Explicit allowlist with SSRF protections; off in air-gap mode | Engineering lead |
| Q15 | Direct LDAP | Defer; use a bridge; build after two partners ask | Owner |
| Q16 | Object storage for originals | Spike in Phase 2; decide before Profile C general availability | Owner and engineering lead |
| Q17 | Validated cryptography | Not claimed; investigate in Phase 4 | Owner |
| Q18 | Counsel review of the regulatory mapping, EULA and export classification | Mandatory before any customer-facing use | Owner |
| Q19 | Moving a pilot from the hosted service to a customer installation | Project history export and import through the CLI; no database extraction | Engineering lead |
| Q20 | Contracting entity versus copyright holder | Counsel aligns them; the licensor must hold or be authorised to license the rights (Verne Systems is the operator; Verne Ltd. holds copyright) | Owner and counsel |
| Q21 | Relationship with the end-to-end encryption design ([E2EE-CRYPTOGRAPHIC-DESIGN.md](E2EE-CRYPTOGRAPHIC-DESIGN.md)) | Self-hosted ships without it; an encrypted mode is a separate profile with documented loss of server-side features | Owner |
| Q22 | Telemetry | None by default; opt-in usage report written to a file | Owner |
| Q23 | Support access model for Profile C | No remote access; customer-driven screen sharing; redacted bundles | Owner |

---

## 18. Appendices

### Appendix A: glossary

| Term | Meaning |
|---|---|
| Authority (receipt authority) | The installation's Ed25519 key pair whose public key is installed once and which signs branch-head receipts |
| Bundle | A signed archive containing images, charts, manifests and tooling for one release |
| `dynodoctl` | Proposed operator tool (name provisional); wraps `engine-cli`, Compose or Helm |
| Expand / contract | Migration classes (6.9) |
| Installation UUID | `device_authority` singleton created by migration 0033 |
| Operator login | The PostgreSQL owner/migration credential, used only by `engine-cli migrate`, `grant-runtime`, `install-ref-authority` |
| Profile A / B / C | Design targets in section 2 |
| Rehearsal | Restoring a production copy into a throwaway database and running the candidate release against it before activation |
| Runtime login | The restricted PostgreSQL login the API uses; no DDL, no history UPDATE/DELETE |

### Appendix B: file index of hosted-only assumptions (P0-1)

Status 22 October 2026, web tier (the first eleven rows, `svelte.config.js` through
`src/routes/source/+server.ts`): the code that runs in every mode now reads the configuration
(the CSP, the request hook, page metrics, the theme cookie domain, the sign-in, workspace,
shared-link, administration and help screens, `robots.txt`). What stays as hosted-mode code,
never served in self-hosted mode: the search-engine constants in `src/lib/seo/`, `nav.ts`,
the `(site)` pages, `src/lib/site/operator.ts`, `/sitemap.xml`, `/llms*.txt` and `/source`.
`scripts/audit-hosted-strings.mjs` lists what remains in a build. `extensions/*`,
`infra/demo/*` and `static/connectors/*` are untouched (P0-4, P0-7).

| Path | Assumption |
|---|---|
| `frontend/researcher/svelte.config.js` | CSP `connect-src` includes `https://app.dynodoc.online`; `img-src` allows `https:` |
| `frontend/researcher/src/hooks.server.ts` | `app.dynodoc.online` root redirect to `dynodoc.online`; `PUBLIC_WORKSPACE_URL` host redirect; `ADMIN_SITE` logic |
| `src/lib/telemetry.ts`, `src/routes/telemetry/+server.ts` | Host allowlist and fixed destination for page metrics |
| `src/lib/theme.svelte.ts` | Theme cookie domain `.dynodoc.online` |
| `src/lib/seo/site.ts`, `src/lib/seo/llms.ts`, `src/routes/robots.txt`, `src/routes/llms*.txt`, `src/routes/sitemap.xml`, `scripts/indexnow.mjs`, `static/*.txt` | Canonical origin, indexable hosts, IndexNow key |
| `src/lib/components/site/nav.ts` | Host-based navigation |
| `src/routes/login/+page.svelte`, `workspace/+page.svelte`, `admin/+page.svelte`, `view/[token]/+page.svelte`, `HelpGuide.svelte` | Links to `dynodoc.online`, `app.dynodoc.online` |
| `src/routes/admin/+page.server.ts` | Environment label derived from the hostname |
| `src/routes/(site)/*` | Marketing and legal pages naming Verne Systems, Resend, Vercel |
| `src/lib/site/operator.ts` | Operator identity and contact facts |
| `src/routes/source/+server.ts` | Redirect to the public engine repository |
| `extensions/*/manifest.xml`, `extensions/google-*/Code.gs`, `appsscript.json`, `frontend/researcher/static/connectors/commands.html` | Origin and CSP meta |
| `infra/demo/compose.yml` | Admin service `ORIGIN` and `PUBLIC_WORKSPACE_URL`, fixed ports, `../../ops` mount |
| `infra/demo/nginx-*.conf`, `infra/demo/.env.example` | Hostnames, certificate paths, `ORIGIN` |
| `infra/demo/check-connectors.py`, `collect-status.py`, `backup.sh` | Default origin, container names, `/opt/dynodoc-demo` |

### Appendix C: references

Verified means the page was fetched or searched on 14 October 2026 and the statement used
matches. "Unverified" items are marked inline in the text.

| Topic | Reference | Status |
|---|---|---|
| Office.js CDN, legacy endpoint, Marketplace and local-reference statement | https://learn.microsoft.com/en-us/office/dev/add-ins/develop/referencing-the-javascript-api-for-office-library-from-its-cdn | Verified |
| Office add-in deployment methods; network share not for production; SharePoint catalog without `VersionOverrides` | https://learn.microsoft.com/en-us/office/dev/add-ins/publish/publish | Verified |
| SharePoint app catalog, Trusted Catalogs policy | https://learn.microsoft.com/en-us/office/dev/add-ins/publish/publish-task-pane-and-content-add-ins-to-an-add-in-catalog | Verified |
| Centralized Deployment requirements and nested-group limit | https://learn.microsoft.com/en-us/microsoft-365/admin/manage/centralized-deployment-of-add-ins | Verified |
| Unified manifest client support | https://learn.microsoft.com/en-us/office/dev/add-ins/develop/unified-manifest-overview | Verified |
| `@microsoft/office-js` package status and licence ambiguity | Search results, Microsoft Q&A and Tech Community, July 2026 | Verified as ambiguous; legal review required |
| PostgreSQL NOTIFY semantics, payload limit, queue | https://www.postgresql.org/docs/16/sql-notify.html | Verified |
| NOTIFY commit-lock contention reports | https://recall.ai/blog/postgres-listen-notify-does-not-scale ; https://dbos.dev/blog/postgres-listen-notify-scalability | Verified as reports; version status unverified |
| PgBouncer and LISTEN | https://www.pgbouncer.org/features.html | Verified via search summary |
| sqlx migration lock | https://github.com/launchbadge/sqlx (sqlx-postgres `migrate.rs`) | Verified |
| EU Cyber Resilience Act reporting from 11 Sept 2026 | https://digital-strategy.ec.europa.eu/en/policies/cra-reporting | Verified via secondary summaries |
| FERPA outsourcing (34 CFR 99.31(a)(1)(i)(B)) | https://studentprivacy.ed.gov/faq/who-school-official-under-ferpa | Verified (guidance; regulation text via eCFR not fetched) |
| EAR encryption provisions | https://www.bis.gov/learn-support/encryption-controls ; 15 CFR 742.15 on eCFR | Partially verified; conflict noted in text |
| AWS KMS Ed25519 | https://aws.amazon.com/about-aws/whats-new/2025/11/aws-kms-edwards-curve-digital-signature-algorithm/ | Verified |
| Google Cloud KMS Ed25519 | https://docs.cloud.google.com/kms/docs/algorithms | Verified |
| Vault transit ed25519 | HashiCorp Vault documentation (transit key types) | Unverified; check at build time |
| Apps Script URL Fetch | https://developers.google.com/apps-script/reference/url-fetch/url-fetch-app | Verified; reachability inference is ours |
| GDPR text | EUR-Lex, Regulation (EU) 2016/679 | Article numbers standard; not re-fetched |
| OpenID Connect Back-Channel and RP-Initiated Logout | https://openid.net/specs/openid-connect-backchannel-1_0.html ; https://openid.net/specs/openid-connect-rpinitiated-1_0.html | Verified |
| SCIM 2.0 | RFC 7643, RFC 7644 (https://datatracker.ietf.org/doc/html/rfc7644) | Verified |
