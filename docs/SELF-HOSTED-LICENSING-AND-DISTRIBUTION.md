# Dynodoc self-hosted edition: licensing and distribution

> **Status: design, not implemented.** 14 October 2026. This document proposes how the
> self-hosted edition is packaged, signed, delivered, licensed, metered, supported and
> governed legally. It is **not legal advice**; every legal statement is a question or a
> checklist item for counsel (master Q18). No price is proposed.
>
> Read first: the [master design](SELF-HOSTED-EDITION.md). Companions:
> [deployment guide](SELF-HOSTED-DEPLOYMENT-GUIDE.md),
> [identity and access](SELF-HOSTED-IDENTITY-AND-ACCESS.md).

Contents

1. Purpose, goals, non-goals, personas, success metrics
2. Current state versus target
3. Editions and profile mapping
4. What is open and what is proprietary
5. Third-party components and copyleft policy
6. Artefact distribution and signing
7. Offline bundles
8. Helm chart
9. Licence key design
10. Seat and project metering without phoning home
11. Telemetry policy
12. EULA and legal-review checklist
13. Support model and security response
14. Security analysis of the licensing and distribution channel
15. Operations
16. Delivery plan
17. Risks and open questions
18. Appendices

---

## 1. Purpose, goals, non-goals, personas, success metrics

### 1.1 Purpose

Today the proprietary application is built from a private repository on Verne's own
server and used by one operator. Once it is delivered to other organisations, five new
questions appear that the engine alone never raised: what exactly are customers allowed
to do with it, how do they know the bits are ours and untampered, how does a licence work
on a network that may never reach the Internet, how is use measured without surveillance,
and what do we owe them after delivery.

### 1.2 Goals

| ID | Goal |
|---|---|
| LG1 | A clear line between the MIT engine/API/CLI and the proprietary workspace, add-ins and tooling, stated the same way in the repository, the EULA and the product. |
| LG2 | Every artefact a customer runs is signed and verifiable offline with a public key the customer can pin. |
| LG3 | A licence mechanism that works with no connectivity and never makes data or provenance inaccessible. |
| LG4 | Metering that the customer can see, understand and export, with no network call to Verne. |
| LG5 | A support model that is honest about what is and is not promised. |
| LG6 | No default telemetry. |
| LG7 | A distribution of third-party software that meets its licences (no GPL surprise). |

### 1.3 Non-goals

- Pricing, packaging prices, discount structures, reseller programmes.
- Hardware-bound or dongle licensing, remote kill switches, online activation.
- Technical measures that cannot be defended: obfuscation of the Node bundle, anti-debug.
- Marketplace publication (Microsoft, Google) of the add-ins; a store listing cannot point
  at per-customer origins (master 7.4) and is a separate project.
- Contractual SLAs (section 13).
- Legal conclusions.

### 1.4 Personas

| Persona | Needs |
|---|---|
| Procurement and legal | A readable licence, clear usage definitions, third-party notices, export statements |
| Security reviewer | Signatures, SBOM, provenance, vulnerability process, no phone-home |
| Platform engineer | Artefacts by digest, mirror-friendly, Helm, offline import |
| Licence administrator (customer) | See usage versus entitlement; renew without surprises |
| Verne release engineer | Reproducible, auditable release pipeline; key custody |
| Verne support engineer | Know which version and edition a customer runs; redacted diagnostics |
| Verne counsel | A checklist of decisions and an artefact inventory |

### 1.5 Success metrics (targets to validate)

| Metric | Target |
|---|---|
| Offline verification of every shipped artefact with only the published root key | 100% in CI and in the first customer pilot |
| Licence renewal without connectivity | Completed by a customer administrator in 30 minutes or less, documentation only |
| Licence expiry causing loss of access to data or export | Zero (by design) |
| Third-party licence findings blocking a release | Zero at release time (gate) |
| Customer-reported usage discrepancy versus Verne's understanding at true-up | Resolved from the customer's exported report without remote access |

---

## 2. Current state versus target

| # | Capability | Tag | Evidence and limitation |
|---|---|---|---|
| 1 | Proprietary notice for the application; MIT for the engine | EXISTS | [`LICENSE`](../LICENSE) (Verne Ltd., all rights reserved); [decision 013](decisions/013-private-application-licence.md); engine repository `ArefinAlter/dynodoc-engine` under MIT |
| 2 | Workspace package marked unlicensed and not published | EXISTS | `frontend/researcher/package.json` `"license": "UNLICENSED"`; Cargo `publish = false`, `license-file = "LICENSE"` |
| 3 | Licence page listing third-party components | PARTIAL | `/licenses` lists direct dependencies and fonts; no generated notices file, no transitive closure, no SBOM |
| 4 | GPL-free browser bundle | PARTIAL | `hyperformula` was removed on 14 October 2026 (in-house formula engine, FORMULA-ENGINE-REPLACEMENT.md); a CI gate against copyleft in the bundle is still to build (P3-6). The AGPL text was removed on 13 October 2026. |
| 5 | Published, signed container images | MISSING | Images are built on the VPS from a source archive; no registry, no signatures |
| 6 | Source packaging with manifest | EXISTS | [`package-source.py`](../infra/demo/package-source.py) (committed revision, SHA-256 manifest). Now unused by the site; kept as a private deployment artefact ([SECURITY-HARDENING.md](SECURITY-HARDENING.md)) |
| 7 | Reproducible builds | PARTIAL | `Cargo.lock`, `--locked`, `rust-toolchain.toml` (1.96 in the Dockerfile), `pnpm install --frozen-lockfile`; images not byte-reproducible |
| 8 | SBOM, provenance attestations | MISSING | |
| 9 | Licence file verification, metering | MISSING | |
| 10 | Offline bundle, Helm chart | MISSING | |
| 11 | Vulnerability disclosure contact | PARTIAL | `/.well-known/security.txt` (static), `info@vernesystems.com`; no advisory process |
| 12 | EULA, DPA | MISSING | Hosted drafts exist at `/terms`, `/dpa` and are "not yet effective" ([LEGAL-PAGES.md](LEGAL-PAGES.md)); they describe the hosted service and name Verne as operator |
| 13 | Support process | MISSING | Contact email only |

---

## 3. Editions and profile mapping

Names, boundaries and numbers are proposals for the owner (master Q1). The principle:
**gate capacity and integration breadth, never integrity, safety or the ability to
leave.** Hash-chain verification, backups, restores, exports, `engine-cli` and security
fixes are available in every edition.

