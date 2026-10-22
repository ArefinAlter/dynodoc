# Dynodoc self-hosted edition: deployment guide

> **Status: design, not implemented.** 14 October 2026. This guide describes how a
> customer would deploy, operate, back up, upgrade and recover a self-hosted Dynodoc
> installation. Commands marked "proposed" belong to tooling that does not exist yet
> (`dynodoctl`, the Helm chart, the signed bundle). Commands that use only
> `docker compose`, `engine-cli`, `pg_dump` and `nginx` match what the repository and
> the production release records contain today.
>
> Read first: the [master design](SELF-HOSTED-EDITION.md). Companions:
> [identity and access](SELF-HOSTED-IDENTITY-AND-ACCESS.md),
> [licensing and distribution](SELF-HOSTED-LICENSING-AND-DISTRIBUTION.md).

Contents

1. Purpose, audience, goals, non-goals
2. Current state versus target
3. Reference topologies
4. Prerequisites and supported platforms
5. Network, DNS and TLS
6. Sizing
7. Configuration reference
8. Database
9. Operator runbooks
10. Observability
11. Failure modes and troubleshooting
12. Security hardening checklist
13. Delivery tasks, acceptance and test strategy
14. Risks and open questions
15. Appendices

---

## 1. Purpose, audience, goals, non-goals

**Purpose.** Give a customer's IT and security staff, and Verne engineers, one place
that says what to run, how it connects, what to configure, what to back up, and what
to do when something fails.

**Audience.** Platform and infrastructure engineers (profiles B and C), a generalist
administrator (profile A), security reviewers, and the engineers who will build the
tooling this guide assumes.

**Goals.**

1. Three reference topologies that match the profiles A, B and C of the master design.
2. A complete configuration reference, grounded in the variables the code reads today
   and clearly separated from proposed ones.
3. Runbooks for install, backup, restore, upgrade with rehearsal, rollback, disaster
   recovery, key and certificate rotation and decommissioning.
4. A statement of what is monitored, what fails how, and which defects are known.

**Non-goals.** Training material for PostgreSQL, Kubernetes or Nginx; vendor-specific
cloud guides; a performance guarantee (section 6 is arithmetic, not a promise); a
hosted offering run by Verne; instructions for any Google-hosted component.

**Personas and use.** Profile A reads sections 3 (A), 7 (A column), 9.1, 9.6 to 9.8.
Profile B adds sections 5, 8, 10. Profile C reads everything and sections 9.4 and 12 in
detail.

| Persona | Uses this guide to |
|---|---|
| Generalist administrator (A) | Install, back up, restore, upgrade without database knowledge |
| Platform or SRE engineer (B, C) | Build the topology, wire monitoring, run drills, plan upgrades |
| Security reviewer (C) | Check egress, secrets, hardening, failure behaviour |
| Verne engineer | Know what the tooling must do and what is already scripted |

**Success metrics (targets to validate, not measured today).**

| Metric | Target |
|---|---|
| Time from empty VM to first sign-in, Profile A, documentation only | 60 minutes or less (median) |
| Restore drill from backup file to verified service | 4 hours or less for A and B (documentation only) |
| Runbook steps that need a command not in this guide | 0 |
| Upgrade rehearsals passing on first attempt | 95% or more |
| Unexpected outbound connections in air-gap mode | 0 |

---

## 2. Current state versus target

Tags: EXISTS, PARTIAL, MISSING. "Today" is commit `994c429`, 14 October 2026.

| # | Capability | Tag | Evidence and limitation |
|---|---|---|---|
| 1 | Hardened containers for API, web, converter, PostgreSQL | EXISTS | [`infra/demo/compose.yml`](../infra/demo/compose.yml): `read_only`, `cap_drop: [ALL]`, `no-new-privileges`, `tmpfs /tmp`, memory/CPU/PID limits, two `internal: true` networks |
| 2 | Compose usable outside the production VPS | PARTIAL | Hard-coded admin origin and workspace URL, `127.0.0.1:3088/3089`, `../../ops`, builds from source |
| 3 | Dockerfiles | EXISTS | `Dockerfile.api` (Rust 1.96 build, `debian:bookworm-slim`, uid 10001, installs `engine-api` and `engine-cli`), `Dockerfile.web` (Node 24, pnpm 10.34.5, `BODY_SIZE_LIMIT=16M`, user `node`), `infra/converter/Dockerfile` (LibreOffice Writer, uid 10001) |
| 4 | Published images, SBOM, signatures | MISSING | |
| 5 | Reverse-proxy configuration | PARTIAL | `nginx-app.conf`, `nginx-admin.conf`, `nginx-https.conf`; hostnames and certbot paths fixed; shared-host assumptions |
| 6 | Operator migration and grants as a separate profile | EXISTS | `database-admin` service under profile `maintenance`; `engine-cli migrate`, `grant-runtime`, `check-runtime` |
| 7 | Receipt authority secret mount | EXISTS | `compose.schema38.override.yml` mounts `REF_AUTHORITY_KEY_FILE` at `/run/secrets/ref_authority` |
| 8 | Backup | PARTIAL | `backup.sh`: `pg_dump -Fc`, listing check, SHA-256, 14-day retention; same host; unencrypted |
| 9 | Restore and drill tooling | MISSING | Procedure only, in prose |
| 10 | PITR / physical backup | MISSING | |
| 11 | Rehearsal and activation scripts | PARTIAL | Exist in private release directories; not in the repository |
| 12 | Health endpoints | PARTIAL | `/healthz` on web and API (liveness only) |
| 13 | Metrics, structured logs | MISSING | `tracing` text output only |
| 14 | Status page for operators | PARTIAL | `/admin` overview from `/admin/overview` plus host JSON from `collect-status.py` |
| 15 | Kubernetes packaging | MISSING | |
| 16 | HA for PostgreSQL | MISSING | Single container with a named volume |
| 17 | Air-gapped install path | MISSING | |

Target: all MISSING items delivered per the phases in the
[master design, section 13](SELF-HOSTED-EDITION.md), with the Phase 0 items required
before any customer installs.

---

## 3. Reference topologies

### 3.1 Profile A: single node

```
 Internet or LAN
       |
   :80 / :443
 +-----v-----------------------------------------------+
 | Host (Linux VM, Docker Engine + Compose v2)           |
 |                                                       |
 |  proxy (nginx)   -- net: edge                         |
 |      |                                                |
 |  web (SvelteKit) -- nets: edge, engine, conversion    |
 |      |        \                                       |
 |  api (Rust)    converter (optional) -- net: conversion|
 |      |                                                |
 |  postgres 16 -- net: engine (internal)                |
 |      |                                                |
 |  volumes: database, secrets, backups                  |
 +-------------------------------------------------------+
   outbound (optional): SMTP relay, OIDC IdP
   backups: /var/lib/dynodoc/backups -> other disk / NFS / S3-compatible target
```

| Process | Count | Notes |
|---|---|---|
| proxy | 1 | Terminates TLS; bundled nginx; only container with published ports |
| web | 1 | Serves workspace and `/admin`; optional second instance for the admin host is not needed |
| api | 1 | `API_HOST=0.0.0.0`, internal network only |
| postgres | 1 | Bundled `postgres:16`; data volume on fast local disk |
| converter | 0 or 1 | Optional; needed only for legacy `.doc` import |
| database-admin | on demand | Profile `maintenance`; runs `engine-cli` with the owner URL |

*As built (22 October 2026, P0-4; `infra/selfhosted`, README there).* The diagram is kept, with
these differences. There are five networks: `published` (the proxy alone; the only one Docker
publishes a port from), `frontend` (internal; the proxy and the web server, so the proxy has no
route to the API or the database), `engine` (internal; web, API, PostgreSQL and the jobs),
`conversion` (internal; web and the optional converter) and `egress` (the web server, and the ACME
client when it runs; remove the web server from it with `compose.no-egress.yml`). Four one-shot jobs
run on every `up` and stop the stack if they fail: `secrets-init`, `preflight`, `db-roles`, `init`.
`database-admin` and `mailcheck` (the real SMTP handshake) are on-demand services of profile
`maintenance`. Details: README of `infra/selfhosted`.

### 3.2 Profile B: replicated, external PostgreSQL

```
  users ---> [ load balancer / WAF, TLS ] ---+---> web-1 ---+
                                             +---> web-2 ---+--> api-1 ---+
                                                            +--> api-2 ---+--> PostgreSQL primary
                                                                              |   streaming
                                                                              +-> standby
  operator host: dynodoctl / engine-cli (owner credential) -> primary
  backup: pgBackRest or equivalent -> repository (other site or object store)
  monitoring: Prometheus scrapes web:9464, api:9464; logs -> central store
  identity: OIDC/SAML IdP; mail: SMTP relay
```

| Process | Count | Notes |
|---|---|---|
| Load balancer | 1 to 2 | Customer's existing; SSE settings in master 6.7 |
| web | 2 | Same `SESSION_SECRET`, `WEB_SERVICE_KEY`; sticky routing until P2-3 |
| api | 2 | Same `PASETO_LOCAL_KEY`; set `DYNODOC_EVENT_FANOUT=postgres` once P2-1 exists; before that, single API replica or accept stale streams |
| PostgreSQL | 1 primary, 1 standby | Managed service or self-run; WAL archiving to the backup repository |
| Operator host | 1 | Where `dynodoctl` and migrations run; not a long-running service |