| Capability | Community | Team | Organization | Enterprise |
|---|---|---|---|---|
| Intended profile | developers | A | B | C |
| Engine, API, CLI (MIT) | yes | yes | yes | yes |
| Web workspace, Projects, review, admin console | no | yes | yes | yes |
| Office and Google add-in packages and manifest generator | no | yes | yes | yes |
| Email sign-in, Google, one OIDC provider | n/a | yes | yes | yes |
| Several OIDC providers, SAML 2.0, group mapping | n/a | no | yes | yes |
| SCIM 2.0, service accounts | n/a | no | yes | yes |
| Replicas, event fan-out, shared limits | n/a | no | yes | yes |
| Metrics endpoint, structured logs | n/a | yes | yes | yes |
| Audit export to SIEM | n/a | no | optional | yes |
| KMS/HSM signer, two-person erasure | n/a | no | no | yes |
| Offline bundles, air-gap acceptance documentation | n/a | optional | optional | yes |
| Helm chart and reference HA | n/a | no | yes | yes |
| Third-party licence and security documentation pack | n/a | no | yes | yes |
| Extended support window, source escrow option | n/a | no | no | by agreement |
| Seat and project limits | none | `limits.seats`, `limits.projects` in the licence | same | same |
| Support | none | documentation, email | documentation, email, upgrade assistance | named contact, advisories, assisted upgrade |

How gating is enforced: licence `features` flags switch routes and screens on in the web
tier (section 9); the MIT engine contains no gating code. This means a determined
customer can use engine capabilities without a licence; that is acceptable (R6).

---

## 4. What is open and what is proprietary

### 4.1 Boundary

| Component | Repository | Licence | Delivered to customers as |
|---|---|---|---|
| `engine-core`, `engine-shared`, `engine-api`, `engine-cli`, migrations, OpenAPI | Private `crates/` mirrored to public `ArefinAlter/dynodoc-engine` | MIT, copyright Verne Ltd. | Compiled binaries in images; source upstream |
| Web workspace, admin console, public site (SvelteKit) | Private | Proprietary, Verne Ltd. | Compiled server bundle in the web image; no source |
| Office and Google add-ins | Private | Proprietary | XML manifests, Apps Script sources, panes served by the web image. Apps Script code is readable by the customer's Google administrators by nature; pane JavaScript is readable in a browser |
| `dynodoctl`, Compose project, Helm chart, proxy templates (proposed) | Private | Proprietary | Binary and rendered templates |
| Licence verifier, enterprise identity connectors (proposed) | Private (web tier) | Proprietary | Inside the web image |
| Documentation | Private | Proprietary, redistributable within a customer organisation | Bundle |
| Converter shim (`infra/converter/server.py`) | Private | Proprietary shim plus third-party LibreOffice | Optional image |

The proprietary [`LICENSE`](../LICENSE) already states that the MIT licence of the engine
"applies to that repository only and does not extend to the workspace, website or editor
connectors".

### 4.2 Consequences to state in the EULA and the documentation

1. A customer may use, modify and redistribute the MIT components under MIT. The EULA
   must not contradict that. Verne's support applies to unmodified Verne-built binaries.
2. The proprietary components are licensed for use inside the customer organisation for
   the licensed term, edition and limits; not for resale, hosting for third parties
   (service bureau), or reverse engineering beyond what law allows.
3. A third party could in principle build its own interface on the MIT API. That is
   intended by decision 013 and is not a licence breach.
4. The product name and logo are not licensed by MIT. A trademark and naming policy for
   forks and derived distributions is needed (open question LQ6).
5. Contributions to the public engine repository: whether a contributor licence
   agreement or developer certificate of origin applies is **unverified; check the
   engine repository's contribution files** (LQ7).

### 4.3 Open-core risk

The more enterprise capability lives in the MIT API, the easier it is to run without a
licence; the more lives in the proprietary web tier, the more the MIT engine depends on a
closed front end for protocol work. The master design places protocol endpoints (OIDC,
SAML, SCIM, mail, licence checks) in the web tier and keeps the API generic (identity
document, 3.1). This keeps the engine usable standalone and the commercial value in
integration, packaging, tooling and support.

---

## 5. Third-party components and copyleft policy

Delivering the product to customers is **distribution**. Copyleft obligations that a
private hosted service does not trigger apply to artefacts that leave Verne's control.

### 5.1 Known items

| Component | Licence (as understood; regenerate from manifests) | Obligation | Action |
|---|---|---|---|
| `hyperformula` | GPL-3.0-only or a commercial licence from its publisher | Distribution of a combined work in a closed product is not permitted under GPL; the browser bundle ships the library | Replaced and removed from the dependency tree (14 Oct 2026); the CI gate (P3-6) keeps it out. |
| LibreOffice (converter image) | MPL-2.0 and other component licences (LGPL, GPL pieces in dependencies) | Notices; source availability for covered files | Converter is optional; ship as a separate image; provide the Debian source references for the exact package versions; counsel confirms the "mere aggregation" position |
| Debian base layers (`bookworm-slim`) | Mixed (GPL, LGPL, BSD, MIT) | Notices; written offer or equivalent for source of GPL components | Notices generated from `dpkg-query`; source offer text in the notices file and on the support site |
| Node.js, pnpm output | MIT and bundled third-party licences | Notices | Generated |
| PostgreSQL | PostgreSQL licence | Notice | Bundled image is an unmodified upstream image where possible |
| nginx | BSD-2-Clause | Notice | |
| Rust crates (`axum`, `sqlx`, `tokio`, `serde`, `ring`, `pasetors`, ...) | Mostly MIT or Apache-2.0; `ring` has its own mixed permissive terms | Notices and attribution | Generated by a license tool; reviewed |
| JS packages (Tiptap, ExcelJS, mammoth, docx, pptxgenjs, pagedjs, KaTeX, JSZip, fflate, jose, openid-client, papaparse, web-vitals, lucide) | Mostly MIT, BSD-2/3, Apache-2.0, ISC; JSZip is offered as MIT or GPL-3.0 (choose MIT) | Notices; dual-licence election recorded | Generated and reviewed; each Tiptap extension checked for open-source status |
| Fonts (Geist, Geist Mono, Public Sans, Noto families) | SIL OFL 1.1 | Include licence text; do not sell fonts alone | Already in `static/fonts/*-OFL.txt` |
| Office.js | Microsoft terms (unclear) | See master 7.3 | Not bundled until counsel confirms (Q7) |

### 5.2 Policy

1. **Allowlist.** Permissive licences (MIT, BSD-2/3, ISC, Apache-2.0, PostgreSQL, OFL for
   fonts, MPL-2.0 for unmodified components) are allowed. Everything else needs written
   approval.
2. **Denylist in the browser and server bundles.** GPL, AGPL and SSPL are forbidden in
   anything linked into or shipped in the proprietary web or add-in bundles.
3. **Dual licences** are resolved explicitly in a record (for example JSZip: MIT).
4. **Transitive closure** is audited, not only direct dependencies.
5. **Gate.** The pipeline fails on a new unapproved licence; exceptions are recorded in a
   file reviewed by counsel.
6. **Notices.** A generated `THIRD-PARTY-NOTICES` file ships in every image and bundle
   and on the documentation site.
7. **Base-image source offer.** Written offer text plus a stable source mirror reference
   for the Debian packages in each release.

Tooling candidates (unverified; check at build time): `cargo-deny` or `cargo-about` for
Rust, a lockfile-based license checker for pnpm, `syft` or `cdxgen` for images.

---

## 6. Artefact distribution and signing

### 6.0 Flow and trust boundaries

```
  Verne build (CI, no signing key)            Verne signing (KMS/HSM, two approvers)
        |  images, chart, tools, SBOM               |  signs manifest, digests, attestations
        v                                           v
  +-----------------------------------------------------------+
  |  Release artefacts + signed manifest + notices            |   Verne boundary
  +----------------------------+------------------------------+
                               | download portal / media (untrusted channel)
                               v
  +-----------------------------------------------------------+
  |  Customer: dynodoctl bundle verify (pinned root key)       |   Customer boundary
  |  -> import to internal registry -> rehearse -> activate    |
  +----------------------------+------------------------------+
                               | licence file (separate path, signed by licence key)
                               v
                         installation (web tier verifies)
```

| Boundary | Trust decision | Control |
|---|---|---|
| Build to signing | Only reviewed, gated builds are signed | Approvals; the key is not on the build host |
| Verne to customer | Channel is untrusted | Signatures; pinned root; digests |
| Customer registry to cluster | Only verified digests deployed | Admission policy (optional) |
| Licence issuance to installation | File integrity and entitlement | Ed25519, domain separation |

### 6.1 Artefacts per release

| Artefact | Format | Signed by |
|---|---|---|
| API image (includes `engine-cli`) | OCI image, `linux/amd64` and `linux/arm64` | Release key (cosign) |
| Web image | OCI image | Release key |
| Proxy image (bundled nginx with templates) | OCI image | Release key |
| Converter image (optional) | OCI image | Release key |
| Helm chart | OCI artefact | Release key |
| `dynodoctl` | Static binaries per platform | Release key |
| Offline bundle | Archive containing the above | Bundle manifest signature (6.4) |
| Add-in packages | Generated per installation | Not distributed centrally |
| SBOM per image | CycloneDX or SPDX JSON | Attached by cosign |
| Provenance statement per image | in-toto attestation (SLSA provenance) | Release key |
| Release notes, migration classes, vulnerability statements (VEX) | Markdown and JSON | In the bundle manifest |

### 6.2 Signing model

Two layers, so that customers need only what they use:

1. **Bundle manifest signature.** An Ed25519 signature over a canonical manifest
   (domain-separated, `dynodoc.bundle\0v1\0`, the same style as the repository's other
   signature domains). Verified by `dynodoctl` with no external tool. Covers every file
   digest in the bundle.
2. **Image signatures.** cosign key-based signatures on each image digest (and
   attestations for SBOM and provenance), for customers whose clusters enforce
   admission policy (Kyverno, Sigstore policy controller or similar) or who verify with
   the cosign tool.

Keyless signing (Fulcio certificate and Rekor log) is excluded for the offline use case:
it depends on public infrastructure, and a reported issue shows verification still
attempting to fetch trust roots over the network even with transparency-log checks
disabled in an air-gapped environment (a cosign GitHub issue found on 14 October 2026; confirm against the pinned cosign version).
Verne may additionally sign keyless for connected customers.

Verification examples (the flags below were seen in vendor air-gap documentation on 14
October 2026; **confirm against the cosign version pinned in the bundle**):

```sh
cosign verify --key cosign.pub --insecure-ignore-tlog=true \
    registry.internal/dynodoc/api@sha256:<digest>
cosign verify-attestation --key cosign.pub --type cyclonedx \
    registry.internal/dynodoc/api@sha256:<digest>
```

### 6.3 Key hierarchy

```
 Root key (offline, HSM, ceremony; fingerprint printed in contracts and on the site)
   |  signs a "release key certificate": key id, public key, not_before, not_after (<= 12 months)
   v
 Release key (KMS/HSM; used by the signing job after two-person approval)
   |  signs bundle manifests, image digests, attestations
   v
 Artefacts

 Licence issuing key (separate HSM; separate people; separate audit log)
   |  signs licence files (section 9)
```

| Property | Design |
|---|---|
| Pinning | Customers pin the root fingerprint once (from the contract pack or the previous bundle) and `dynodoctl` accepts only release certificates chained to it |
| Rotation | New release certificate per year with overlap; each bundle carries current and next certificates |
| Compromise | Root-signed revocation list shipped in the next bundle and published; customers who see no new bundle stay exposed until they import one (stated limitation) |
| Offline root | Used a few times a year; stored under dual control |
| Separation | Licence keys cannot sign artefacts and vice versa |
| Cloud KMS | The release key can live in a cloud KMS that supports Ed25519 (AWS KMS announced support in November 2025; Google Cloud KMS lists `EC_SIGN_ED25519`; both verified 14 October 2026) or an HSM via PKCS#11; cosign supports KMS URIs. Which KMS Verne uses is an owner decision (LQ8). |

### 6.4 Registry and channels

| Channel | Content | Audience |
|---|---|---|
| Download portal (static site) | Bundles, checksums, signatures, release notes, public keys, advisories | All customers |
| Public registry (optional) | Images and chart by digest for connected customers | A, B |
| Customer mirror | Imported from bundle or mirrored with `skopeo copy --all` / `crane` | B, C |

Customers deploy **by digest**. Tags are a convenience for humans; manifests and charts
reference digests.

### 6.5 Build pipeline requirements

| Requirement | Detail |
|---|---|
| Source | Committed revision only (as in `package-source.py`); dirty trees refused |
| Builds | Locked dependencies; pinned toolchain and base image digests; no network during compilation where feasible; no host compilation on customer machines |
| Reproducibility | Goal: two builds of the same revision produce identical image digests or identical layer contents with a published diff; measured and reported (P4-7) |
| Provenance | SLSA-style provenance for every image: builder identity, source revision, inputs |
| Scanning | Vulnerability scan of images and dependencies; findings reviewed; VEX statements for not-affected cases |
| Gates | Licence gate (5.2), SBOM completeness, test suites in both repositories, migration class check (master 6.9) |
| Approval | Two people approve the signing step; the signing key is not on the build host |