Variant B2 on Kubernetes (small cluster): same shape with Deployments, an Ingress and
a Job for migrations; PostgreSQL stays outside the cluster or under an operator
(CloudNativePG, Zalando or similar; customer's choice).

### 3.3 Profile C: highly available, intranet or air-gapped

```
 Site 1                                              Site 2 (DR)
 +------------------------------------------+        +---------------------------+
 | LB pair (internal CA)                    |        | warm standby (optional)   |
 |  web x3   api x3 (+ job lease)           |        |  PostgreSQL async standby |
 |  PostgreSQL: primary + sync standby      |------->|  WAL archive copy         |
 |  + third node / witness                  |        |  restore runbook ready    |
 |  secrets: Vault / K8s secrets (CSI)      |        +---------------------------+
 |  KMS/HSM: receipt authority (optional)   |
 |  SIEM <- audit + application logs        |
 |  registry mirror <- signed offline bundle|
 +------------------------------------------+
```

| Concern | Choice |
|---|---|
| Orchestrator | Kubernetes 1.29+ (Helm chart) or three VMs with systemd and Compose |
| PostgreSQL | Three-node Patroni, or an operator; one synchronous standby; fencing; `synchronous_commit` per policy |
| Ingress | Customer's L7; WAF exceptions for JSON patches and SSE |
| Secrets | External secret store; no secrets in Helm values or images |
| Egress | None; deny by default; IdP, SMTP, SIEM, KMS and registry are internal |
| Updates | Offline bundle, verified, imported to an internal registry, rehearsed on staging |
| Clock | Internal NTP; skew tolerance is seconds (tokens, challenges) |
| Observability | Metrics, logs and audit events into the customer's SIEM |

### 3.4 Component, port and flow matrix

| From | To | Port | Protocol | Purpose | Required |
|---|---|---|---|---|---|
| Users | proxy / LB | 443 (80 redirects) | HTTPS | App, add-in panes, packages | Yes |
| proxy | web | 3000 (container) | HTTP | Upstream | Yes |
| web | api | 8080 | HTTP | Gateway and sign-in exchange | Yes |
| web | converter | 8090 | HTTP | `.doc` conversion | Optional |
| api | PostgreSQL | 5432 | TCP, TLS recommended | All data | Yes |
| api listener | PostgreSQL | 5432 | TCP, session mode | LISTEN (proposed) | B, C with replicas |
| web | IdP | 443 | HTTPS | OIDC discovery, token, userinfo, JWKS | If SSO |
| IdP | web | 443 (inbound) | HTTPS | SCIM, back-channel logout | If SCIM |
| web | SMTP relay | 25/465/587 | SMTP(S) | Sign-in links, invitations | If email |
| web | LDAP | 636 | LDAPS | Directory lookup (optional feature) | If LDAP |
| Prometheus | web, api | 9464 | HTTP | Metrics (proposed) | B, C |
| operator host | PostgreSQL | 5432 | TCP | Migrations, backups, drills | Yes |
| Office clients | LB | 443 | HTTPS | Add-in panes and icons | If add-ins |
| engine-cli on endpoints | LB | 443 | HTTPS | `sync` | If CLI used |

### 3.5 Trust boundaries and data flows (deployment view)

| Boundary | Between | Authentication | Exposure |
|---|---|---|---|
| Edge | Users and the proxy or load balancer | TLS server certificate; anonymous pages | Internet or intranet |
| Application | Proxy to web; web to API | Network isolation; `x-dynodoc-service-key`; bearer tokens | Internal networks only |
| Data | API to PostgreSQL | Restricted runtime login; TLS (B, C) | Internal network only |
| Operator | Operator host to PostgreSQL and registry | Owner credential; signed artefacts | Management network |
| Identity | Web to IdP, SMTP relay | TLS; client secret | Egress to named hosts only |
| Backup | PostgreSQL to backup repository | Repository credentials; encryption | Separate failure domain |
| Endpoint | Office hosts, browsers, `engine-cli` | Session cookie, connector key, device key | User devices |

Data at rest: PostgreSQL volume (content, history, identities, audit, originals),
WAL archive and dumps (everything), secrets store, proxy logs (client addresses),
endpoint-local add-in ledgers. No other component stores user data; the proxy, web and
converter hold only transient request data (converter files live in `tmpfs`).

---

## 4. Prerequisites and supported platforms

Supported platforms are a proposal (master Q6).

| Item | Requirement |
|---|---|
| OS | Linux x86-64 or arm64, a distribution with a supported Docker Engine or containerd |
| Container runtime | Docker Engine with Compose v2 for A; Kubernetes 1.29 or newer for B2/C. Podman is possible but not tested. |
| PostgreSQL | 16 (the CI service image is `postgres:16`; no other major version is tested). Extensions `pgcrypto` and `pg_trgm` (migration 0000). Both are shipped in the standard contrib set; the operator login must be allowed to `CREATE EXTENSION` (on managed services check that both are on the allowlist). Database timezone UTC recommended (the schema uses `timestamptz`). |
| Separate roles | An owner/migration login and a dedicated non-privileged runtime login. The API refuses owner, superuser, role members, DDL or temporary-object privileges ([DATABASE-PRIVILEGES.md](DATABASE-PRIVILEGES.md)). Shared databases are not a supported deployment contract. |
| Time | NTP-synchronised clocks on all nodes; skew above a few seconds breaks token `nbf`/`exp` and 120-second device challenges |
| TLS | A certificate trusted by every client: browsers, the Office host, `engine-cli` (rustls), and the Node web process (`NODE_EXTRA_CA_CERTS`) when it calls an IdP behind a private CA |
| Disk | See section 6; separate volumes for database, WAL archive and backups |
| Outbound | None required by the product (section 5.4 lists what each optional feature needs) |
| Browser | Current evergreen browsers; the workspace uses CSP with nonces and no `eval` |
| Office | Word needs WordApi 1.3, Excel ExcelApi 1.13, PowerPoint PowerPointApi 1.4; PowerPoint 2021 does not qualify ([INSTALL.md](../extensions/INSTALL.md)) |

---

## 5. Network, DNS and TLS

### 5.1 Names and origins

| Choice | Description | Use |
|---|---|---|
| Single origin (default for self-hosted) | One name, for example `docs.example.org`, serves the workspace, `/admin`, add-in panes and packages | A, B |
| Split origin | `docs.example.org` for the workspace and `admin.docs.example.org` for the admin surface (second web process with `ADMIN_SITE=true`, as in production) | C, or where cookie separation and a separate rate-limit zone are wanted |

The web process builds request URLs from `ORIGIN` (adapter-node), and the gateway and
authentication routes reject any non-GET request whose `Origin` header differs from that
origin (`routes/api/engine/[...path]/+server.ts`, `routes/auth/[...action]/+server.ts`),
so `ORIGIN` must equal the public URL exactly (scheme, host, no trailing slash, port only
if non-default). Cookies are
host-scoped. Add-in manifests, OIDC redirect URIs, SAML ACS URLs and `engine-cli`
remote files all embed the origin, which is why the hostname is the single hardest thing
to change later (runbook 9.9).

### 5.2 Certificates

| Mode | When | Notes |
|---|---|---|
| ACME (HTTP-01 or DNS-01) via certbot or lego | Public name | The current production proxy answers `/.well-known/acme-challenge/` from a webroot. **Built** in the bundle: lego v5.5.2, HTTP-01 only, `DYNODOC_TLS_MODE=acme`; no default authority; terms must be accepted in `.env` |
| Customer-provided certificate and key (internal CA) | Intranet, air-gap | Provide full chain; reload the proxy on renewal. **Built**: `DYNODOC_TLS_MODE=provided`; the proxy checks key, dates, names and chain order and refuses to start otherwise; a replaced certificate is checked, then loaded without downtime |
| Evaluation self-signed | Test only | Office hosts and `engine-cli` will reject it; never for add-in pilots. **Built**: `DYNODOC_TLS_MODE=evaluation`, labelled in the certificate, a response header and the log, five-minute HSTS |

The bundle has no default mode: `DYNODOC_TLS_MODE` must be set, so no production certificate is ever
chosen or generated silently. Renewal and reload commands are in the README of `infra/selfhosted`.

Settings carried over from the production templates: `ssl_protocols TLSv1.2 TLSv1.3`,
`Strict-Transport-Security: max-age=31536000` (no `includeSubDomains`, no preload
claim), HTTP to HTTPS redirect. A customer using an internal name must understand that
HSTS pins the browser to HTTPS for a year; use a shorter `max-age` during first
installation (configurable in the proxy template).

### 5.3 Proxy requirements (from production templates)

```nginx
# Proposed template parametrised from infra/demo/nginx-app.conf
client_max_body_size 16m;
client_header_timeout 15s;  client_body_timeout 30s;  send_timeout 30s;
proxy_buffer_size 16k;  proxy_buffers 8 16k;  proxy_busy_buffers_size 32k;  # 502 fix
proxy_http_version 1.1;  proxy_set_header Connection "";
proxy_buffering off;  proxy_read_timeout 3600s;                            # SSE
proxy_set_header Host $host;
proxy_set_header X-Forwarded-Proto $scheme;
proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
log_format dynodoc_safe '$remote_addr - [$time_local] "$request_method $dynodoc_safe_uri $server_protocol" $status $body_bytes_sent';
# /view/<token> is logged as /view/[private-link]; query strings are not logged
limit_req_zone $binary_remote_addr zone=dynodoc_auth:10m rate=60r/m;       # /auth/
limit_req_zone $binary_remote_addr zone=dynodoc_metrics:10m rate=120r/m;   # /telemetry (hosted mode only)
```

The production template also maps the `Content-Security-Policy` header so that error
responses and the static `/connectors/commands.html` frame carry a policy, and removes
`X-Frame-Options` only for the three Office pane routes and the commands page. Keep
those maps; they are what [`check-connectors.py`](../infra/demo/check-connectors.py)
tests.

### 5.4 Egress matrix

| Feature | Destination | Profile A | Profile B | Profile C |
|---|---|---|---|---|
| Product operation | none | none | none | none |
| Email sign-in and invitations | SMTP relay | internet relay or none | internal relay | internal relay or disabled |
| OIDC / SAML | IdP endpoints (discovery, token, JWKS, userinfo) | optional | required | internal IdP |
| Google sign-in, Google Forms | `accounts.google.com`, `*.googleapis.com` | optional | optional | not possible |
| Resend mail | `api.resend.com` | optional | not recommended | not possible |
| Office.js (CDN mode) | `officeapis.public.onecdn.static.microsoft`, `appsforoffice.microsoft.com`, `ajax.aspnetcdn.com` from client machines | optional | optional | local copy |
| Research connectors | customer-chosen public hosts | optional | optional | disabled |
| Certificates (ACME) | public CA | optional | optional | internal CA |
| Image pulls | registry | internet or bundle | mirror | internal registry |
| Verne | none ever | none | none | none |

### 5.5 Firewall and WAF notes

- Block direct access to API (8080), PostgreSQL (5432), converter (8090) from user networks.
- Allow `POST` with `application/json` bodies up to 16 MiB to `/api/engine/*`; WAF
  rules that flag JSON patches containing markup-like text will produce false positives.
- Do not let a WAF buffer or compress `text/event-stream` responses.
- Rate-limit `/auth/*` at the proxy as the templates do; the API also limits magic-link
  requests to five per 15 minutes per email hash.

---

## 6. Sizing

Read the [master design, section 9](SELF-HOSTED-EDITION.md) first. **We have no capacity
numbers.** What follows is arithmetic from stated assumptions and from code limits.

### 6.1 Worked example inputs and results

Symbols are defined in the master design (9.1). Coefficients: 1.55 KB per event
including indexes (measured once), 0.45 snapshot-to-state ratio (measured once, synthetic
fixture), 200 bytes per refresh row (assumption), overhead 1.3 to 2 times (assumption).

| | A: lab | B: department | C: enterprise |
|---|---|---|---|
| Registered / weekly active | 30 / 15 | 3,000 / 900 | 20,000 / 6,000 |
| Documents D | 200 | 10,000 | 150,000 |
| State size s | 150 KB | 300 KB | 400 KB |
| Events per document per year | 300 | 400 | 500 |
| Snapshots per document per year | 3 | 3 | 4 |
| Originals (size x uploads) | 1 MB x 2 | 1.5 MB x 1.5 | 2 MB x 1.5 |
| Events per year | 60,000 x 1.55 KB = 0.09 GB | 4 M x 1.55 KB = 6.2 GB | 75 M x 1.55 KB = 116 GB |
| Snapshots per year | 200 x 3 x 68 KB = 0.04 GB | 10,000 x 3 x 135 KB = 4.1 GB | 150,000 x 4 x 180 KB = 108 GB |
| Originals per year | 0.4 GB | 22.5 GB | 450 GB |
| Refresh rows, unpruned (weekly active x 250 days x 8) | 30,000 rows, 6 MB | 1.8 M rows, 0.36 GB | 12 M rows, 2.4 GB |
| Sum before overhead | about 0.55 GB | about 33 GB | about 680 GB |
| With overhead | 0.7 to 1.1 GB | 43 to 66 GB | 0.9 to 1.4 TB |
| Daily dumps kept 30 days (0.3 to 0.5 compression, assumption) | 6 to 17 GB | 390 to 990 GB | 8 to 21 TB |

The last row is why daily full dumps are a Profile A method only. At Profile B use
physical backups with WAL archiving and a weekly base backup (section 9.6).

### 6.2 Provisional starting points

| Profile | Nodes | API | Web | PostgreSQL | Storage |
|---|---|---|---|---|---|
| A | 1 VM, 4 vCPU, 8 GB | 1 x (2 CPU, 2 GiB limit) | 1 x (2 CPU, 1 GiB limit) | on the same VM | 100 GB SSD for OS, images, database; backups on a different disk or share |
| B | 3 to 4 nodes | 2 x (2 vCPU, 2 GiB) | 2 x (1 vCPU, 1 GiB) | primary 4 to 8 vCPU, 16 to 32 GB; standby same | 250 GB to 1 TB NVMe primary; backup repository sized for WAL plus weekly base |
| C | 6 or more nodes across two failure domains | 3+ x (2 to 4 vCPU, 2 to 4 GiB) | 2 to 3 x (1 to 2 vCPU, 1 to 2 GiB) | 3 nodes, 8+ vCPU, 32 to 64 GB | 1 to 4 TB NVMe plus repository; decide object storage before going past 1 TB |

These are chosen conservatively from the production container limits (API 2 CPU/2 GiB,
web 2 CPU/1 GiB, converter 1 CPU/768 MiB) and the code's admission limits. Replace them
with the output of the capacity programme (P2-11) when it exists.

### 6.3 Connection and queue budgets

- `max_connections` of at least `API replicas x 8 + 20`. The listener connection (master
  6.2) is separate from the 8-connection pool. A transaction-mode pooler must not sit
  in front of the listener.
- `idle_in_transaction_session_timeout` is set to 30 s by the API per connection; do
  not override it at the role level with a longer value.
- Watch `pg_notification_queue_usage()` once fan-out is enabled.

### 6.4 Leading indicators

| Signal | Source (proposed metric) | Meaning |
|---|---|---|
| Pool wait above 100 ms p95, or 503 from pool exhaustion | `dynodoc_db_pool_wait_seconds` | Add replicas or connections |
| 429 above 1% of requests outside bursts | `dynodoc_admission_rejections_total` | Limits or abusive client |
| Stream delivery lag above 1 s p95 | `dynodoc_stream_delivery_lag_seconds` | Listener or fan-out |
| Statement timeouts | `dynodoc_db_statement_timeouts_total` | Slow queries on large documents or search |
| WAL archive backlog, replication lag | PostgreSQL | Backup and failover risk |
| Disk growth above plan | host | Retention, bloat |
| Restore drill exceeds the target | drill report | Outgrown the backup method |

---

## 7. Configuration reference

Today every setting is an environment variable. Variables the code reads are listed
with their source; proposed variables are marked **(proposed)** with the task that would
add them. Secrets should come from files or a secret store, never from images or Helm
values. The API implements the `*_FILE` convention (read the value from the named file)
for every secret it reads: `DATABASE_URL`, `PASETO_LOCAL_KEY` and `WEB_SERVICE_KEY`
(P0-2, API half, 20 October 2026; see "Secret files" below). The web process does the same
for its five secrets, with the same rules, and validates its whole configuration when it starts
(P0-1 and P0-2, web half, 22 October 2026; see 7.2). Rows marked **built** below exist in the
code; everything marked **(proposed)** does not.

### 7.1 API process (`engine-api`)

| Variable | Req. | Code default | Compose default | Meaning and notes |
|---|---|---|---|---|
| `DATABASE_URL` or `DATABASE_URL_FILE` | yes | none | `${RUNTIME_DATABASE_URL}` | Runtime login only. Must parse as a PostgreSQL URL. The API refuses owner, superuser, role members, DDL/temporary-object privileges. For TLS use `?sslmode=verify-full&sslrootcert=/path/ca.pem` (sqlx URL parameters). |
| `MIGRATION_DATABASE_URL` / `_FILE` | must be absent | none | not set | Startup aborts if either form is present. Only the operator tool uses it. |
| `PASETO_LOCAL_KEY` or `PASETO_LOCAL_KEY_FILE` | yes | none | required | 32 bytes as 64 hex characters; all zeros rejected. Identical on every replica. Rotation ends all sessions (until the key ring, P2-10). |
| `WEB_SERVICE_KEY` or `WEB_SERVICE_KEY_FILE` | yes | none | required | At least 32 characters; compared with HMAC in constant time. Shared with the web process. |
| `TOKEN_TTL_SECONDS` | no | 86400 | 3600 | Access-token lifetime. Production uses 3600. Recommended default for self-hosted: 3600. |
| `PUBLIC_API_BASE_URL` | no | `http://127.0.0.1:8080` | `${ORIGIN}` | Base for the `verify_url` returned by `/auth/magic-link`. The web process builds its own link; set it to the public origin anyway. |
| `API_HOST` / `API_PORT` | no | `127.0.0.1` / `8080` | `0.0.0.0` / `8080` | Bind address. Never publish this port to user networks. |
| `ADMIN_EMAILS` | no | empty (nobody) | `${ADMIN_EMAILS:-}` | Comma-separated verified emails allowed to use `/admin/*`. Bootstrap only once roles exist (P1-3). |
| `DYNODOC_REF_SIGNING_KEY_FILE` | no | unset | via override file | Path to the receipt-authority key (DER PKCS#8, at most 4,096 bytes). Unset means signed shared-branch writes are refused. |
| `DYNODOC_SHARED_DRAFT_BASES` | no | off | not set | `shared-v2` enables shared draft bases. Keep off until accepted. |
| `DYNODOC_NAMED_SNAPSHOT_STORAGE` | no | `legacy` | not set | `shared-v2` enables shared named snapshots. Keep off. |
| `DYNODOC_PERIODIC_CHECKPOINT_STORAGE` | no | `legacy` | not set | `shared-v1` or `shared-v2`. Keep off. |
| `DYNODOC_ALLOW_OPEN_SECRET_FILES` | no | unset | not set | `true` accepts a `*_FILE` secret that group or others can read (Unix). Needed where the platform fixes the mode, such as Docker and Kubernetes secret mounts. |
| `RETENTION_JOBS` | no | `on` | not set | `on` or `off`. Master switch of the retention loop (below). Anything else stops startup. |
| `RETENTION_INTERVAL_SECONDS` | no | `3600` | not set | Time between passes, 60 to 86,400. The first pass runs after at most 60 s. |
| `RETENTION_BATCH_ROWS` | no | `1000` | not set | Rows removed per DELETE statement, 10 to 10,000. |
| `RETENTION_AUTH_DAYS` | no | `7` | not set | Days an expired or used sign-in link, expired or revoked refresh-token row and expired device-enrollment challenge is kept, 1 to 365. |
| `RETENTION_METRICS_DAYS` | no | `30` | not set | Days a `web_metric` row is kept, 1 to 3,650. The admin overview reads the last 30 days, so a shorter window shortens what it shows. |
| `RUST_LOG` | no | `info,engine_api=debug` | `info` | `tracing` filter. |
| `APP_ENV` | no | not read | `production` | Present in compose; no code reads it. Do not rely on it. |

**Secret files (API).** For each secret above, set `NAME` or `NAME_FILE`, not both: both
non-empty is a startup error, and an empty value of either counts as unset (so a template
line `WEB_SERVICE_KEY=` does not collide with `WEB_SERVICE_KEY_FILE`). A plain `NAME` is
used exactly as it always was. The file must be a regular file of at most 16 KiB holding
UTF-8 text; trailing line breaks are removed, nothing else. On Unix a file readable or
writable by group or others is refused (use mode 0400 or 0600) unless
`DYNODOC_ALLOW_OPEN_SECRET_FILES=true`; Windows has no mode check. The API validates every
secret and retention setting before it opens a database connection and stops with one
error that lists all problems, for example:

```text
invalid configuration (2 problems):
  - WEB_SERVICE_KEY_FILE (/run/secrets/web_service_key): cannot open: No such file or directory (os error 2)
  - RETENTION_AUTH_DAYS must be a whole number from 1 to 365
```

Messages name the variable and the path, never the value or part of it.

**Retention (API).** Every replica runs the loop; a PostgreSQL session advisory lock taken
on a dedicated connection lets one replica prune per pass while the others skip. Rows are
removed in batches of `RETENTION_BATCH_ROWS` with `FOR UPDATE SKIP LOCKED`, and only
rows no code path can use any more:

| Table | Pruned when |
|---|---|
| `magic_link` | expired, or consumed, more than `RETENTION_AUTH_DAYS` ago |
| `refresh_token` | expired, or revoked, more than `RETENTION_AUTH_DAYS` ago (a rotated token that has not expired is kept, because it can still end its login) |
| `auth_request_window` | older than its 15-minute window |
| `device_enrollment_challenge` | expired more than `RETENTION_AUTH_DAYS` ago |
| `web_metric` | recorded more than `RETENTION_METRICS_DAYS` ago |
| `idempotency_key` | older than the 24-hour replay window |

No history, event, snapshot, audit or erasure table is touched, and the runtime login
needs no new privilege. Counters of rows pruned per table, passes, skipped passes and
failures are in the `retention` object of `GET /admin/overview` (per process, since
start). An info log line with counts per table is written after each pass that removed
rows. A failed pass is logged and retried within five minutes.

Fixed in code today (candidates to expose): pool of 8 connections, 5 s acquire timeout,
15 s statement timeout, 5 s lock timeout, 30 s idle-in-transaction timeout
(`main.rs`); 64/256/8 admission slots and the per-identity and per-project windows
(`workloads.rs`); 100 subscribers per document and 30 s keepalive (`stream.rs`); 16 MiB
request body (`lib.rs`); refresh lifetime of 30 days and link lifetime of 15 minutes
(`auth/mod.rs`).

### 7.2 Web process (SvelteKit, adapter-node)

The web process reads its whole configuration once, when it starts
(`frontend/researcher/src/lib/server/config/`), and stops before it listens if anything is
wrong, with one message that lists every problem and never a secret value (the API's format
in 7.1). A development server (`vite dev`) reports the same problems as warnings so the public
pages can be worked on without secrets; `vite preview` and `node build` stop. Unknown
`DYNODOC_*` names that resemble a known one (`DYNODOC_TELEMTRY`) are reported as warnings.

| Variable | Req. | Default | Meaning and notes |
|---|---|---|---|
| `DYNODOC_MODE` | no | `hosted` | **Built.** `hosted` is the hosted service exactly as it runs today. `selfhosted` removes the hosted-only parts, as listed under "Self-hosted mode" below. Any other value stops startup (`self-hosted` with a hyphen is not accepted). |
| `ORIGIN` | yes (selfhosted) | none | Exact public origin: scheme, host, optional port, no path or trailing slash. Mismatch makes every POST fail with `Invalid origin`. In `selfhosted` mode a missing or malformed value stops startup and a plain `http://` origin other than loopback is warned about. In `hosted` mode a malformed value is a warning. |
| `DYNODOC_ALT_ORIGINS` | no | empty | **Built**, `selfhosted` only. Further origins the same installation answers to, comma-separated, at most 8, each checked like `ORIGIN`. They join `ORIGIN` in the CSP `connect-src` and in the origins allowed to send page metrics. |
| `SESSION_SECRET` or `SESSION_SECRET_FILE` | yes (selfhosted) | none | At least 32 characters and not trivial: a value that is short, blank, made of fewer than 8 distinct characters, repeats a pattern of up to 8 characters, or contains a documented placeholder (`change-me`, `replace-with`, `placeholder`, ...) stops startup in both modes. This is a sanity check, not a proof of entropy: generate it with `openssl rand -hex 32`. Hashed (SHA-256) into the AES-256-GCM key for the session cookie and provider-state cookies. **Checked when the server starts** (it was checked on first use before 22 October 2026). Absent: stops a `selfhosted` server; in `hosted` mode a warning, because the public homepage host on Vercel runs with no secret at all ([VERCEL-DEPLOYMENT.md](VERCEL-DEPLOYMENT.md)) and must keep starting. Same on all replicas. |
| `WEB_SERVICE_KEY` or `WEB_SERVICE_KEY_FILE` | yes (selfhosted) | none | Service credential for `/auth/magic-link`, `/auth/exchange`, `/telemetry`; the API must hold the same value. Checked like `SESSION_SECRET`. Missing in `hosted` mode is a warning (as before, the pages start and sign-in fails at the API); present but weak stops startup in both modes. Equal to `SESSION_SECRET`: an error in `selfhosted` mode, a warning in `hosted` mode. |
| `ENGINE_API_URL` | no | `http://127.0.0.1:8080` | Internal API address (the web process's API base URL). Must be an `http://` or `https://` URL without user name, password, query or fragment; a trailing slash stops startup in `selfhosted` mode and is a warning in `hosted` mode. |
| `CONVERTER_URL`, `CONVERTER_KEY` or `CONVERTER_KEY_FILE` | pair | unset | Legacy `.doc` import. Missing: the route answers 503 with an instruction to save as DOCX. One of the two set: a warning. |
| `DYNODOC_MAIL_TRANSPORT` | no | `resend` when `RESEND_API_KEY` is set, else `none` | **Built.** `resend`, `smtp` or `none` (any other value stops startup). `none` turns mail off; the interface says so (see "Mail" below). Explicit `resend` without its key and sender stops startup. |
| `RESEND_API_KEY` or `RESEND_API_KEY_FILE`, `EMAIL_FROM` | pair | unset | The `resend` transport, as before. Both set means `email: true` in `/auth/options` (unless `AUTH_EMAIL_LINK=off`). Set but the transport is `smtp` or `none`: a warning, they are not used. |
| `SMTP_HOST` | yes (smtp) | none | **Built.** Mail server: a host name or an IP address, with no scheme (`smtp://`), port, path or spaces. IPv6 without brackets. |
| `SMTP_PORT` | no | `587` (`starttls`), `465` (`tls`), `25` (`none-local`) | **Built.** 1 to 65535. |
| `SMTP_SECURITY` | no | `starttls` | **Built.** `tls` (TLS from the first byte), `starttls` (upgrade; refuses to go on without it) or `none-local` (plain text, only to a loopback or private address; the loudly named opt-out). Certificate and host name are always verified, TLS 1.2 or newer. |
| `SMTP_USER`, `SMTP_PASSWORD` or `SMTP_PASSWORD_FILE` | pair | unset | **Built.** Both or neither (one without the other stops startup). The password takes `NAME` / `NAME_FILE` like every secret (7.2 secret files), may not contain a NUL, and is never logged or printed. AUTH PLAIN or LOGIN. |
| `SMTP_FROM` | yes (smtp) | none | **Built.** The sender address and envelope sender, such as `docs@example.org`; the relay must accept it. The Message-ID is under its domain. |
| `SMTP_FROM_NAME` | no | the product name (`DYNODOC_BRAND_NAME`, else `Dynodoc`) | **Built.** Display name of the sender, one line, up to 120 characters. |
| `SMTP_REPLY_TO` | no | unset | **Built.** Reply-To for messages that have none of their own (an invitation answers to the person who invited). |
| `SMTP_CA_FILE` | no | unset | **Built.** PEM file with the certificate authorities to trust **instead of** the built-in list, for a mail server behind a private authority. Up to 256 KiB and 100 certificates; read at start; an expired one is a warning. Ignored with `none-local`. |
| `SMTP_TIMEOUT_SECONDS` | no | `30` | **Built.** 5 to 120. The time one send may take in all, retries included. |
| `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` or `GOOGLE_CLIENT_SECRET_FILE` | pair | unset | Both set means Google sign-in and Forms import are offered. |
| `AUTH_EMAIL_LINK` | no | `on` | **Built.** `off` hides email sign-in on the sign-in page, answers `email: false` and `development: false` in `/auth/options`, and makes `POST /auth/email` answer 403 before any mail or API call. A link that was already issued can still be confirmed. On with no mail transport that can send: a `selfhosted` server stops at start (nobody could sign in) unless `DEV_AUTH=true`; a `hosted` one warns. |
| `DYNODOC_TELEMETRY` | no | `on` in `hosted`, `off` in `selfhosted` | **Built.** `on` or `off`. Off: the browser code makes no request, loads nothing, and `/telemetry` answers 404 to every method. On in `selfhosted` mode: pages report to `/telemetry` of the same installation, accepted only from `ORIGIN` and `DYNODOC_ALT_ORIGINS`, stored in the installation's own `web_metric` table. |
| `DYNODOC_BRAND_NAME` | no | `Dynodoc` | **Built**, `selfhosted` only. Product name in the sign-in page, workspace header and footer, shared-link header and administration page (up to 60 characters, one line). The logo images and the installable-app name are not configurable yet. |
| `DYNODOC_ORG_NAME` | no | empty | **Built**, `selfhosted` only. Organisation name for the notice (up to 120 characters, one line). |
| `DYNODOC_ORG_NOTICE` or `DYNODOC_NOTICE_FILE` | no | empty | **Built**, `selfhosted` only. Plain text of up to 2,000 characters and 40 lines, shown on the sign-in page and in the workspace footer where the hosted service shows its privacy and terms links. Line breaks are kept; markup is shown as typed. Control and direction-override characters are refused. The file form reads at most 16 KiB of UTF-8 with no permission check; both set is an error. It stands in for the privacy policy, terms and contact pages, which a self-hosted installation does not serve. |
| `DYNODOC_SUPPORT_URL` | no | empty | **Built**, `selfhosted` only. An `https:`, `http:` or `mailto:` link of up to 300 characters, without credentials, shown as "Contact or support" in the notice. |
| `DYNODOC_ALLOW_OPEN_SECRET_FILES` | no | unset | `true` accepts a secret file that group or others can read (Unix). Same meaning as for the API. |
| `CONNECTOR_ORIGINS` | no | empty | Comma-separated extra HTTPS origins appended to pane `connect-src`. Malformed entries discarded. |
| `DEV_AUTH` | no | `false` | Never `true` outside a test stack: with no mail key it returns sign-in tokens in the HTTP response. Only the exact text `true` turns it on; `selfhosted` mode warns when it is. |
| `ADMIN_SITE` | no | `false` | `true` turns this process into the admin site (root redirects to `/admin`, sign-in lands there). Only the exact text `true` turns it on. |
| `PUBLIC_WORKSPACE_URL` | no | unset | Redirects workspace paths from the public marketing host to another host. Off unless set, in both modes; not for a single-host installation (`selfhosted` mode warns when it is set). |
| `OPS_STATUS_PATH` | no | unset | File with the host status JSON for `/admin`. |
| `BODY_SIZE_LIMIT` | image | `16M` | adapter-node request body cap. |
| `HOST` / `PORT` / `NODE_ENV` | image | `0.0.0.0` / `3000` / `production` | |
| `NODE_EXTRA_CA_CERTS` | proposed use | unset | PEM bundle for outbound TLS to an IdP behind a private CA. The mail server uses `SMTP_CA_FILE` instead. |

**Secret files (web).** `SESSION_SECRET`, `WEB_SERVICE_KEY`, `RESEND_API_KEY`,
`GOOGLE_CLIENT_SECRET` and `CONVERTER_KEY` each take `NAME` or `NAME_FILE`, with the API's
rules (7.1): both non-empty is an error; an empty value of either counts as unset; the file
is a regular file of at most 16 KiB, UTF-8, with trailing line breaks removed; on Unix a file
group or others can access is refused unless `DYNODOC_ALLOW_OPEN_SECRET_FILES=true`; Windows
has no mode check. A plain `NAME` is used exactly as before. All problems are reported
together, for example:

```text
invalid configuration (3 problems):
  - SESSION_SECRET looks like a placeholder; generate a random value (openssl rand -hex 32)
  - WEB_SERVICE_KEY is not set (at least 32 characters; or set WEB_SERVICE_KEY_FILE)
  - ORIGIN is not set (the public origin people use, such as https://dynodoc.example.org)
```

**Self-hosted mode (`DYNODOC_MODE=selfhosted`).** What changes, all of it covered by tests:

- The marketing, legal and help pages of the public site (`/privacy`, `/terms`, `/features`,
  `/support`, ... every page of the `(site)` group), `/sitemap.xml`, `/llms.txt`,
  `/llms-full.txt` and `/source` answer 404, with no redirect. `/` sends a visitor to
  `/login`, or to `/projects` when signed in.
- Nothing is offered to search engines: every response carries
  `x-robots-tag: noindex, nofollow`, every page carries `<meta name="robots" content="noindex,nofollow">`
  and no canonical or social tags, `/robots.txt` is `User-agent: *` / `Disallow: /`, there is
  no sitemap.
- The CSP `connect-src` is `'self'` plus `ORIGIN` and `DYNODOC_ALT_ORIGINS` (hosted:
  `'self' https://app.dynodoc.online`). The Office panes keep the Microsoft sources they need
  (the Office.js CDN is a known exception until P0-7).
- No page metrics (see `DYNODOC_TELEMETRY`), no `app.dynodoc.online` root redirect, no
  shared theme-cookie domain, no workspace-host redirect unless `PUBLIC_WORKSPACE_URL` is set.
- The sign-in page shows no link to the hosted site, no privacy or terms links, and no Google
  button unless Google is configured; the organisation notice stands in their place.
- The browser is told only what it may know (`site` in the root layout data): mode, product
  name, notice, whether metrics are on and where, and whether email sign-in is on.

**What still names the hosted service in the build.** The marketing pages are compiled into
the same build and are only unreachable in `selfhosted` mode, and a few static files carry
hosted names (the add-in packages built for `app.dynodoc.online`, `security.txt`, the social
image). `node frontend/researcher/scripts/audit-hosted-strings.mjs` lists every occurrence in
a build and says whether a self-hosted installation can reach it; `--check` fails on anything
not in `scripts/hosted-strings-known.json`, `--strict` applies the acceptance criterion as
written ("no file in the shipped images contains `dynodoc.online` outside third-party
notices") and fails until that list is empty, which needs an image build that leaves the
`(site)` routes out (P0-6) and per-installation add-in packages (P0-7).

**Mail (sign-in links and invitations).** The web server sends exactly two kinds of
message: the emailed sign-in link and the invitation to a document or workspace. Nothing else
is emailed. `DYNODOC_MAIL_TRANSPORT` chooses how they are sent: `resend` (the hosted
service's HTTPS provider, unchanged), `smtp` (a mail server of your choice) or `none`.
Unset, it is `resend` when `RESEND_API_KEY` is set and `none` otherwise, so the hosted service
behaves as it always did.

With `none`, mail is off and the product says so rather than dropping anything: the sign-in
page shows "Email sign-in is awaiting administrator setup." and the button is disabled, and an
invitation grants access and shows the person who invited the link to send themselves ("Email
is not set up on this server, so no invitation was sent. Send <address> this link yourself:
<link>"). A self-hosted server with email sign-in on and no way to send the link stops at
start (nobody could sign in); set a transport or `AUTH_EMAIL_LINK=off`. Hosted only warns.
The test stack switch `DEV_AUTH=true` is the one exception (it warns).

*Library decision.* SMTP is nodemailer **10.1.0**, pinned exactly: licence MIT-0 (the
package's own LICENSE text is the MIT permission without the attribution condition), **no
runtime dependencies** (its `package.json` has no `dependencies`, so the lockfile gained one
package and nothing else), published from GitHub Actions with npm provenance, integrity
`sha512-fXW8AulB25aOTuwfRYV6lqHyfz2VM4rr4SGXH03mE7x5znEUpzL5mu6anARUpZN7744Ziwkmr4AxSLPJsBQspg==`
(identical in the registry metadata and the lockfile), `pnpm audit --prod` lists no advisory for
it. It is loaded only when the transport is `smtp`. An own client over `node:net` and
`node:tls` was rejected: parsing replies, STARTTLS state, AUTH and MIME is security-critical code
that a maintained library already gets reviewed. The library is released very often (10.0.0 on 3
September, 10.2.0 on 21 October 2026, many of the fixes in the address parser): Dynodoc therefore
gives it only addresses it has validated itself, builds the envelope itself, and checks the
finished message again (below), so a parser fix in the library is not a precondition for
safety. 10.1.0 was chosen because it was twelve days old at the time and carries the connection
hardening of 10.1.0; later 10.1.x releases (address-parser fixes, limits on work per message,
credential stripping in an HTTP fetch Dynodoc never uses) are not needed for how it is used
here. Re-run `pnpm audit` and the mail tests when you update it.

*Connection security.* `SMTP_SECURITY=tls` is TLS from the first byte (port 465).
`starttls` (the default; port 587) upgrades the connection and **refuses to continue if the
server does not offer or does not complete STARTTLS**: a server that leaves STARTTLS out cannot
make Dynodoc send a password in the clear, and a login offered only before TLS is never used.
Certificates and the host name are always verified, TLS 1.2 is the minimum, and the check holds
even if `NODE_TLS_REJECT_UNAUTHORIZED=0` is set for the process (a warning says so).
`SMTP_CA_FILE` (a PEM file of one or more certificates) **replaces** the built-in list of
authorities, for a mail server behind a private authority. `none-local` is plain text and is
**only** for a relay on this machine or a private network: an IP address in the configuration
must be loopback (127.0.0.0/8, ::1), private (10/8, 172.16/12, 192.168/16, fc00::/7),
link-local or shared (100.64/10), and a host name is resolved when mail is sent and refused
unless every address is one of those (the connection goes to the address that was checked).
With a login, `none-local` sends the user name and password unencrypted to that host; the
server warns at start. Authentication is AUTH PLAIN or LOGIN, whichever the server offers first
(CRAM-MD5 only if it offers nothing else).

*Sending.* One time budget (`SMTP_TIMEOUT_SECONDS`, 30 by default) covers a whole send; inside
it, connect and greeting are given up on after 10 s and a silent connection after 15 s. A 4xx
reply, a refused or lost connection or a timeout is tried again, at most three attempts in all,
after 1 s then 2 s (each randomly shortened by up to half). A 5xx reply, a refused certificate,
a wrong password or a refused message is never tried again. A message has at most 10
recipients (the product sends one) and at most 256 KiB. Nothing is queued: a send that fails
returns an error to the person (503 for a sign-in link, "the invitation email could not be sent"
with the link for an invitation). Message-ID is under the domain of `SMTP_FROM`; the EHLO name is
the host name in `ORIGIN` when it is a real DNS name, otherwise that domain. Each message is
multipart/alternative (text and simple HTML built from the same template, every value escaped,
no images, no tracking, one link) in UTF-8 with RFC 2047 headers, and carries `Auto-Submitted:
auto-generated`. Links are always built on `ORIGIN` in self-hosted mode, never on what a request
says.

*Addresses and headers.* A recipient or sender must be `local@domain` with an ASCII local part of
letters, digits and `. _ + - ' = &` and a plain DNS domain with a dot. Quoted, bracketed,
display-name, group, comment, IP-literal, `%`, `!` and non-ASCII forms are refused, not
rewritten. A line break or any control character in a subject, a name, a user name or an address
is refused before anything is sent, the finished message is parsed again and refused unless its
headers are exactly the expected ones, and a reply-to is only taken from the request that
asked for it (the inviter) or `SMTP_REPLY_TO`.

*Log.* One line per outcome, `[dynodoc] mail provider=smtp outcome=sent kind=signin attempts=1
id=<...@example.org>` or `outcome=failed class=auth-failed`: the provider, the outcome, a short
class, the attempt count and the Message-ID. Never a password, a recipient, a subject or the
text of the server's reply.

*Check the settings against the real server:* `node scripts/check-mail.mjs --dir <dir> --to
you@example.org` reads the `.env` in `<dir>` with the validator the server runs, connects, secures
the connection, says EHLO, logs in, gives MAIL FROM and RCPT TO, and drops the connection when
the server asks for the message text, so nothing is delivered. `--send` then sends one short test
message to that address. `--json` prints one object for `dynodoctl`. Exit code 0 passed, 1
failed, 2 could not run, 3 the transport is not SMTP. It never prints a password; for a refusal it
shows the server's own words, which may quote the `--to` address.

*What an operator sees, and does.* The message is what the check prints and what a failed send
carries in the error (the log carries only the class):

| Message | What to do |
|---|---|
| TLS certificate not trusted: set SMTP_CA_FILE to the certificate authority that signed the mail server certificate (or to the certificate itself) | Put the certificate of the authority that signed the mail server certificate (or the certificate itself) in a PEM file and name it in `SMTP_CA_FILE`. It replaces the built-in list, so include every authority the server chain needs. Do not switch certificate checks off: there is no setting for that. |
| TLS certificate does not match the server name: SMTP_HOST must be a name that the certificate lists | Set `SMTP_HOST` to a name the certificate lists (the name on the certificate, not an IP address, unless the certificate lists the address). |
| TLS certificate of the mail server has expired or is not valid yet: ask the mail administrator to renew it, and check this machine's clock | The mail administrator renews the certificate. If it is current, check this machine clock. |
| TLS handshake failed: check SMTP_PORT and SMTP_SECURITY (port 465 usually needs tls, port 587 usually needs starttls; TLS 1.2 or newer is required) | The port and `SMTP_SECURITY` do not match: 465 is usually `tls`, 587 usually `starttls`. The server must also speak TLS 1.2 or newer. |
| the server does not offer STARTTLS, so the connection would not be encrypted and nothing was sent: use SMTP_SECURITY=tls on port 465, or ask the mail administrator to enable STARTTLS (a password is never sent without encryption) | Use `SMTP_SECURITY=tls` (port 465) if the server has that, or ask for STARTTLS to be enabled. For a relay on this machine or a private network only, `none-local` is the explicit alternative. |
| the mail server did not answer in time: check SMTP_HOST, SMTP_PORT and any firewall, and SMTP_SECURITY (a server that speaks TLS from the first byte, usually on port 465, needs SMTP_SECURITY=tls) | Check the host, the port and any firewall between the two machines. A server that speaks TLS from its first byte (usually port 465) needs `SMTP_SECURITY=tls`. |
| the mail server refused the connection: check SMTP_HOST and SMTP_PORT, and that the relay is running | Check `SMTP_HOST` and `SMTP_PORT`, and that the relay is running. |
| SMTP_HOST could not be found in DNS | Check the spelling of `SMTP_HOST` and the DNS settings of this machine. |
| authentication failed: check SMTP_USER and SMTP_PASSWORD | Check `SMTP_USER` and the password (`SMTP_PASSWORD` or the file named by `SMTP_PASSWORD_FILE`). |
| the server requires authentication before it accepts mail: set SMTP_USER and SMTP_PASSWORD | Set `SMTP_USER` and `SMTP_PASSWORD`, or ask for this machine to be allowed to relay without a login. |
| relay refused: the server will not take mail from this installation for that recipient; ask the mail administrator to allow it (check SMTP_FROM and which machines the relay trusts) | The relay will not take mail from this installation for that recipient. The mail administrator allows the machine or the login, and confirms that `SMTP_FROM` is an address the relay accepts. |
| the server refused the sender address: SMTP_FROM must be an address the relay accepts | Set `SMTP_FROM` to an address the relay allows this login to send from. |
| SMTP_SECURITY=none-local is only allowed for a loopback or private network address, and the host resolves elsewhere | The name in `SMTP_HOST` resolves to an address that is not loopback or private. Use `tls` or `starttls`, or a name that resolves only to a private address. |

*Settings for common mail servers* (all other settings keep their defaults):

```text
# A corporate relay on port 587 with STARTTLS and a login. Its certificate is from a private
# authority, whose certificate is in the file named by SMTP_CA_FILE.
DYNODOC_MAIL_TRANSPORT=smtp
SMTP_HOST=smtp.corp.example
SMTP_SECURITY=starttls
SMTP_USER=dynodoc
SMTP_PASSWORD_FILE=/run/secrets/smtp_password
SMTP_FROM=docs@corp.example
SMTP_CA_FILE=/etc/dynodoc/corp-ca.pem

# A provider on port 465 (TLS from the first byte), public certificate, login.
DYNODOC_MAIL_TRANSPORT=smtp
SMTP_HOST=smtp.provider.example
SMTP_PORT=465
SMTP_SECURITY=tls
SMTP_USER=apikey-or-user
SMTP_PASSWORD_FILE=/run/secrets/smtp_password
SMTP_FROM=docs@example.org

# Postfix on the same machine, accepting mail from 127.0.0.1 without a login. In a container,
# 127.0.0.1 is the container itself: give the private address of the host or of the mail
# container instead (for example 172.17.0.1, or the service name when it resolves privately).
DYNODOC_MAIL_TRANSPORT=smtp
SMTP_HOST=127.0.0.1
SMTP_SECURITY=none-local
SMTP_FROM=docs@example.org
```

*Not built:* a database outbox with retry and administrator visibility (a failed send is
reported to the person, not queued), operator-issued sign-in links, XOAUTH2, DKIM signing
(Dynodoc does not sign: SPF, DKIM and DMARC belong to the relay. Publish SPF for the machines the
relay sends from, have the relay sign for the domain of `SMTP_FROM`, and keep `SMTP_FROM` in a
domain you control so that DMARC aligns), bounce handling, and per-language templates.
A From address does not create an inbound mailbox: replies go where `SMTP_REPLY_TO` or the
inviter's address says.

**Environment-file generator and check.** `node frontend/researcher/scripts/dynodoc-env.mjs
generate-env --dir <new directory> --origin https://dynodoc.example.org` writes a commented `.env`
that names secret files, and a `secrets/` directory with one random 32-byte value per secret
(`session_secret`, `web_service_key`, `paseto_local_key`, `postgres_password`, `database_url`
with a random runtime password, and `converter_key` with `--with-converter`), mode 0600 on
Unix. Without a mail server the `.env` has `DYNODOC_MAIL_TRANSPORT=none` and
`AUTH_EMAIL_LINK=off` (the server refuses email sign-in with no way to send the link) and a
commented SMTP block; `--smtp-host H --smtp-from A [--smtp-port N] [--smtp-security
tls|starttls|none-local] [--smtp-user U] [--smtp-ca-file P]` writes a checked SMTP block instead
(with `--smtp-user`, an empty `secrets/smtp_password` for you to fill; `check-config` says it is
empty until you do). It refuses to overwrite anything, writes nothing if any target exists,
prints no secret, and lists what to back up. `check-config --dir <directory>` validates that directory with the
same validator the server runs (plus the shape of `PASETO_LOCAL_KEY` and `DATABASE_URL`, which
only the API reads) without starting anything; exit code 0 (valid), 1 (problems) or 2 (cannot
run), `--json` for tools. `dynodoctl` is meant to call both.
### 7.3 Other components

| Component | Variables | Notes |
|---|---|---|
| Converter | `CONVERTER_KEY` | Constants in `server.py`: 3,000,000-byte cap, two conversion slots, 45 s timeout, macros disabled by profile registry. Listens on 8090. |
| PostgreSQL image | `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB` | Compose uses `dynodoc`/`dynodoc`. The password is the owner credential, not the runtime one. |
| Operator tool | `MIGRATION_DATABASE_URL`, `DATABASE_URL` | `engine-cli migrate` and `grant-runtime` use the first; `check-runtime`, `verify` use the second. CLI binaries read the environment, not dotenv files. |
| Compose helper | `REF_AUTHORITY_KEY_FILE` | Only with `compose.schema38.override.yml` (hosted). Not wired into `infra/selfhosted` yet. |
| Bundle (`infra/selfhosted`, **built**) | `DYNODOC_TLS_MODE` (required: `provided`, `evaluation`, `acme`), `DYNODOC_BIND` (default `127.0.0.1`), `DYNODOC_HTTP_PORT`, `DYNODOC_HTTPS_PORT`, `DYNODOC_HSTS_MAX_AGE`, `DYNODOC_TLS_RELOAD_SECONDS`, `DYNODOC_ACME_EMAIL`, `DYNODOC_ACME_ACCEPT_TOS`, `DYNODOC_ACME_CA`, `DYNODOC_IMAGE_REGISTRY`, `DYNODOC_VERSION`, `DYNODOC_POSTGRES_IMAGE`, `COMPOSE_PROFILES` | Set in `.env` beside the web settings of 7.2; documented in `infra/selfhosted/bundle.env.example` and its README. The bundle fixes the database names (login `dynodoc_runtime`, database `dynodoc`, host `postgres`). |

### 7.4 Proposed variables

| Variable | Task | Purpose |
|---|---|---|
| `DYNODOC_AIRGAP=true` | P0-1, P0-9 | Refuses to start features that need outbound access; tightens CSP (`img-src`, Office hosts) |
| Support link in add-in manifests | P0-7 | `DYNODOC_SUPPORT_URL` exists (7.2); the manifest generator does not read it yet |
| Mail delivery | P0-3 | **Built** as `DYNODOC_MAIL_TRANSPORT` and `SMTP_*` (7.2), not under the names proposed here (`MAIL_TRANSPORT`, `SMTP_URL`, `SMTP_REQUIRE_TLS`): TLS is required by default and `SMTP_SECURITY=none-local` is the only way out |
| Operator-issued sign-in links | P0-3 | `AUTH_EMAIL_LINK` and SMTP exist (7.2); the operator-issued link does not |
| `OIDC_PROVIDERS_FILE`, `SAML_PROVIDERS_FILE` | P1-1, P1-8 | Provider definitions ([identity document](SELF-HOSTED-IDENTITY-AND-ACCESS.md)) |
| `SCIM_ENABLED` | P1-5 | Mount `/scim/v2` |
| `SESSION_IDLE_TIMEOUT_SECONDS`, `SESSION_ABSOLUTE_TIMEOUT_SECONDS`, `REFRESH_TTL_SECONDS`, `REFRESH_REUSE_INTERVAL_SECONDS` | P1-6, P2-3 | Session policy |
| `DYNODOC_EVENT_FANOUT=local\|postgres\|poll` | P2-1 | Live-stream fan-out |
| `DYNODOC_RATE_LIMIT_BACKEND=local\|share\|postgres`, `DYNODOC_REPLICA_COUNT` | P2-2 | Shared limits |
| `DYNODOC_JOBS=on\|off`, `DYNODOC_COPY_DETECTION=on\|off` | P2-4 | Which replica runs jobs; privacy switch |
| `DB_POOL_MAX`, `DB_ACQUIRE_TIMEOUT_MS`, `DB_STATEMENT_TIMEOUT_MS` | P2-14 | Pool and deadlines |
| `API_CORS_ORIGINS` | P2-14 | Allowed browser origins for the API port (default none) |
| `METRICS_ADDR`, `LOG_FORMAT=json\|text`, `OTEL_EXPORTER_OTLP_ENDPOINT` | P2-6 | Observability |
| `OFFICEJS_LOCAL_PATH` | P0-7 | Directory with a local Office.js copy |
| `RESEARCH_ALLOWED_HOSTS` | Q14 | Private research servers |
| `DYNODOC_LICENSE_FILE` | P3-3 | Licence file path |

### 7.5 Secret generation and shape

| Secret | Generate | Shape |
|---|---|---|
| `POSTGRES_PASSWORD`, runtime DB password | `openssl rand -hex 32` | URL-encode if placed in a URL |
| `PASETO_LOCAL_KEY` | `openssl rand -hex 32` | 64 hex |
| `WEB_SERVICE_KEY`, `SESSION_SECRET`, `CONVERTER_KEY` | `openssl rand -hex 32` each, independently | At least 32 characters |
| Receipt authority key | `openssl genpkey -algorithm ED25519 -outform DER -out ref-authority.der` then `chmod 0400` | DER PKCS#8, readable by uid 10001 |

Never reuse one secret for two purposes; the production template says to generate each
independently ([`.env.example`](../infra/demo/.env.example)).

---

## 8. Database

### 8.1 Roles and bootstrap

Three credentials with different custody:

| Credential | Used by | Holder |
|---|---|---|
| Database owner (also used as migration login unless split) | `engine-cli migrate`, `grant-runtime`, `install-ref-authority`, backups and drills | Database administrators; never in API or web environments |
| Runtime login | `engine-api` only (`DATABASE_URL`) | Deployment secret |
| Monitoring and replication logins | Prometheus exporter, standby | Platform team; read-only |

Bootstrap, taken from [DATABASE-PRIVILEGES.md](DATABASE-PRIVILEGES.md) and the schema 38
release record:

```sql
-- as the database owner, interactively (password not on a command line)
CREATE ROLE dynodoc_runtime LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
\password dynodoc_runtime
REVOKE ALL ON DATABASE dynodoc FROM PUBLIC;
GRANT CONNECT ON DATABASE dynodoc TO dynodoc_runtime;
```

```sh
# operator host, owner URL in MIGRATION_DATABASE_URL
engine-cli migrate
engine-cli grant-runtime dynodoc_runtime
# runtime URL in DATABASE_URL
engine-cli check-runtime
```

`grant-runtime` grants an explicit allowlist and verifies it in the same transaction;
it creates no role and handles no password. Re-run it, then `check-runtime`, after every
migration (every schema release record says so).

### 8.2 Recommended parameters (starting points, not measured)

| Parameter | Suggestion | Why |
|---|---|---|
| `timezone` | `UTC` | Schema uses `timestamptz`; deterministic dumps |
| `max_connections` | at least replicas x 8 + 20 | 8 per API process plus operator and monitoring |
| `shared_buffers` | about 25% of RAM | Common starting point |
| `wal_level` | `replica` (or `logical` only if the customer needs it) | Standby and PITR |
| `archive_mode` / `archive_command` | on, to the backup repository | PITR (P2-9) |
| `archive_timeout` | 60 s | Bounds archive lag on a quiet system |
| `max_wal_size`, `checkpoint_timeout` | raise for write bursts such as bulk imports | Fewer checkpoints |
| `log_min_duration_statement` | 500 ms | Find slow search and verification queries |
| `track_io_timing` | on | Diagnose I/O |
| `idle_in_transaction_session_timeout` | leave unset at role level | API sets 30 s per connection |
| autovacuum | defaults; watch `event`, `refresh_token`, `document_fingerprint*` | High insert or churn tables |

### 8.3 High availability options

| Option | Notes |
|---|---|
| Managed PostgreSQL service | Offers failover and PITR; check that `pgcrypto`, `pg_trgm` and a non-owner login are allowed; the `grant-runtime` allowlist expects a plain role |
| Patroni with etcd/Consul | Common for VMs; synchronous replication gives RPO 0 for acknowledged commits at a latency cost; test fencing |
| Kubernetes operator (CloudNativePG, Zalando, others) | Backup and failover handled by the operator; verify that the owner credential stays out of API pods |
| Streaming standby without automation | Profile B minimum: manual promotion runbook |

The API reconnects pooled connections on failover; open SSE streams reconnect through the
browser. In-flight writes may be cut after commit; clients retry with idempotency keys
(`Idempotency-Key`, 24 h window).

### 8.4 TLS to PostgreSQL

Require TLS on the database listener for B and C. The API's driver accepts
`sslmode=verify-full` and `sslrootcert`; the operator tool uses the same URL syntax. A
private CA must be present in the API image or mounted. This is not exercised by the
existing compose (plain TCP on an internal network).

### 8.5 Trust boundary reminder

The restricted runtime login cannot rewrite history, disable triggers or create objects.
It does **not** make the database owner untrusted: an owner can disable the append-only
guards. Separation of duties (owner credential held by a different team than the
application operators), independent anchors for chain heads (open work) and
`verify-snapshots --all` after any privileged maintenance are the controls.

### 8.6 Operational rules

- Never edit `_sqlx_migrations`; never use `TRUNCATE`, manual `DELETE` or `UPDATE` on
  history tables; use the admin erasure workflow, which writes receipts.
- Do not drop and recreate the database to "reset" a deployment that has a receipt
  authority installed: the installation UUID and public key live in the database.
- A restored copy has the same installation identity (master 8.2).

---

## 9. Operator runbooks

Conventions: commands with `dynodoctl` are proposed; each has an equivalent using
existing tools, shown or referenced. Paths follow the production layout
(`/opt/dynodoc-demo`) only where the existing scripts require it. Every runbook ends
with a record: who, when, version, checks run, results.

*Status, 22 October 2026 (task P0-5).* `dynodoctl` v1 exists for Profile A and is documented in
[SELF-HOSTED-OPERATIONS.md](SELF-HOSTED-OPERATIONS.md). Where this chapter proposes a command, the built
equivalent is: `install` (9.1), `backup` and `backup --verify` (9.6), `restore-verify` and
`restore --into-live` (9.7), `rehearse-lite` and `upgrade` (9.8; the plan is `upgrade --plan`, and
`rehearse` is `rehearse-lite`), `rollback` (9.10, level 3 only), `status` and `verify-delivery` (the proposed `check`
and `verify-delivery`), `configure` (9.9 partly: the origin is changed, the add-in manifests are not regenerated),
`support-bundle`. Not built: `bundle verify` and `bundle import` (signed bundles, P3-1), `admin sign-in-link` (P0-3),
`addin build` (P0-7), encryption and remote targets for backups (9.6), binary rollback without a restore (9.10 level 2),
rotation helpers (9.12), the Helm and Profile B/C paths. The backup is `pg_dump -Fc`; its copy of the secrets is not
encrypted (decision recorded in the operations guide). DQ1 is settled as a Rust binary in the application repository.

### 9.1 Install, Profile A

*Today (P0-4, no `dynodoctl` yet):* follow the README of `infra/selfhosted`: build or pull the
images, `generate-env`, choose `DYNODOC_TLS_MODE`, `docker compose run --rm mailcheck --to ...`,
`docker compose up -d --wait`. The four jobs of the start-up do steps 1 to 4 below on every `up`
(secrets and settings checked, runtime login created, migrate, grant, check). Step 2 of 9.2 needs
a working mail transport: operator-issued sign-in links (P0-3) are not built, so the first
administrator signs in by an emailed link, and the first request for a link creates an account.

Proposed (`dynodoctl`):

```sh
sha256sum -c SHA256SUMS
dynodoctl bundle verify dynodoc-1.0.0-linux-amd64.bundle --public-key cosign.pub
dynodoctl install --origin https://docs.example.org \
    --tls provided --cert fullchain.pem --key privkey.pem \
    --data /srv/dynodoc --backups /mnt/backups --mail smtp://relay.example.org:587
dynodoctl admin sign-in-link admin@example.org     # one-time link, 15 minutes
dynodoctl check                                     # health, auth options, delivery, runtime gate
```

What it does, using building blocks that exist today:

1. Create the environment file from the template, generating each secret with
   `openssl rand -hex 32`; set `ORIGIN` exactly; keep the file mode 0600.
2. `docker compose --env-file .env -f compose.yml up -d postgres` and wait for health.
3. Create the runtime role and database ACL (8.1).
4. `docker compose --profile maintenance run --rm database-admin migrate`, then
   `database-admin grant-runtime dynodoc_runtime`; `database-admin check-runtime`.
5. `docker compose up -d api web converter`; install the proxy configuration and
   certificate; reload the proxy.
6. `curl -fsS https://docs.example.org/healthz` and `/auth/options`: expect `development: false`.
7. Run the delivery check (`dynodoctl verify-delivery`, today `check-connectors.py`
   against a source checkout).

### 9.2 First-run acceptance (all profiles)

| Check | Expected |
|---|---|
| `/healthz` on web and API | 200 `status: ok` |
| `/auth/options` | `development: false`; the providers you configured are `true` |
| Anonymous `/api/engine/documents` | 401 |
| Sign in as the bootstrap administrator; open `/admin` | Overview loads; accounts counted |
| Create a document, edit, reload | Content persists; history shows events |
| `engine-cli check-runtime` | Passes |
| `engine-cli verify-snapshots --all` and `verify <id>` | Pass |
| Backup then restore into a scratch database (9.7) | Verified |
| Add-in manifests (7.2 master) served and byte-verified | `verify-delivery` passes |
| TLS | Chain valid for browsers and Office hosts |
| Response headers | One CSP, HSTS, `nosniff`; panes allow Office frames only |

### 9.3 Install, Profile B (VM or small cluster)

1. Provision PostgreSQL (primary and standby, or managed). Apply 8.1 and 8.2.
2. Create secrets in the customer's store; distribute the same `PASETO_LOCAL_KEY`,
   `WEB_SERVICE_KEY` and `SESSION_SECRET` to every replica.
3. From the operator host: migrate, grant, check (8.1).
4. Start API replicas (one at first; add replicas when fan-out ships or accept sticky
   streams), then web replicas; configure the load balancer (master 6.7).
5. Configure sign-in: SMTP relay (7.4) and an OIDC provider ([identity document](SELF-HOSTED-IDENTITY-AND-ACCESS.md));
   test with a non-administrator.
6. Configure PITR (9.6), metrics and alerts (section 10).
7. Run 9.2. Run one restore drill before go-live.
8. Distribute the add-ins through the customer's channel (master 7.4).

### 9.4 Install, Profile C (Kubernetes, air-gapped)

Prerequisite: a signed offline bundle and an internal registry.

```sh
# proposed
dynodoctl bundle verify dynodoc-1.0.0.bundle --public-key cosign.pub
dynodoctl bundle import dynodoc-1.0.0.bundle --registry registry.internal:5000 --project dynodoc
helm install dynodoc oci://registry.internal:5000/dynodoc/chart --version 1.0.0 \
     --namespace dynodoc -f values.yaml
```

| Step | Detail |
|---|---|
| Namespaces and policy | Dedicated namespace; default-deny NetworkPolicy; allow proxy to web, web to API and converter, API to PostgreSQL, web to IdP/SMTP/SIEM |
| Secrets | External secret operator or CSI driver; chart accepts only references |
| Migration | Helm pre-install and pre-upgrade `Job` runs `engine-cli migrate` and `grant-runtime`; owner secret mounted only into that Job |
| Pods | `runAsUser: 10001`, `readOnlyRootFilesystem`, `capabilities.drop: [ALL]`, `seccompProfile: RuntimeDefault`, `emptyDir` for `/tmp`; resource requests and limits from section 6 |
| Availability | `PodDisruptionBudget` (minAvailable 1 per tier), anti-affinity across nodes and zones, readiness on `/readyz` once it exists |
| Ingress | `proxy-buffering off`, `proxy-buffer-size 16k`, `proxy-read-timeout 3600`, `proxy-body-size 16m` (annotation names depend on the controller) |
| Certificates | Internal CA; cert-manager issuer or mounted secret |
| Authority key | Not installed. If enabled: KMS signer (P2-10) or a mounted secret with the checklist in master 8.3 |
| Verification | `helm test`, 9.2, and the egress-blocked check (`kubectl exec` curl to a public address must fail) |

### 9.5 Air-gapped delivery of a release

1. On the connected side: download bundle, signature, public key fingerprint page.
2. Verify the bundle signature against the fingerprint obtained **out of band** (printed
   in the contract pack, or the key from the previous bundle).
3. Move through the transfer path (data diode, scanned media). Record SHA-256 at both ends.
4. `dynodoctl bundle verify` on the inside; import to the registry; verify image digests
   match the manifest.
5. Never `docker pull` on the inside; deployments reference digests.

### 9.6 Backup

**What to back up.**

| Item | Method | Frequency | Notes |
|---|---|---|---|
| PostgreSQL | Logical: `pg_dump -Fc` (A). Physical: base backup plus WAL archive (B, C) | A: daily. B, C: weekly base, continuous WAL | The dump holds content, history, identities, audit, originals and the installation UUID |
| Secrets (`.env`, key files) | Secret store backup or sealed copy | On change | Not in the database dump; losing `PASETO_LOCAL_KEY` only ends sessions, losing the authority key is unrecoverable (master 8.3) |
| Receipt authority key | Two offline copies | Once | Never on the dump's backup path |
| Licence file | Copy | On change | |
| Proxy configuration and certificates | Config management | On change | |
| Bundle and previous images | Registry or archive | Per release | Needed for rollback targets |

**Logical backup (Profile A, existing script).** `backup.sh` writes
`dynodoc-<UTC>.dump`, checks `pg_restore --list`, writes `.sha256` and deletes dated
files older than 14 days. It assumes `/opt/dynodoc-demo` and the project's compose files;
`dynodoctl backup` generalises paths, adds encryption (`age` or `gpg` recipient set in
configuration), writes to a second target (mounted share, S3-compatible storage with
object lock where available) and records the result for the status page.

**Physical backup (Profile B, C), illustrative pgBackRest settings:**

```ini
# /etc/pgbackrest/pgbackrest.conf (illustrative; tuned by the customer's DBA)
[global]
repo1-path=/srv/pgbackrest
repo1-retention-full=4
repo1-retention-diff=14
repo1-cipher-type=aes-256-cbc
repo1-cipher-pass=<from secret store>
start-fast=y
archive-async=y
[dynodoc]
pg1-path=/var/lib/postgresql/16/main
```

Weekly full, daily differential, continuous WAL, `pgbackrest verify` weekly, restore
test monthly (B) or quarterly with evidence (C). Barman, WAL-G and cloud snapshots are
equivalent choices.

**Same-host backups are not disaster recovery.** The production record states that
off-site recovery is owner-deferred; a self-hosted installation must not repeat that
gap silently. The status page (proposed) shows "no off-host copy" as a warning.

**Objectives.** The customer sets RPO and RTO. Capabilities of the methods (design, not
promises): daily dump gives an RPO up to 24 hours; WAL archiving with a 60 s archive
timeout gives minutes; synchronous replication gives zero for acknowledged commits at
write-latency cost. RTO depends on database size and storage throughput and must be
measured by drill (master 9.5, U8).

### 9.7 Restore and restore drill

Restore into a **new** database, never over the live one (existing rule).

1. Provision an empty PostgreSQL 16 instance on an isolated network. Create the runtime
   role first (the dump contains GRANTs to it) and apply the database ACL
   (`REVOKE ALL ... FROM PUBLIC; GRANT CONNECT ...`); both pitfalls were hit in the
   schema 38 rehearsal.
2. Verify the dump: checksum, `pg_restore --list`.
3. `pg_restore -d dynodoc_restore ...` as the owner.
4. With the **same release** binary that took the dump (or newer, following the upgrade
   path): `engine-cli check-runtime`, `engine-cli verify-snapshots --all`,
   `engine-cli verify <document>` for each document (or a sample plus all
   shared-project documents), compare counts and digests with the last recorded values.
5. Start an API against it read-only in a sandbox (no mail key, no signing key) and run
   9.2 checks.
6. Record duration of each step, data size, and result. This is the evidence for the
   customer's recovery objective.

For a real recovery, after verification point the production API at the restored
database (or restore in place on a rebuilt host), keep the old volume until a day of
normal use has passed, and expect to lose writes made after the backup.

### 9.8 Upgrade with rehearsal

| Step | Proposed command | Existing equivalent |
|---|---|---|
| Verify and stage | `dynodoctl bundle verify` / `bundle import` | `sha256sum -c`; image build tagged `release-<sha7>` |
| Read the notes | release notes: migrations, class, expected time | `docs/operations/<release>.md` |
| Plan | `dynodoctl upgrade --plan` | Compare `_sqlx_migrations` with the candidate list |
| Backup | `dynodoctl backup --label before-upgrade` | `backup.sh`; keep `before-<release>` image tags |
| Rehearse | `dynodoctl rehearse` | Master 12.2 steps 5 to 9 |
| Window | Announce; proxy to maintenance page | nginx 503 maintenance on all sites |
| Activate | `dynodoctl upgrade` | Master 12.2 steps 10 to 15 |
| Verify | `dynodoctl check`, `verify-delivery` | 22 to 23 live checks |
| Record | Report written to the data directory | Release record |

Evidence record contents: release, previous release, migration list, rehearsal report,
backup file name and SHA-256, fingerprints before and after (counts and digests),
check results, rollback target, time to activation, user-visible downtime.

### 9.9 Hostname or origin change

Things that embed the origin and what to do:

| Item | Action |
|---|---|
| `ORIGIN`, `PUBLIC_API_BASE_URL`, proxy `server_name`, certificate | Change; recreate web and API |
| OIDC redirect URIs and SAML ACS URLs / entity IDs | Update at the IdP before the switch |
| Office manifests (six lines each, plus `SupportUrl`) | Regenerate (`dynodoctl addin build`), raise `Version`, redeploy through the catalog; users may need to remove the old add-in |
| Google add-ons (`DYNODOC_ORIGIN`, `urlFetchWhitelist`) | Regenerate and reinstall |
| Pairing grants (seven-day, origin-bound approval links) | Re-pair; old grants expire |
| Add-in local ledgers (IndexedDB per origin) | Unsent queued captures at the old origin are orphaned; ask users to push or export a sidecar first |
| `engine-cli` remote files | Update the base URL; pins are keyed by authority UUID and public key, not hostname, so receipt journals remain valid |
| Public view links | Old links break unless the old name keeps redirecting |
| Device keys and signed history | Unaffected (installation UUID unchanged) |
| Keep a redirect from the old name for a transition period | Do not reuse the old name for something else (cookie and manifest confusion) |

### 9.10 Rollback

| Level | When | Procedure |
|---|---|---|
| 1. Configuration | Bad setting | Restore the previous environment file; recreate the affected containers only |
| 2. Binary, compatible schema | After P2-5, every applied migration since the target is `expand` | `dynodoctl rollback`: start the previous images (digests recorded in the evidence record); rehearsed against the upgraded copy before release |
| 3. Restore | Today for any migration; later for `breaking` migrations | Restore the verified pre-upgrade backup into a new database, start the previous images against it, accept loss of post-backup writes. A forward fix is preferred |

Never point an older image at a newer schema without the compatibility mechanism; it
fails closed at start. Never delete migration records or edit checksums to make it boot.
Never rotate secrets or replace volumes as part of a rollback.

### 9.11 Disaster scenarios

| Scenario | Detection | Response | Data loss |
|---|---|---|---|
| Host or node loss (A) | Health alert; no status | Rebuild host, restore latest verified backup, restore secrets and key files, same origin | Up to the backup interval |
| Primary database loss (B, C) | Replication/health alerts | Promote standby (Patroni/operator) or PITR restore; point API; run 9.2 and `verify-snapshots --all` | Zero (sync) or minutes (async) |
| Site loss (C) | Monitoring | Promote DR site per customer runbook; update DNS; same installation identity continues | Replication lag |
| Corruption or ransomware | Verification failures, unexpected changes | Isolate; restore from a backup older than the incident into a new database; compare chain heads with any independently retained commitment | Window since last clean backup |
| Accidental administrator erasure | Audit row, user report | Erasure is irreversible by design; recover from a backup into a scratch database and re-import the project history (CLI `sync` or document export) | Per policy |
| Authority key lost | Startup failure when the key file is mounted and mismatched | No recovery today; new authority and re-pinning (master 8.3). With the epoch design deployed ([REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md)): revoke, recovery epoch, explicit client re-pin | None to content; receipt continuity is broken and visible |
| Secret leak | See 9.13 | | |
| Certificate expiry | Alert | Renew; reload proxy | None |
| Clock failure | Token errors | Fix time source; restart if needed | None |

### 9.12 Rotation

| Secret | Procedure | Impact |
|---|---|---|
| `SESSION_SECRET` | Change on every web replica, recreate | All browser sessions end; users sign in again; no data impact |
| `PASETO_LOCAL_KEY` | Change on every API replica, recreate (key ring later) | All sessions and project device sessions end; connector keys unaffected (stored as hashes) |
| `WEB_SERVICE_KEY` | Change on API and web together | Sign-in fails during the switch; sessions unaffected |
| `CONVERTER_KEY` | Change on web and converter together | `.doc` import unavailable during the switch |
| Runtime DB password | Create a new role, `grant-runtime NEWROLE`, switch `DATABASE_URL`, drop the old role (done on 11 October 2026 as `dynodoc_runtime_20261011`) | Brief restart |
| Owner password | Change; update the operator tool only | None |
| TLS certificate | Replace files, reload proxy | None |
| IdP client secret | Update provider file; restart web | SSO fails during the switch |
| SCIM token | Create new token, update IdP, revoke old | None if overlapped |
| Receipt authority key | Not rotatable until the separate rotation work ([REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md)) is deployed | See master 8.3 |

### 9.13 Suspected compromise

| Situation | Actions |
|---|---|
| Lost or stolen endpoint | Revoke the user's device keys (Profile, or admin "revoke sessions" which also revokes keys); end web sessions; review project activity for the window |
| Leaked `SESSION_SECRET` | Rotate; attacker needs a valid cookie payload and the secret to forge: treat all sessions as exposed |
| Leaked `PASETO_LOCAL_KEY` | Rotate immediately; review audit rows for `device.session_issued` and admin actions in the window |
| Leaked runtime DB credential | New role (9.12); review `operation_audit`; history rewrite is blocked by role and guards |
| Leaked owner credential | Rotate; assume guards could have been disabled: run `verify-snapshots --all` and `verify` for all documents; compare against an independent anchor if one exists |
| Compromised IdP | Disable the provider in configuration; bump account generations for affected users; restore access through email links or another IdP |
| Rogue administrator | Review `operation_audit`; admin actions require typed confirmation and are recorded; restore erased resources from backup if needed |

### 9.14 Decommissioning

1. Announce; freeze writes (maintenance page).
2. Offer exports: native downloads per document, project history through `engine-cli sync`
   to customer-controlled storage; verify with `engine-cli verify`.
3. Take a final backup; hold it per the customer's retention rule.
4. Remove manifests from catalogs and ask users to remove the add-ins; local ledgers on
   devices persist until cleared.
5. Destroy the database volume and WAL archives per policy; destroy secrets and the
   authority key; record the destruction.
6. Retire the licence and DNS names; keep the old name from being reused.

---

## 10. Observability

### 10.1 What exists

| Signal | Today | Source |
|---|---|---|
| Liveness | `GET /healthz` on API (`{"status":"ok","service":"engine-api"}`) and web | `lib.rs`, `routes/healthz` |
| Logs | `tracing_subscriber::fmt` text to stdout; `RUST_LOG` filter; `TraceLayer` per request | `main.rs`, `lib.rs` |
| Web access log | nginx, safe format (private-link paths redacted, no query strings) | `nginx-app.conf` |
| Operator view | `/admin` overview: counts, table sizes, connections, migrations, web vitals; host JSON | `admin.rs`, `collect-status.py` |
| Audit | `operation_audit` rows through the admin records API | `admin.rs` |
| Tracing | None exported; the code comment says "OpenTelemetry-compatible" | `lib.rs` |

### 10.2 Proposed signals (P2-6, P2-7, P2-14)

**Endpoints.** `/healthz` (liveness, unchanged), `/readyz` (pool, schema gate, listener,
lease), `/metrics` on a separate bind address (`METRICS_ADDR`, default loopback) so it is
never on the public listener.

**Metrics** (Prometheus names, bounded labels; no per-document or per-user labels):

| Metric | Type | Labels | Use |
|---|---|---|---|
| `dynodoc_http_requests_total` | counter | `route` (template), `method`, `status` | Error rate |
| `dynodoc_http_request_duration_seconds` | histogram | `route`, `method` | Latency |
| `dynodoc_admission_rejections_total` | counter | `pool` (bodies, streams, expensive, identity, project) | Limits |
| `dynodoc_db_pool_connections` / `_idle` / `dynodoc_db_pool_wait_seconds` | gauge / histogram | | Saturation |
| `dynodoc_db_statement_timeouts_total` | counter | `kind` | Slow queries |
| `dynodoc_sse_streams` | gauge | | Live connections |
| `dynodoc_stream_delivery_lag_seconds` | histogram | | Commit to delivery |
| `dynodoc_listener_connected` | gauge | | Fan-out health |
| `dynodoc_job_lease_held` | gauge | `job` | Exactly one holder |
| `dynodoc_events_appended_total` | counter | | Write rate |
| `dynodoc_auth_events_total` | counter | `kind` (login, refresh, logout, invalid, revoked), `provider` | Security and usage |
| `dynodoc_receipt_sign_seconds` | histogram | | Signer latency (KMS) |
| `dynodoc_web_mail_total` | counter | `result` | Mail health |
| `dynodoc_license_days_remaining` | gauge | | Renewal (licensing document) |
| `dynodoc_backup_age_seconds`, `dynodoc_restore_drill_age_seconds` | gauge | | Fed by `dynodoctl` |

**Logs.** `LOG_FORMAT=json`: one object per line with `ts`, `level`, `target`, `msg`,
`request_id`, `route` (template), `status`, `duration_ms`, `identity_id` (UUID, never
email), `session_id` (hashed). Never log bearer tokens, cookies, magic-link tokens,
connector keys, challenges, MACs or signatures (a rule the device-session record already
states for request logs). Add `request_id` end to end (proxy to web to API).

**Audit export.** `operation_audit` and review/receipt events as JSON lines, syslog
(RFC 5424) or CEF, delivered by pull (read API with cursor) or push (webhook, syslog
over TLS). Fields: time, actor id, action, target type and id, outcome, request id,
digests of before/after for settings (already how audit rows work). No document
content.

### 10.3 Alerts (recommended thresholds; customers tune)

| Alert | Condition | Severity |
|---|---|---|
| Service down | Readiness failing 2 minutes on all replicas of a tier | Page |
| 5xx rate | Above 2% for 5 minutes | Page |
| Database unreachable | Pool errors, `pg_up` 0 | Page |
| Listener down | `dynodoc_listener_connected` 0 for 1 minute (fan-out mode) | Ticket |
| Backup stale | `dynodoc_backup_age_seconds` above 26 hours (daily) | Ticket |
| Restore drill stale | Above 35 days (B) or 100 days (C) | Ticket |
| WAL archive failing | `pg_stat_archiver.failed_count` increasing | Page |
| Replication lag | Above 30 seconds for 5 minutes | Ticket |
| Disk | Free below 20% (ticket), below 10% (page) | |
| Certificate | Expiry within 21 days | Ticket |
| Licence | Fewer than 30 days remaining | Ticket |
| Auth anomalies | Refresh failures above 5% | Ticket |
| NOTIFY queue | `pg_notification_queue_usage()` above 0.1 | Ticket |
| Pool wait | Above 200 ms p95 for 10 minutes | Ticket |

---

## 11. Failure modes and troubleshooting

Known regression traps are taken from the repository's troubleshooting notes and release
records.

| Symptom | Likely cause | Fix |
|---|---|---|
| 502 only on signed-in pages through the proxy; direct web container returns 200 | Response headers (asset preload `Link` plus rotated cookies) exceed the default 4 KiB upstream buffer | `proxy_buffer_size 16k; proxy_buffers 8 16k; proxy_busy_buffers_size 32k` ([WORKSPACE-502-FIX.md](WORKSPACE-502-FIX.md)) |
| Every POST returns `Invalid origin` (403) | `ORIGIN` differs from the browser's origin (scheme, host, port, trailing slash), or the proxy changes `Host` | Set `ORIGIN` exactly; preserve `Host`; send `X-Forwarded-Proto` |
| API exits: `MIGRATION_DATABASE_URL must not be present` | Owner URL in the API environment | Remove it; operator tool only |
| API exits: `database schema differs from this binary` | Image does not match the applied migrations (older image on newer schema, or the reverse) | Use the matching build; never edit migration records; restore if needed |
| API exits: runtime role has DDL/temporary-object/ownership issues | Role has extra privileges or the database ACL still grants PUBLIC | Re-create the role without privileges; `REVOKE ALL ON DATABASE ... FROM PUBLIC`; run `grant-runtime`, `check-runtime` |
| API exits: `invalid configuration (N problems)` followed by lines such as `WEB_SERVICE_KEY of at least 32 characters is required`, `PASETO_LOCAL_KEY must be 32 bytes`, `X and X_FILE are both set` or `X_FILE (path): cannot open` | Missing, malformed or doubly specified secret, unreadable secret file, or an out-of-range `RETENTION_*` value | Fix each listed line; generate secrets per 7.5; see "Secret files" in 7.1 |
| Users logged out at random with several web replicas | Different `SESSION_SECRET` per replica, or the refresh race | Same secret everywhere; sticky routing or reuse interval (master 6.4) |
| Editors do not see each other's changes live, page reload shows them | Several API replicas without event fan-out | Enable `DYNODOC_EVENT_FANOUT=postgres` (when shipped) or run one API replica |
| Add-in pane blank in Office | Untrusted certificate, mixed content, CSP blocking Office.js (CDN not reachable), wrong `CONNECTOR_ORIGINS` | Check TLS trust on the endpoint; use a local Office.js path; run `verify-delivery` |
| Add-in message: Dynodoc updated its document history, reload | Cached old pane without the `office-v5` header | Reload; do not remove the gate |
| Sign-in link opens but fails | Link older than 15 minutes, used twice, or a scanner consumed it (the page uses a POST confirmation to defeat prefetch) | Request another link |
| Google `redirect_uri_mismatch` | Callback not registered | Register `https://<origin>/auth/google/callback` exactly |
| 429 with `Retry-After` | Admission or rate window | Wait; investigate the client; raise limits only with evidence |
| 507 with `storage_limit` | Per-document or per-account object budget reached | Operator increase per [SHARED-STORAGE-BUDGETS.md](SHARED-STORAGE-BUDGETS.md); not by deleting |
| 504 on search or audit | 2 s or 15 s statement timeout on a large corpus | Narrow queries; plan indexed search; check for lock waits |
| `.doc` upload returns busy or fails | Converter slots (2), 3,000,000-byte cap, 45 s timeout | Retry; save as DOCX |
| Database stops accepting writes | Disk full | Free space or extend volume; do not delete WAL by hand |
| Token errors across nodes | Clock skew | Fix NTP |
| `verify` reports a chain mismatch | Corruption or tampering | Stop; preserve; restore a clean backup into a new database; compare to anchors |

---

## 12. Security hardening and threat view

### 12.1 Checklist

| Area | Item | Source or status |
|---|---|---|
| Host | Dedicated VM or namespace; unrelated applications not on the same Docker daemon; no `docker compose down -v` | HANDOFF operational boundary |
| Containers | Non-root users; `read_only`; `cap_drop: [ALL]`; `no-new-privileges`; `tmpfs` `/tmp`; PID, memory, CPU limits | EXISTS in `compose.yml` |
| Networks | `engine` and `conversion` `internal: true`; only the proxy publishes ports | EXISTS |
| Secrets | Files with mode 0400/0600; no secrets in images, environment dumps, `docker compose config` output or logs | Partly documented |
| Database | Separate runtime and owner logins; TLS; no PUBLIC privileges; backups encrypted | EXISTS (roles); TLS and encryption proposed |
| Proxy | One CSP, one HSTS, `nosniff`, referrer policy; request and header timeouts; safe access log | EXISTS in templates |
| Authentication | `DEV_AUTH=false`; email links disabled if SSO is mandatory; admin list minimal | Config |
| Add-ins | Trusted certificate; origin allowlist; manifests generated, not hand-edited | Proposed |
| Updates | Verify signatures; rehearse; keep rollback images by digest | Proposed |
| Monitoring | Alerts in 10.3; audit export to SIEM | Proposed |
| Clients | Full-disk encryption; key files permissioned; device key grants narrowed to needed projects and scopes | Docs for users |
| Physical | Backups and key escrow in separate locations with separate custodians | Customer |

### 12.2 Threat view for the deployment

| Threat | Example | Controls | Residual risk |
|---|---|---|---|
| Exposed internal ports | API or PostgreSQL reachable from user networks | Internal networks; firewall rules (5.5); no published ports except the proxy | Misconfiguration; check with a port scan in acceptance |
| Secret disclosure | `.env` in a ticket, `docker compose config` output, image layer | Files with restrictive modes; `*_FILE` secrets; no secrets in images or Helm values; secret scanning | Operator error |
| Backup theft | Dump contains all content and the history | Encryption at rest; off-host copy access control; short retention | Key custody |
| Database administrator abuse | Disabling append-only triggers | Separate owner credential; `verify-snapshots --all` after maintenance; external anchors (open work) | A privileged insider can rewrite undetectably without an anchor |
| Proxy misconfiguration | CSP weakened; buffers too small; SSE buffered | Templates; `verify-delivery` header checks | Customer edits |
| Stale or wrong restore | Restoring an old dump over production, or giving a copy the authority key | Restore only into a new database; key never mounted on copies | Process discipline |
| Supply-chain substitution | Unsigned image pulled by tag | Digests; offline verification; internal registry only | Key compromise |
| Denial of service | Slow readers, expensive reads, upload floods | Admission limits, proxy limits, container limits | No capacity proof |
| Clock manipulation | Tokens and licences misbehave | NTP; monitoring | Air-gapped drift |

---

## 13. Delivery tasks, acceptance and test strategy

Estimates and phases are owned by the [master design, section 13](SELF-HOSTED-EDITION.md).
Tasks that deliver this guide's content:

| Task | PW | Conf. | Depends on | What it delivers here |
|---|---|---|---|---|
| P0-2 | 1.5 | H | none | Section 7 schema, validation, `*_FILE` |
| P0-4 | 3.0 | M | P0-1, P0-2 | Appendix A compose and nginx template |
| P0-5 | 5.0 | M | P0-4, P0-7 | `dynodoctl` in sections 9.1, 9.6 to 9.8, 9.12 |
| P0-9 | 2.0 | M | P0-4 | Egress-blocked acceptance (9.2, section 12) |
| P0-10 | 1.5 | H | P0-4, P0-5 | This guide's Profile A path, tested by a fresh reader |
| P2-6 | 3.5 | H | none | Section 10 |
| P2-8 | 4.5 | M | P2-5, P0-6 | Appendix B Helm chart and 9.4 |
| P2-9 | 3.0 | M | P0-5 | 9.6 physical backup, 9.7 drills |
| P2-14 | 1.5 | H | none | `/readyz`, graceful drain, configurable pool |

Basis and assumptions are those of master 13.1 (person-week of focused work, a team that
knows the repository, three engineers at 75% utilisation for calendar figures).
**Excluded:** customer-specific installation and onboarding effort, cloud and hardware
costs, third-party tool licences, external audit or penetration-test fees, writing
vendor-specific guides (cloud providers, particular hypervisors), translation of the
guide, and the receipt-authority rotation, formula-engine replacement, AI-agent
connector and end-to-end encryption work.

**Rollout and migration.** The tooling ships to design partners first (master 15). Hosted
production keeps its present scripts until `dynodoctl` reproduces every step of its
release routine and a release has been executed by both paths on the same dataset and
produced identical fingerprints. Existing hosted data is not migrated; `infra/demo`
remains the hosted reference.

**Acceptance per profile.**

| Profile | Acceptance |
|---|---|
| A | Section 9.2 passes on a clean VM with no outbound network using a signed bundle; restore drill completes from the backup target; upgrade with rehearsal-lite succeeds; a reader unfamiliar with the project completes install in under 60 minutes using this guide only |
| B | A, plus: two API and two web replicas pass the master 6.8 harness; PITR restore to a chosen timestamp verified; SSO sign-in and group mapping work; alerts fire in a test; one rolling upgrade rehearsed |
| C | B, plus: Helm install from an offline bundle into a cluster with default-deny network policy and no egress; 3-node PostgreSQL failover under write load with no acknowledged write lost; KMS-held authority key verified (if enabled); audit events visible in the customer's SIEM; documented evidence pack |

**Test strategy.** Each runbook is executed in CI or a staging environment by someone
other than its author; commands are copy-paste tested; failure injection for each row of
sections 9.11 and 11 that can be simulated (kill a replica, fill a disk, expire a
certificate, skew a clock, drop the listener, revoke the database ACL).

---

## 14. Risks and open questions

### 14.1 Risks

| ID | Risk | Mitigation |
|---|---|---|
| DR1 | Runbooks drift from the product | Run them in CI; version with the bundle |
| DR2 | Customers skip rehearsal | `dynodoctl upgrade` refuses without a current report (override is recorded) |
| DR3 | Same-host backups taken as recovery | Status warning; documentation; backup target is a mandatory install question |
| DR4 | Proxy templates are weakened by customer edits (CSP, buffers) | `verify-delivery` checks headers; support bundle includes effective proxy test results |
| DR5 | Internal CA not trusted by Office or `engine-cli` hosts | Pre-install connectivity test from a client |
| DR6 | PgBouncer in transaction mode placed in front of PostgreSQL silently breaks LISTEN | Readiness requires a successful self-notification test |
| DR7 | PostgreSQL major version upgrades | Not covered; PostgreSQL 16 only; a separate qualification project per major |

### 14.2 Open questions (deployment-specific; shared questions are Q1 to Q23 in the master)

| # | Question | Recommended default | Decider |
|---|---|---|---|
| DQ1 | Implementation language and packaging of `dynodoctl` | A single static binary (Rust, in the private bundle repository) so Profile A has no Python or Node requirement on the host | Engineering lead |
| DQ2 | Bundled proxy: nginx or Caddy | nginx (existing CSP maps and tests); revisit if ACME automation is the main pain | Engineering lead |
| DQ3 | Compose or systemd for single node | Compose | Engineering lead |
| DQ4 | Ship PostgreSQL in the bundle for B | No; document supported external options | Engineering lead |
| DQ5 | Default backup target for A | Local second disk plus a prompt to configure an off-host target during install | Owner |
| DQ6 | Which Kubernetes distributions are tested | Upstream via kind in CI, k3s as the small-cluster reference | Engineering lead |

---

## 15. Appendices

### Appendix A: Profile A Compose project (proposed, abridged)

*The built project is `infra/selfhosted/compose.yml` (22 October 2026); this sketch is older and
differs from it as follows.* Secrets are not Compose `secrets:` (Compose ignores `uid`, `gid` and
`mode` for file secrets, so a 0600 file is unreadable to the container user): the job
`secrets-init` copies them into one volume per service, owned by that service's user, mode 0400.
Four networks plus `published`; a `tools` image runs `preflight`; the `database-admin` and
`mailcheck` services are on demand; the proxy is a built image (`images/Dockerfile.proxy`), not
a bare nginx; images are referenced as `${DYNODOC_IMAGE_REGISTRY}/name:${DYNODOC_VERSION}` and a
generated overlay (`scripts/make-release-overlay.mjs`) pins them by digest. The sketch below is
left as the original design record.

```yaml
# docker-compose.yml, release 1.0 (proposed). Images pinned by digest; no build section.
name: dynodoc
x-hardening: &hardening
  read_only: true
  cap_drop: [ALL]
  security_opt: [no-new-privileges:true]
  tmpfs: ["/tmp:size=64m"]
services:
  proxy:
    image: registry.example/dynodoc/proxy@sha256:<digest>
    restart: unless-stopped
    ports: ["80:80", "443:443"]
    volumes: ["./tls:/etc/dynodoc/tls:ro"]
    networks: [edge]
    depends_on: [web]
    <<: *hardening
  web:
    image: registry.example/dynodoc/web@sha256:<digest>
    restart: unless-stopped
    environment:
      ORIGIN: ${ORIGIN:?}
      ENGINE_API_URL: http://api:8080
      CONVERTER_URL: http://converter:8090
      DYNODOC_MODE: selfhosted
      DEV_AUTH: "false"
      SESSION_SECRET_FILE: /run/secrets/session_secret
      WEB_SERVICE_KEY_FILE: /run/secrets/web_service_key
      CONVERTER_KEY_FILE: /run/secrets/converter_key
    secrets: [session_secret, web_service_key, converter_key]
    networks: [edge, engine, conversion]
    depends_on: [api]
    mem_limit: 1g
    cpus: 2
    pids_limit: 128
    <<: *hardening
  api:
    image: registry.example/dynodoc/api@sha256:<digest>
    restart: unless-stopped
    environment:
      API_HOST: 0.0.0.0
      PUBLIC_API_BASE_URL: ${ORIGIN:?}
      TOKEN_TTL_SECONDS: "3600"
      DATABASE_URL_FILE: /run/secrets/runtime_database_url
      PASETO_LOCAL_KEY_FILE: /run/secrets/paseto_local_key
      WEB_SERVICE_KEY_FILE: /run/secrets/web_service_key
    secrets: [runtime_database_url, paseto_local_key, web_service_key]
    networks: [engine]
    depends_on:
      postgres: {condition: service_healthy}
    mem_limit: 2g
    cpus: 2
    pids_limit: 128
    <<: *hardening
  postgres:
    image: postgres:16@sha256:<digest>
    restart: unless-stopped
    environment:
      POSTGRES_USER: dynodoc
      POSTGRES_DB: dynodoc
      POSTGRES_PASSWORD_FILE: /run/secrets/postgres_password
      TZ: UTC
    secrets: [postgres_password]
    volumes: ["database:/var/lib/postgresql/data"]
    networks: [engine]
    healthcheck:
      test: [CMD-SHELL, "pg_isready -U dynodoc -d dynodoc"]
      interval: 5s
      timeout: 3s
      retries: 20
  converter:
    profiles: [converter]
    image: registry.example/dynodoc/converter@sha256:<digest>
    restart: unless-stopped
    environment: {CONVERTER_KEY_FILE: /run/secrets/converter_key}
    secrets: [converter_key]
    networks: [conversion]
    tmpfs: ["/tmp:size=256m"]
    mem_limit: 768m
    cpus: 1
    pids_limit: 128
    read_only: true
    cap_drop: [ALL]
    security_opt: [no-new-privileges:true]
  database-admin:
    profiles: [maintenance]
    image: registry.example/dynodoc/api@sha256:<digest>
    entrypoint: [engine-cli]
    environment:
      MIGRATION_DATABASE_URL_FILE: /run/secrets/migration_database_url
      DATABASE_URL_FILE: /run/secrets/runtime_database_url
    secrets: [migration_database_url, runtime_database_url]
    networks: [engine]
    <<: *hardening
networks:
  edge: {}
  engine: {internal: true}
  conversion: {internal: true}
volumes:
  database: {}
secrets:
  session_secret:        {file: ./secrets/session_secret}
  web_service_key:       {file: ./secrets/web_service_key}
  converter_key:         {file: ./secrets/converter_key}
  paseto_local_key:      {file: ./secrets/paseto_local_key}
  postgres_password:     {file: ./secrets/postgres_password}
  runtime_database_url:  {file: ./secrets/runtime_database_url}
  migration_database_url: {file: ./secrets/migration_database_url}
```

Differences from `infra/demo/compose.yml`: no `build:`; no host-specific mounts; a proxy
container; secrets as files with the proposed `*_FILE` convention; no separate admin
service; the converter is optional.

### Appendix B: Helm values sketch (proposed, abridged)

```yaml
# values.yaml (proposed)
global:
  origin: https://docs.example.org
  imageRegistry: registry.internal:5000/dynodoc
  imagePullSecrets: []
mode: self-hosted
airgap: true
database:
  external:
    host: pg-rw.db.svc
    port: 5432
    name: dynodoc
    sslMode: verify-full
    caSecret: pg-ca
  runtimeUrlSecret: {name: dynodoc-runtime-db, key: url}      # API pods only
  migrationUrlSecret: {name: dynodoc-owner-db, key: url}      # migration Job only
secrets:
  pasetoLocalKey: {name: dynodoc-core, key: paseto}
  webServiceKey:  {name: dynodoc-core, key: service}
  sessionSecret:  {name: dynodoc-core, key: session}
api:
  replicas: 3
  resources: {requests: {cpu: "1", memory: 1Gi}, limits: {cpu: "2", memory: 2Gi}}
  eventFanout: postgres
  rateLimitBackend: postgres
web:
  replicas: 2
  resources: {requests: {cpu: 500m, memory: 512Mi}, limits: {cpu: "1", memory: 1Gi}}
ingress:
  className: nginx
  annotations:
    nginx.ingress.kubernetes.io/proxy-buffering: "off"
    nginx.ingress.kubernetes.io/proxy-buffer-size: "16k"
    nginx.ingress.kubernetes.io/proxy-read-timeout: "3600"
    nginx.ingress.kubernetes.io/proxy-body-size: "16m"
  tls: {secretName: dynodoc-tls}
converter: {enabled: false}
receiptAuthority: {mode: none}        # none | file | kms
metrics: {enabled: true, serviceMonitor: false}
networkPolicy: {enabled: true}
podDisruptionBudget: {enabled: true}
migrations: {hook: true}              # pre-install, pre-upgrade Job: migrate + grant-runtime
```

### Appendix C: glossary

| Term | Meaning |
|---|---|
| ACL | Access control list; here also PostgreSQL database-level privileges |
| CAB | Change advisory board |
| Fan-out | Delivering one committed event to every live subscriber on every replica |
| `dynodoctl` | Proposed operator tool (name provisional) |
| Installation UUID | Immutable identifier of one database lineage (`device_authority`) |
| Operator login | PostgreSQL owner or migration credential; used only by `engine-cli` maintenance commands |
| PITR | Point-in-time recovery from a base backup plus archived WAL |
| Rehearsal | Running the candidate release against a restored copy before activation |
| Runtime login | Restricted login used by the API |
| SSE | Server-sent events (the live document stream) |
| WAL | PostgreSQL write-ahead log |

Further terms are in the [master design](SELF-HOSTED-EDITION.md), appendix A.

### Appendix D: references

| Topic | Reference | Status |
|---|---|---|
| PostgreSQL NOTIFY | https://www.postgresql.org/docs/16/sql-notify.html | Verified 14 Oct 2026 |
| PgBouncer feature compatibility (LISTEN not in transaction mode) | https://www.pgbouncer.org/features.html | Verified via search summary |
| pgBackRest | https://pgbackrest.org/ | Unverified; check at build time |
| PostgreSQL privileges | https://www.postgresql.org/docs/16/ddl-priv.html | Cited by the repository |
| Helm OCI registries (Helm 3.8 and later) | https://helm.sh/docs/topics/registries/ | Version statement verified via search; page not fetched |
| Office add-in deployment | See master appendix C | Verified |