---

## 7. Offline bundles

### 7.1 Layout

```
dynodoc-1.2.0-linux.bundle/            (a tar archive; also a directory form)
  manifest.json                        canonical JSON; signed
  manifest.sig                         Ed25519 signature + release key certificate
  keys/root.pub, release-2027.cert, release-2028.cert
  oci/                                 OCI image layout (index.json, blobs/sha256/...)
  charts/dynodoc-1.2.0.tgz
  tools/dynodoctl-linux-amd64, dynodoctl-linux-arm64
  sbom/*.cdx.json
  attestations/*.intoto.jsonl
  notices/THIRD-PARTY-NOTICES.txt, SOURCE-OFFER.txt
  docs/RELEASE-NOTES.md, UPGRADE.md, deployment-guide.pdf
  revoked.json                         root-signed revocations (keys, licences, bundles)
```

### 7.2 Manifest (proposed)

```json
{
  "bundle_format": 1,
  "product": "dynodoc",
  "version": "1.2.0",
  "released_at": "2027-03-01T00:00:00Z",
  "platforms": ["linux/amd64", "linux/arm64"],
  "schema": {
    "migrations_through": 44,
    "min_upgrade_from": "1.0.0",
    "classes": {"0042": "expand", "0043": "expand", "0044": "expand"}
  },
  "components": [
    {"name": "api", "digest": "sha256:...", "layout": "oci/", "sbom": "sbom/api.cdx.json"},
    {"name": "web", "digest": "sha256:...", "layout": "oci/", "sbom": "sbom/web.cdx.json"},
    {"name": "proxy", "digest": "sha256:...", "layout": "oci/"},
    {"name": "converter", "digest": "sha256:...", "layout": "oci/", "optional": true}
  ],
  "charts": [{"name": "dynodoc", "version": "1.2.0", "sha256": "..."}],
  "tools": [{"name": "dynodoctl", "platform": "linux/amd64", "sha256": "..."}],
  "files": [{"path": "notices/THIRD-PARTY-NOTICES.txt", "sha256": "..."}],
  "release_key_id": "release-2027",
  "licence_verifier_keys": ["lic-2026-1", "lic-2027-1"]
}
```

### 7.3 Procedures

| Step | Command (proposed) | Checks |
|---|---|---|
| Build | CI job | Reproduced digest list, SBOM complete, notices generated |
| Verify | `dynodoctl bundle verify FILE` | Manifest signature chains to the pinned root; every file digest matches; platform compatible; `min_upgrade_from` satisfied; not revoked |
| Import | `dynodoctl bundle import FILE --registry HOST/PROJECT` | Copies images by digest into the internal registry; verifies digests after copy; records import |
| Stage | `dynodoctl upgrade --plan` | Migration classes, expected downtime (rolling or window) |
| Rehearse and activate | Deployment guide 9.8 | |

### 7.4 Size and transfer

Size is not known until the first build (track per release). The converter image
(LibreOffice) is expected to dominate and is therefore optional. Transfers go through the
customer's controlled path (data diode, scanned media); the SHA-256 of the archive is
recorded at both ends, and verification repeats after transfer.

---

## 8. Helm chart

| Item | Design |
|---|---|
| Distribution | OCI chart in the bundle and registry; signed with cosign; Helm 3.8 or later for OCI support (verified 14 October 2026 via vendor documentation) |
| Templates | Deployments for api, web, proxy (optional if the customer's ingress is used), converter (optional); Services; Ingress; migration Job (pre-install, pre-upgrade hook) with the owner secret mounted only there; `grant-runtime` and `check-runtime` steps; PodDisruptionBudgets; NetworkPolicies; ServiceMonitor (optional); ConfigMap for provider and mapping files; Secret references only |
| Values validation | JSON schema; `helm lint`; `kubeconform`; `helm unittest`; `helm test` hook running delivery checks |
| Hooks and downgrade | `helm rollback` does not undo schema changes; a pre-upgrade guard refuses a chart downgrade across any non-`expand` migration and prints the restore instruction |
| Security context | Non-root uid 10001 (API, converter) and the image's `node` user (web), read-only root filesystem, `capabilities.drop: [ALL]`, `seccompProfile: RuntimeDefault`, `automountServiceAccountToken: false` |
| Images | By digest from `global.imageRegistry` |
| Out of scope | A PostgreSQL subchart; customers use an external instance or an operator |

---

## 9. Licence key design

### 9.1 Principles

1. **Offline.** Verification needs only a public key shipped with the product. No
   activation server, no heartbeat.
2. **Fail soft.** An expired or missing licence never blocks reading, exporting,
   verifying, backing up or restoring data. It limits growth (new seats, new projects)
   and shows banners.
3. **Honest.** Enforcement is friction plus contract. The MIT engine cannot enforce
   anything, and a customer with root on the host can patch the web bundle. The licence
   states the entitlement; the EULA and audit rights provide the remedy.
4. **Minimal content.** A licence names the customer and entitlements; it contains no
   personal data of users.

### 9.2 File format

A single ASCII file `dynodoc.lic`:

```
DLIC1.<base64url(payload)>.<base64url(signature)>
```

`payload` is canonical JSON (sorted keys, compact UTF-8, the encoding rules the
repository already uses for device-enrolment and receipt signatures). `signature` is
Ed25519 over the bytes `"dynodoc.licence\0v1\0" || payload`. The domain label separates
licence signatures from every other signature the product handles. A fixed public test
vector (synthetic key seed, mirroring `device-enrollment-v1.json`) pins the encoding and
must not be regenerated to hide a mismatch.

### 9.3 Payload (proposed)

```json
{
  "v": 1,
  "licence_id": "01JB...ULID",
  "kid": "lic-2027-1",
  "customer": {"name": "Example University, Department of Biology", "contract_ref": "C-2027-014"},
  "edition": "organization",
  "kind": "production",
  "issued_at": "2027-03-01T00:00:00Z",
  "not_before": "2027-03-01T00:00:00Z",
  "expires_at": "2028-03-31T23:59:59Z",
  "grace_days": 30,
  "limits": {
    "seats": 1500,
    "seat_basis": "active_identities_90d",
    "projects": 400,
    "service_accounts": 25
  },
  "features": ["workspace", "addins", "oidc_multi", "saml", "scim", "group_mapping", "replicas", "audit_export"],
  "binding": {"installation": "2d1c0b9e-7f65-4f8e-8b0a-3a4a6a9f1c21"},
  "support": {"tier": "organization", "until": "2028-03-31"},
  "notes": "Design-partner terms apply"
}
```

| Field | Meaning |
|---|---|
| `kind` | `evaluation`, `production`, `non_production` (staging, DR drill) |
| `binding.installation` | Optional installation UUID (`device_authority` row). Absent for evaluation licences and for customers who prefer unbound files. Present binds the file to one database lineage; a disaster-recovery restore keeps the UUID by design, so the licence continues to apply. |
| `limits` | Entitlements; absent means unlimited for that field |
| `features` | Flags the web tier checks to enable routes and screens |
| `grace_days` | Days after `expires_at` before restrictions start |

### 9.4 Verification

| Where | When | How |
|---|---|---|
| Web tier | At start, hourly, and on file replacement | Parse; check domain-separated Ed25519 signature against the accepted `kid` list compiled into the build and extendable by bundle; check `not_before`/`expires_at` against the host clock; check binding against the installation UUID read from the API |
| Admin screen | On open | Shows state, entitlements, current usage, expiry, renewal instructions |
| Add-in manifest generator, SCIM provisioning, provider configuration | On use | Consult the cached state and feature flags |

Node's built-in `crypto.verify` supports Ed25519; no extra library is needed (verify at
build time against the pinned Node version).

### 9.5 States and behaviour

```
 no file ---> trial (30 days from first start, full features, banner) ---> restricted
 valid ---> expiring (30 days before expires_at: banner) ---> grace (expired, within grace_days: banner)
        ---> restricted (after grace)
 invalid signature / wrong binding / unknown kid ---> treated as no file, reason shown
```

| State | Allowed | Refused or limited |
|---|---|---|
| valid, expiring, grace, trial | Everything the features allow | Over-limit actions per seat/project limits are warned, not blocked, until restricted |
| restricted | All reading, editing, review, merge, export, verify, backup, restore, sign-in by existing accounts, deactivation, `engine-cli` | Creating new accounts through SCIM or JIT beyond the limit; creating new projects beyond the limit; enabling gated features that were not enabled before; the admin screen explains how to renew |
| never | Data lock-out, forced read-only mode, deletion, degraded integrity checks | |

A trial counted from first start is trivially reset by deleting state; it is a courtesy
and an evaluation convenience, not a protection. Evaluation licences issued as files
carry an expiry and are the managed route for design partners.

### 9.6 Issuance, renewal and revocation

| Step | Detail |
|---|---|
| Request (new or renewal) | Customer exports `licence-request.json` from the admin screen: installation UUID, product version, current usage counts, requested term. Unsigned; contains no secrets and no people data |
| Issue | Internal `dynodoc-licence` tool; the issuing key is in a separate HSM or KMS; two-person approval; screening step for sanctions (master 10.5); every issuance appended to an internal hash-chained log (the same append-only technique as the product) |
| Deliver | Email, portal download or inside a bundle; the customer places the file at `DYNODOC_LICENSE_FILE` or uploads it in the admin screen; the web tier verifies and swaps atomically; an audit row is written |
| Term | Up to 13 months so that a stolen or abandoned licence ages out without revocation infrastructure |
| Revocation | No online revocation. A root-signed `revoked.json` shipped in bundles lists revoked `licence_id`s; applied when a bundle is imported. Honest limit: a customer who never imports a bundle never learns of a revocation |
| Key rotation | The accepted `kid` list grows by bundle; old keys retire after all licences signed by them have expired |
| Lost file | Re-issue with the same `licence_id` and a new `issued_at` |

### 9.7 Tamper stance

The licence verifier is part of the proprietary web build. A customer administrator with
write access to the container filesystem or the Node bundle can disable it. Mitigations
are non-technical: contractual restrictions (EULA), audit rights, support only for
unmodified signed images (the bundle's image digests make modification visible), and the
fact that the proprietary value (add-ins, tooling, upgrades, support) continues only with
a current relationship.

---

## 10. Seat and project metering without phoning home

### 10.1 Definitions

| Term | Definition | Source of truth |
|---|---|---|
| Registered identity | `identity` rows with `kind = 'person'` and `erased_at is null` | Database |
| Enabled identity | Registered and `disabled_at is null` | Database |
| Active identity (window W) | Enabled, with an authenticated session refresh, sign-in or edit in the last W days | `login_session.last_refresh_at` and `event.created_at` by `actor_id` (the `login_session` table is proposed; until then `refresh_token.created_at` is a usable proxy) |
| Contributor (window W) | Distinct `event.actor_id` of non-service identities with at least one authored event in W days | `event` |
| Project | `access_space` with `kind = 'project'` not archived or in trash | Database |
| Service account | `identity.kind = 'service'` | Database |

The proposed default seat basis is the active identity over a 90-day window
(`seat_basis: active_identities_90d`, master Q4): viewers who never sign in cost
nothing; people who do, count, whether they edit or only read. The alternative bases
(contributors only, or registered) are values of the same field.

### 10.2 Queries (illustrative)

```sql
-- active identities in the last 90 days (after login_session exists)
select count(*) from identity i
 where i.kind = 'person' and i.disabled_at is null and i.erased_at is null
   and exists (select 1 from login_session s
                where s.identity_id = i.id and s.last_refresh_at > now() - interval '90 days');

-- contributors in the last 90 days
select count(distinct e.actor_id) from event e
  join identity i on i.id = e.actor_id and i.kind = 'person'
 where e.created_at > now() - interval '90 days';

-- projects
select count(*) from access_space where kind = 'project';
```

(Column names for archival state follow the project settings implementation and must be
checked at build time. The queries run as the runtime login, which has the needed
privileges.)

### 10.3 Usage report

The administrator screen shows current usage against `limits` and exports
`usage-report.json`. The export is explicit, local, and human-reviewable before
sending.

```json
{
  "report_format": 1,
  "generated_at": "2027-06-30T12:00:00Z",
  "product_version": "1.2.0",
  "installation": "2d1c0b9e-7f65-4f8e-8b0a-3a4a6a9f1c21",
  "licence_id": "01JB...ULID",
  "window_days": 90,
  "counts": {
    "registered": 2310,
    "enabled": 2204,
    "active": 1432,
    "contributors": 611,
    "service_accounts": 7,
    "projects": 318,
    "documents": 9120,
    "database_bytes": 38234112000
  },
  "limits": {"seats": 1500, "projects": 400}
}
```

It contains no names, emails, document titles or content. It is unsigned: Verne cannot
authenticate it, which is why the EULA carries audit rights rather than relying on report
integrity. A hash-chained local history of reports (append-only, as for other
operational records) lets the customer show consistency over time.

### 10.4 What is never measured

Feature usage per person, document titles, search terms, timings per user, IP addresses,
or anything sent automatically.

---

## 11. Telemetry policy

| Statement | Detail |
|---|---|
| Default | No telemetry of any kind leaves the installation. No update check. No crash reporting. |
| Anonymous page metrics | The hosted site's page-load metrics (`web_metric`) are inactive in self-hosted mode; the endpoint and CSP entry are removed (master D6) |
| Opt-in | The customer may enable a periodic local usage report file (10.3) and, separately, may attach a support bundle to a ticket |
| Support bundle contents | Version and image digests, configuration with all secrets and hostnames replaced by stable placeholders, migration state, counts (10.3), table sizes, last N log lines at warning level with identifiers hashed, resource limits, failed health checks. Never: documents, titles, emails, tokens, cookies, keys, IPs. The customer reviews the file before sending |
| Retention by Verne | Support bundles deleted after the ticket closes plus a stated period (decision LQ9) |
| Verne-hosted services | The download portal logs access as any web server does; the policy page says so |
| Change control | A new data category requires a documented decision and a release note |

---

## 12. EULA and legal-review checklist

Not legal advice. A checklist of what the agreement and supporting documents must decide.

| # | Topic | Question or proposal | Recommended default | Decider |
|---|---|---|---|---|
| 1 | Licensor | Which entity grants the licence: Verne Ltd. (copyright holder) or Verne Systems (operator and trading name) | Counsel aligns; the licensor must hold or be authorised to sublicense the rights | Owner, counsel |
| 2 | Grant | Non-exclusive, non-transferable, term-limited, internal use; installations (production, non-production, DR); edition and limits | As stated | Counsel |
| 3 | MIT carve-out | Explicit statement that MIT components are licensed under MIT and not restricted by the EULA | Yes | Counsel |
| 4 | Restrictions | No resale, service-bureau use, benchmarking publication limits (if any), reverse engineering beyond law, circumvention of the licence mechanism | Standard | Counsel |
| 5 | Third-party software | Notices, precedence of third-party licences, source offer text | Reference the generated notices | Counsel |
| 6 | Modification | Support only for unmodified signed images | Yes | Owner |
| 7 | Customer data | Customer owns it; Verne has no access by default; return and deletion on exit | Yes | Counsel |
| 8 | Support access | When Verne personnel access content: DPA, location, confidentiality, customer-initiated only | Customer-initiated sessions only | Owner, counsel |
| 9 | Data protection | DPA template; no sub-processors in the default model; international transfer analysis if support access is used from outside the customer's jurisdiction | Template ready (P4-6) | Counsel |
| 10 | Security | Vulnerability handling, duty to apply patches within a reasonable time, advisory channel | Yes | Owner |
| 11 | Warranty | Disclaimer scope; no capacity, availability or compliance warranties; no fitness claims | As stated in master 10.6 | Counsel |
| 12 | Liability | Caps, exclusions; carve-outs required by law | Counsel | Counsel |
| 13 | IP indemnity | Whether offered, and exclusions for open-source and modifications | Limited | Counsel |
| 14 | Audit rights | Usage verification once a year with notice; customer-side self-certification with the usage report | Self-certification first | Owner |
| 15 | Term and renewal | Term up to 13 months per licence file; grace period; restricted-mode description | As 9.5 | Owner |
| 16 | Termination | Effect on use; data export remains possible; return or destruction of keys and files | As 9.5 | Counsel |
| 17 | Fees and taxes | Out of scope here | n/a | Owner |
| 18 | Export controls and sanctions | Customer warranties; Verne's screening; classification statement | After classification (10.5) | Counsel |
| 19 | Escrow | Source escrow option for Profile C | Available by agreement | Owner |
| 20 | Governing law and forum | The hosted drafts propose Bangladeshi law and Dhaka courts as a "plain term of the draft"; enterprise and academic customers outside Bangladesh may require a neutral seat and law | Counsel proposes neutral arbitration option | Owner, counsel |
| 21 | Language and documents | Order of precedence among EULA, order form, DPA, support terms | Standard | Counsel |
| 22 | Trademarks | Naming and logo use, fork naming policy | Policy (LQ6) | Counsel |
| 23 | Publicity and references | Whether Verne may name the customer | Opt-in | Owner |
| 24 | Design-partner and beta terms | Shorter terms, feedback licence, no warranty | Separate agreement | Owner, counsel |
| 25 | AI | Statement that the product sends no content to AI services; a placeholder to revisit when AI-agent connectors exist (a separate design) | Yes | Owner |
| 26 | Accessibility | Statement of goals (WCAG 2.2 AA), known gaps, VPAT or ACR when available | P4-4 | Owner |
| 27 | Assignment, change of control | Standard | Counsel | Counsel |
| 28 | CRA and regulatory notices | Customer notification channel for actively exploited vulnerabilities | Advisory list | Owner, counsel |

The existing hosted legal pages are not reusable as a self-hosted EULA: they describe a
service Verne operates, name Verne Systems as operator and controller, list Resend and
Vercel, and are marked as drafts "not yet effective" by owner instruction
([LEGAL-PAGES.md](LEGAL-PAGES.md)). Their facts-omitted-until-decided policy applies
equally here: undecided points are left out of customer-facing documents, not shown as
placeholders.

---

## 13. Support model and security response

### 13.1 Position

**No service-level agreement is promised.** The model below describes what Verne does,
in what order, and what it will not do. Response and resolution times are not
commitments; they may be published as observed statistics after at least twelve months of
data (LQ10). This follows the owner's direction.

### 13.2 Scope

| In scope | Out of scope |
|---|---|
| Installation, configuration, upgrade questions for supported releases | The customer's infrastructure, network, IdP, directory, mail relay, KMS |
| Defects in unmodified signed Verne artefacts | Modified images or forks of the engine |
| Security vulnerabilities in supported releases | Releases outside the support window |
| Documentation errors | Microsoft Office or Google editor behaviour; third-party add-in hosts |
| Licence issuance and renewal | Custom development |

### 13.3 Channels and process

| Item | Detail |
|---|---|
| Channel | A single support mailbox at first (the operator's published address, `info@vernesystems.com`); a ticket portal when volume warrants. Engine issues: the public repository's issue tracker |
| Intake | Release version and image digests, edition, profile, steps to reproduce, redacted support bundle (section 11) |
| Triage | Severity assigned by Verne with the customer's input (13.4); duplicate detection against a knowledge base |
| Resolution | Workaround, configuration change, patch release, or documentation fix |
| Remote access | None by default. Customer-driven screen sharing on request. No standing access, no jump host |
| Hours | Business hours in Bangladesh (UTC+6; the working week is commonly Sunday to Thursday, **to be confirmed by the owner**), which leaves limited overlap with US time zones |
| Language | English |
| Escalation | Named engineering contact for Enterprise customers |
| Closure | Customer confirmation or a stated lapse period |

### 13.4 Severity definitions (for prioritisation, not for promises)

| Class | Definition | Examples |
|---|---|---|
| S1 | Data integrity or confidentiality at risk, or total outage with no workaround | Chain verification fails; authentication bypass; all users blocked |
| S2 | Major function unusable for many users | SSO sign-in failing; live updates broken; upgrade fails mid-way |
| S3 | Function degraded or workaround exists | Add-in pane fails on one Office build; slow search |
| S4 | Question, documentation, cosmetic | |

### 13.5 Security response (PSIRT)

| Step | Detail |
|---|---|
| Intake | `security.txt` contact; encrypted mail option; acknowledgement as a goal, not a promise |
| Assessment | Reproduce, assign severity (CVSS or equivalent), identify affected supported releases |
| Fix | Patch releases for the current and previous minor (master 12.1); engine fixes also published to the public repository |
| Disclosure | Advisory with affected versions, fixed versions, workaround, credit; delivered through the download portal and the advisory mailing list; machine-readable form (CSAF or similar, unverified) |
| Embargo | Short coordination with reporters and, for engine issues, downstream users |
| Regulatory | EU Cyber Resilience Act reporting of actively exploited vulnerabilities and severe incidents to the EU reporting platform from 11 September 2026 (24-hour early warning, 72-hour notification, final report) where Verne is a manufacturer or steward; counsel and owner determine applicability (master 10.5) |
| Customer duties | Subscribe to advisories; apply patches within their own change process |

### 13.6 What customers should plan for

Operating the product is the customer's responsibility. Verne provides documentation,
tooling and fixes; it does not operate, monitor or recover customer installations.
Customers should keep a staging environment, run rehearsals before upgrades, test
restores, and maintain a contact for advisories.

---

## 14. Security analysis of the licensing and distribution channel

| Threat | Impact | Control | Residual |
|---|---|---|---|
| Malicious or tampered bundle | Total compromise of the installation | Signed manifest and images, pinned root, digests, offline verification, rehearsal | Key compromise |
| Compromised build pipeline | Signed malicious artefact | Two-person signing approval, key outside CI, provenance, reproducibility goal, SBOM diffing between releases | Insider collusion |
| Signing key compromise | As above | Offline root, short-lived release certificates, revocation list in next bundle | Customers without a new bundle |
| Dependency compromise | Backdoor | Locked versions, scanning, review of dependency bumps, offline mirror of audited sources | Zero-day in a dependency |
| Licence forgery | Free use | Ed25519 signature, separate key, `kid` list | Binary patching |
| Licence theft and reuse | Use beyond entitlement | Binding optional, term limit, audit rights | Unbound files copyable |
| Denial through licence | Customer locked out of data | Fail-soft design; never block read/export | None by design |
| Support bundle leaks data | Confidentiality | Redaction rules, customer review, retention limit, no bundle without customer action | Redaction bugs |
| Download portal compromise | Malicious files | Detached signatures, pinned keys; a portal compromise alone is insufficient | Users skipping verification |
| Sanctions or export violation by licence issuance | Legal | Screening step, classification | Counsel |

---

## 15. Operations

### 15.1 Verne-side

| Process | Owner | Cadence |
|---|---|---|
| Release | Release engineer | Quarterly minor; patch releases as needed |
| Key ceremony | Owner plus a second custodian | Root: rare; release certificate: annual |
| Licence issuance | Licence administrator plus approver | On request; log reviewed monthly |
| Advisory publication | Security lead | As needed |
| Third-party audit | Release engineer | Every release (gate) |
| Download portal | Release engineer | Every release |

### 15.2 Customer-side

| Task | Procedure |
|---|---|
| Install a licence | Place the file or upload; check the admin screen state |
| Renew | Export the request, send, receive the new file, replace |
| Review usage | Admin screen or exported report; reconcile before renewal |
| Import a bundle | Deployment guide 9.5 |
| Monitor | `dynodoc_license_days_remaining`; alerts in deployment guide 10.3 |

### 15.3 Failure modes

| Failure | Behaviour | Action |
|---|---|---|
| Clock set backwards or far ahead | Licence appears valid longer or expired early | The web tier records the highest seen time and warns; no blocking beyond restricted mode |
| Wrong installation binding after a rebuild with a new database | State `invalid_binding`, treated as no file | Request a new file; explain the UUID |
| Expired licence in an air-gapped site with a long approval chain | Grace days; restricted mode only prevents growth | Plan renewal early; 30-day banner |
| `kid` unknown | Treated as invalid | Import the latest bundle |

---

## 16. Delivery plan

Phase 3 of the master plan (13.5). Basis and assumptions are master 13.1 (person-week of
focused work, a team that knows the repository). **Excluded:** counsel and notary fees,
KMS or HSM hardware or service charges, download-portal hosting and bandwidth, external
audit and certification fees, support staffing, and the formula-engine replacement
that removed `hyperformula` (done). Additional tasks from other phases that this document
depends on are listed.

| ID | Task | PW | Conf. | Depends on | Acceptance |
|---|---|---|---|---|---|
| P0-6 | Image pipeline (multi-arch, digests, no host compilation, SBOM) | 2.0 | M | none | Images build in CI; SBOM emitted |
| P3-1 | Release pipeline: signing (KMS/HSM), provenance, SBOM publication, vulnerability scanning, notices generation, two-person approval | 5.0 | M | P0-6 | A release produces signed artefacts verifiable offline; notices complete; gates demonstrably fail on a seeded violation |
| P3-2 | Offline bundle builder, `bundle verify` and `bundle import`, registry mirroring guide | 3.0 | M | P3-1 | Tamper test per file class fails; import by digest into a registry mirror; air-gap run passes |
| P3-3 | Licence format, verifier, issuer tool, admin screen, grace behaviour, test vectors | 4.0 | M | none | State machine tests; independent vector; fail-soft verified: restricted mode still exports and restores |
| P3-4 | Metering views and usage report | 2.0 | H | P1-6 | Queries match fixtures; report contains no personal data (automated check) |
| P3-5 | Support bundle collector with redaction and review step | 2.0 | H | P0-5 | Redaction tests with seeded secrets, emails, hostnames |
| P3-6 | Third-party licence audit tool and CI gate | 1.5 | H | none | Seeded GPL dependency fails the build; notices reproducible |
| P3-7 | Download and documentation portal (static), per-version docs | 2.0 | H | P3-1 | Signed index; offline copy of docs inside the bundle |
| P3-8 | Opt-in telemetry export and policy text | 1.0 | H | P3-4 | Default off verified by egress test |
| | **Phase 3 total** | **20.5** | | | |
| P4-2 | PSIRT process and advisory publication | 2.5 | M | P3-1 | Tabletop exercise |
| P4-7 | Reproducible builds | 2.5 | L | P3-1 | Two independent builds compared; differences listed |

**Test strategy.** Seeded negative tests for every gate; a clean-room machine verifying
a bundle with only the public root; licence vectors generated by an independent tool
(OpenSSL) as for other fixtures in the repository; an egress-blocked pilot.

**Rollout and migration.** Licence enforcement code ships disabled in `hosted` mode
(Verne's own service has no licence). The first design-partner licences are evaluation
files with generous limits. No existing hosted data is affected.

---

## 17. Risks and open questions

### 17.1 Risks

| ID | Risk | Mitigation |
|---|---|---|
| LR1 | A GPL or AGPL transitive dependency reaches a distributed artefact | Gate, transitive audit, counsel exception list |
| LR2 | Signing key custody failure (loss or theft) | Offline root, documented ceremony, revocation list, overlap rotation |
| LR3 | Licence file mishandling locks a customer in practice | Fail-soft states; no lock-out; clear renewal instructions; grace |
| LR4 | Customers dislike the absence of an SLA | State support model plainly; offer assisted upgrades; revisit with data |
| LR5 | Office.js redistribution terms block the offline Office path | Customer-supplied copy option; engage Microsoft or counsel |
| LR6 | CRA applicability and obligations misjudged | Counsel determination before first customer |
| LR7 | Bundles are large and slow to move into restricted networks | Optional converter; delta bundles considered later |
| LR8 | Maintenance of several supported versions exceeds capacity | Two-minor window; automation of backports |
| LR9 | Licence design leaks personal data | Payload content rules; automated check on reports |

### 17.2 Open questions (licensing-specific; shared questions are in the master)

| # | Question | Recommended default | Decider |
|---|---|---|---|
| LQ1 | Evaluation length and whether a built-in trial exists | 30-day built-in trial plus evaluation files | Owner |
| LQ2 | Licence term | Up to 13 months | Owner |
| LQ3 | Binding to installation UUID by default | Optional; on for Enterprise on request | Owner |
| LQ4 | Is a hosted licence portal needed or is email enough | Email first | Owner |
| LQ5 | Public registry in addition to bundles | Yes for A and B if costs allow; digests only | Engineering lead |
| LQ6 | Trademark and fork-naming policy | Written policy before public bundle release | Counsel |
| LQ7 | Contribution terms for the public engine repository | Verify existing files; decide CLA or DCO | Owner, counsel |
| LQ8 | Cloud KMS or HSM for release and licence keys | HSM or KMS in an independent account, never on build hosts | Owner |
| LQ9 | Support bundle retention at Verne | Delete at ticket close plus 30 days | Owner |
| LQ10 | When to publish response-time statistics | After 12 months of data | Owner |
| LQ11 | Source escrow provider and trigger events | By agreement for Enterprise | Owner, counsel |
| LQ12 | Reseller and partner distribution of bundles | Not in v1 | Owner |

---

## 18. Appendices

### Appendix A: release signing procedure (proposed)

1. Release candidate tagged; CI builds artefacts from the tag; gates pass.
2. Release engineer prepares the manifest; a second person reviews the diff of SBOMs,
   migration classes and notices against the previous release.
3. The signing job requests the release key operation through the KMS using a short-lived
   credential issued after two approvals.
4. Signatures and attestations published; manifest signed; bundle assembled.
5. A clean-room verification job downloads the artefacts and verifies them with only the
   root public key.
6. Portal updated; advisory and release notes published.

### Appendix B: support bundle structure (proposed)

```
support-bundle-<date>.tar
  README.txt                      what is inside and how to review it
  versions.json                   release, image digests, edition, licence id (not file)
  config.redacted.yaml            keys replaced by placeholders; hostnames by stable aliases
  database.json                   migration list, table sizes, row counts, settings subset
  health.json                     /healthz, /readyz, check-runtime result, listener state
  resources.json                  limits, usage, replica counts
  logs.redacted.jsonl             warnings and errors, identifiers hashed
  usage-report.json               optional
```

### Appendix C: glossary

| Term | Meaning |
|---|---|
| Attestation | Signed statement about an artefact (SBOM, provenance) |
| Bundle | Signed archive of images, chart, tools and metadata for one release |
| `kid` | Key identifier of a signing key |
| PSIRT | Product security incident response team or process |
| SBOM | Software bill of materials |
| SLSA | Supply-chain Levels for Software Artifacts framework |
| VEX | Vulnerability exploitability exchange statement |

### Appendix D: references

| Topic | Reference | Status |
|---|---|---|
| Cosign key-based and offline verification practice | https://docs.sigstore.dev/cosign/verifying/verify/ ; vendor air-gap documentation seen 14 Oct 2026 (Element, Mirantis) | Verified partially; flags to confirm against the pinned version |
| AWS KMS Ed25519 | https://aws.amazon.com/about-aws/whats-new/2025/11/aws-kms-edwards-curve-digital-signature-algorithm/ | Verified |
| Google Cloud KMS algorithms | https://docs.cloud.google.com/kms/docs/algorithms | Verified |
| Helm OCI support | https://helm.sh/docs/topics/registries/ | Version statement verified via search; page not fetched |
| EU Cyber Resilience Act reporting | https://digital-strategy.ec.europa.eu/en/policies/cra-reporting | Verified via summaries |
| Microsoft add-in deployment channels | See master appendix C | Verified |
| Office.js packaging and licence ambiguity | See master appendix C | Verified as ambiguous |
| FERPA school-official exception guidance | https://studentprivacy.ed.gov/faq/who-school-official-under-ferpa | Verified |
| Ed25519 | RFC 8032 | Standard; not re-fetched |
| CycloneDX, SPDX, in-toto, SLSA | Project specifications | Unverified; check at build time |
| CSAF | OASIS CSAF 2.0 | Unverified |
