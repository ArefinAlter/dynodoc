# Dynodoc self-hosted bundle (Profile A: one machine)

Status, 22 October 2026: the first build of the single-node bundle (task P0-4 of the
[self-hosted edition plan](../../docs/SELF-HOSTED-EDITION.md)). It is source and tests in this
repository. It is not released, not signed and has no installer (`dynodoctl` is a later task), so
today it is for design partners and evaluation. What was tested, and what was not, is listed at the
end.

It runs Dynodoc on one Linux machine with Docker: the web application, the engine API, PostgreSQL
and an nginx proxy that terminates TLS. Nothing is sent to Verne or to any third party. The only
outbound connections are the ones you choose: a mail server for sign-in links, and, if you pick
automatic certificates, the certificate authority.

```
 browsers, Office add-ins            published on DYNODOC_BIND (default 127.0.0.1)
        |  HTTPS 443 / HTTP 80 (redirect)
   +----v-------+  frontend (internal)   +-------+  engine (internal)   +-----+     +----------+
   |   proxy    |----------------------->|  web  |--------------------->| api |---->| postgres |
   |   nginx    |                        +---+---+                      +-----+     +----------+
   +------------+                            |  egress (mail server, identity provider later)
                                             v
                                        your network
```

| Service | What it is | Runs as | Reachable from |
|---|---|---|---|
| `proxy` | nginx: TLS, redirect to HTTPS, HSTS, the content-security-policy maps of the hosted service, request limits, safe access log | uid 101 | The only published ports |
| `web` | The SvelteKit application (`DYNODOC_MODE=selfhosted`) | uid 10001 | `proxy` only |
| `api` | `engine-api`, with the restricted database login only | uid 10001 | `web` only |
| `postgres` | PostgreSQL 16, passwords on every connection (SCRAM) | uid 999 | `api` and the jobs only |
| `secrets-init`, `preflight`, `db-roles`, `init` | One-shot jobs, run in this order on every `up` and stop the stack if they fail (below) | see below | none or `postgres` |
| `acme` | Optional: automatic certificates (profile `acme`) | uid 101 | the certificate authority |
| `converter` | Optional: legacy `.doc` import (profile `converter`) | uid 10001 | `web` only |
| `database-admin` | On demand: `engine-cli` with the owner login (profile `maintenance`) | uid 999 | `postgres` |

The jobs, in order, each of which must succeed before the next starts:

1. `secrets-init` copies your secret files into one small volume per service, with the right owner
   and mode (`0400`), so each service sees only its own secrets. It is the only container that reads
   your `secrets/` directory, and it has no network.
2. `preflight` checks `.env` and the secrets with the same validator the web server runs, before
   anything else is touched. No network.
3. `db-roles` creates the restricted runtime login (or sets its password to the current secret).
4. `init` runs `engine-cli migrate` as the database owner, then `grant-runtime`, then
   `check-runtime`. The API and web never receive the owner password.

## Before you start

- A Linux machine (x86-64; arm64 images build but are not tested) with Docker Engine and Compose
  **v2.24 or newer** (`docker compose version`). Docker Desktop works for evaluation.
- About 4 GB of memory and 10 GB of disk for a trial. Idle, the four services use about 110 MiB;
  their limits add up to 5.25 GiB (`web` 1 GiB, `api` 2 GiB, `postgres` 2 GiB, `proxy` 256 MiB).
- Node 20 or newer, **or** the `tools` image (below), to create the settings files.
- For anything beyond a trial: a DNS name for the machine, and an outbound mail server (SMTP) so
  people can receive sign-in links. See "The first administrator".
- Clocks kept in step with NTP. Tokens and certificates depend on it.

## Quick start

Everything is done in the **bundle directory**: the directory that holds `compose.yml`. In a source
checkout that is `infra/selfhosted`. In a release it is wherever you unpacked it.

**1. Get the images.** From a source checkout (the first build compiles the API and takes a long
time, about 25 minutes on a 12-core laptop; the second is cached):

```sh
docker compose -f compose.yml -f compose.build.yml build
```

From a registry (a release): set `DYNODOC_IMAGE_REGISTRY` and `DYNODOC_VERSION` in `.env`, add the
generated `compose.release.generated.yml` to every `docker compose` command below
(`-f compose.yml -f compose.release.generated.yml`), and run `docker compose pull`.

**2. Create the settings and the secrets.** With Node:

```sh
node ../../frontend/researcher/scripts/dynodoc-env.mjs generate-env --dir . \
  --origin https://docs.example.org --container-secrets-dir /run/secrets \
  --org-name "Example Ltd." --smtp-host smtp.example.org --smtp-from docs@example.org
```

Without Node, with the `tools` image (on Linux, run it as yourself so you own the files):

```sh
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/bundle" dynodoc-local/tools:dev \
  node scripts/dynodoc-env.mjs generate-env --dir /bundle \
  --origin https://docs.example.org --container-secrets-dir /run/secrets \
  --org-name "Example Ltd." --smtp-host smtp.example.org --smtp-from docs@example.org
```

This writes `.env` (settings and the paths of the secrets, no secret value) and `secrets/` (one
random value per file, mode 0600). It never overwrites anything. **Back up `.env` and `secrets/`
somewhere separate from the server**: without them an installation cannot be started again. The
command lists what each file protects. Options for the mail server (`--smtp-user`, `--smtp-port`,
`--smtp-security`, `--smtp-ca-file`) and `--with-converter` are in the
[deployment guide](../../docs/SELF-HOSTED-DEPLOYMENT-GUIDE.md), section 7.2. `ORIGIN` is exactly what
people will type in the browser, with the port if it is not 443.

**3. Choose how the proxy gets its certificate** and who administers it. Append to `.env`:

```sh
cat >> .env <<'EOF'
DYNODOC_TLS_MODE=evaluation
ADMIN_EMAILS=you@example.org
EOF
```

`DYNODOC_TLS_MODE` has no default: `evaluation`, `provided` or `acme` (next section). For a trial on
your own computer use `evaluation` with `ORIGIN=https://localhost:8443` and add
`DYNODOC_HTTPS_PORT=8443` and `DYNODOC_HTTP_PORT=8080` (ports below 1024 need privileges on some
systems). The other bundle settings are in `bundle.env.example`.

**4. Check the mail server** (skip if you set no `--smtp-host`; email sign-in is then off):

```sh
docker compose run --rm mailcheck --to you@example.org
```

It does the real handshake (TLS, login, sender, recipient) from the same network position as the web
server and stops before sending any text. Add `--send` to send one test message. It prints what to
change when the server refuses (a wrong port or security mode, an untrusted certificate, a rejected
sender); the messages are listed in the deployment guide, section 7.2.

**5. Start it.**

```sh
docker compose up -d --wait
```

The first start creates the database and takes about two minutes. If a job fails, the command
stops and says which one; `docker compose logs <job>` has the reason in plain words (see
"Troubleshooting").

**6. Open it.** `https://<your ORIGIN>/`. In `evaluation` mode the browser warns about the
certificate; that is expected. Sign in with the address you put in `ADMIN_EMAILS`.

**7. Look at it** (optional): `docker compose ps` shows every service healthy;
`curl -k https://localhost:8443/healthz` answers `{"status":"ok",...}`.

## The first administrator

The people who may use `/admin` are the addresses listed in `ADMIN_EMAILS` in `.env` (comma
separated). Today there is no invitation-only mode and no operator-issued sign-in link (that is part
of task P0-3, not built). What that means, as the code stands:

- To sign in, a person asks for an email link on the sign-in page and clicks it. The link only
  arrives if the installation can send mail (SMTP configured and working). **Without a working
  mail transport nobody can sign in**, the first administrator included. The web server refuses to
  start with email sign-in on and no transport, and `generate-env` without `--smtp-host` turns email
  sign-in off, which also means nobody can sign in. Google sign-in is the only other method today
  and needs internet access.
- The first request for a link **creates the account**. Anyone who can reach the sign-in page and
  read mail at an address can make an account (they see only their own documents). Until the
  identity features of Phase 1 exist, limit who can reach the installation (`DYNODOC_BIND`, a
  firewall, a VPN) instead of relying on the sign-in page.
- An address in `ADMIN_EMAILS` gets the administration page once it has signed in that way. Its
  owner proves the address by receiving the link.
- `DEV_AUTH=true` makes the sign-in page show the link instead of mailing it. It exists for test
  stacks. Anyone who can reach the page can then sign in as anyone, administrators included. Do
  not use it on a machine other people can reach.

## The three certificate modes

| `DYNODOC_TLS_MODE` | Use it for | What you provide | Certificate trusted by browsers and Office |
|---|---|---|---|
| `evaluation` | Trying it out on one computer | nothing | No. Self-signed, labelled "EVALUATION - NOT FOR PRODUCTION" |
| `provided` | A company certificate, an internal CA, an air-gapped site | `tls/fullchain.pem` and `tls/privkey.pem` | Yes, if your CA is trusted by the clients |
| `acme` | A public name reachable from the internet | an email address, acceptance of the CA's terms | Yes (Let's Encrypt or another ACME CA) |

Whatever the mode, the proxy never invents a production certificate by itself, and it refuses to
start rather than serve a certificate it cannot vouch for.

**`provided`.** Put your certificate chain (the server certificate first, then the intermediates) in
`tls/fullchain.pem` and the private key (PEM, no passphrase) in `tls/privkey.pem`. They stay private
to you on the host; `docker compose up` copies them into the proxy's own volume. At start the proxy
checks that the key belongs to the first certificate, that the certificate is current (and warns in
the last 30 days), that it covers the name in `ORIGIN` and in `DYNODOC_ALT_ORIGINS`, that each
certificate was issued by the next one, and that the key is readable without a passphrase. If
anything is wrong it prints every problem in plain words and exits, so `docker compose up --wait`
fails; nothing is listening. You can check the files without starting anything:

```sh
docker compose run --rm --no-deps secrets-init      # copy tls/ into the proxy's volume
docker compose run --rm --no-deps proxy check       # validate; changes nothing
```

*Renewing without downtime.* Replace `tls/fullchain.pem` and `tls/privkey.pem`, then run the two
commands above followed by `docker compose exec proxy proxy-entrypoint reload`. The proxy checks the
new files first; if they are wrong it says so and **keeps serving the old certificate**; if they are
right nginx reloads gracefully and no connection is dropped. (Even without the last command the proxy
notices a changed file within a minute, `DYNODOC_TLS_RELOAD_SECONDS`.)

**`evaluation`.** At first start the proxy makes a self-signed certificate for the name in `ORIGIN`
(plus `localhost`, `127.0.0.1` and `::1`), valid 90 days. It says so loudly: the certificate's
subject reads "Dynodoc EVALUATION - NOT FOR PRODUCTION", every response carries
`X-Dynodoc-Tls-Mode: evaluation; ...`, the proxy log opens with a banner, and HSTS is limited to five
minutes so a trial cannot pin your browser to a certificate that will be thrown away. It is reused
across restarts and replaced automatically, without downtime, when less than 30 days remain. Office
add-ins and `engine-cli` reject it by design.

**`acme`.** Needs a public DNS name that points at this machine, and ports 80 and 443 reachable from
the internet: set `DYNODOC_BIND=0.0.0.0` (or the public address). Add to `.env`:

```
DYNODOC_TLS_MODE=acme
COMPOSE_PROFILES=acme
DYNODOC_ACME_EMAIL=admin@example.org
DYNODOC_ACME_ACCEPT_TOS=true
DYNODOC_ACME_CA=letsencrypt-staging
```

Read the certificate authority's terms before setting `DYNODOC_ACME_ACCEPT_TOS=true`; nothing is
requested until you do, and there is no default authority. Try `letsencrypt-staging` first (its
certificates are not trusted by browsers, but its limits are generous), then change it to
`letsencrypt` (or another [lego shortcode](https://go-acme.github.io/lego/) or an ACME directory
URL) and `docker compose up -d`. The `acme` service is [lego](https://go-acme.github.io/lego/) v5.5.2
(pinned by digest) using the HTTP-01 challenge through the proxy's port 80. Until a certificate
exists the proxy answers only on plain HTTP; the moment lego has one, the proxy switches HTTPS on
by itself. lego renews when the certificate is due (checked twice a day); the proxy reloads the new
one without downtime. DNS-01, wildcard certificates and TLS-ALPN-01 are not wired up. The ACME
client container needs the `egress` network, so `acme` and `compose.no-egress.yml` do not mix.

## Exposure: who can reach it

Only `proxy` publishes ports. They are bound to **127.0.0.1** unless you say otherwise, so a fresh
installation answers on the machine itself and nowhere else. To let other computers in, decide it on
purpose in `.env`:

```
DYNODOC_BIND=0.0.0.0        # every interface; or one address, such as 192.0.2.10
DYNODOC_HTTP_PORT=80
DYNODOC_HTTPS_PORT=443
```

Docker publishes ports through its own firewall rules, which **bypass `ufw` and most `firewalld`
setups**. Choose the address in `DYNODOC_BIND`; do not rely on a host firewall rule to hide a port
you published on `0.0.0.0`. The API (8080) and PostgreSQL (5432) are never published and cannot be
reached from outside Docker; the proxy has no route to either.

`DYNODOC_ALT_ORIGINS` (comma-separated `https://` origins) lets the same installation answer to more
names. Any other `Host` header gets `421 Misdirected Request`.

## Files, volumes and permissions

| What | Where | Notes |
|---|---|---|
| Settings | `.env` | No secret values. Mode 0600 is fine. |
| Secrets | `secrets/*` | The only copy that matters. Mode 0600, owned by you. **Back up separately from the server.** Never in git, tickets or chat. |
| Your certificate | `tls/` | `provided` mode only. Keep the key private to you. |
| Notice, mail CA | `config/` | Non-secret files `.env` points at (`/etc/dynodoc/...`). Make them readable (0644). |
| Database | volume `<project>_database` | The data. `docker compose down -v` deletes it. |
| Certificates made here | volume `<project>_tls-state` | Evaluation certificate; ACME account and certificates. |
| Staged secrets | volumes `<project>_secrets-*`, `_tls-provided` | Copies for the services; rebuilt on every `up`; safe to delete. |
| Backups you take | `backups/` (yours to create) | Where the commands below write. Put it on another disk. |

`<project>` is `dynodoc-selfhosted` unless you pass `-p`.

*Why the secrets are copied.* Compose bind-mounts a file secret with the host file's owner and mode
and ignores `uid`, `gid` and `mode` (it prints that it does). A secret that is private to you
(0600) would then be unreadable inside the container, and making it readable would make the API
refuse it. `secrets-init` removes the problem the same way on Linux, macOS and Windows hosts, and a
stale copy of a removed secret is removed on the next `up`. After you change a secret file, run
`docker compose up -d --force-recreate` so the services read the new value.

*Docker Desktop.* Bind-mounted files appear as root-owned 0777 inside containers; the staging step
makes that irrelevant. Ports below 1024 may be taken by other software (IIS, WSL): use
`DYNODOC_HTTP_PORT`/`DYNODOC_HTTPS_PORT`.

## Day to day

```sh
docker compose ps                         # state and health of every service
docker compose logs -f proxy web api      # logs (rotated: 5 files of 10 MB per container)
docker compose restart web                # restart one service
docker compose up -d                      # apply a changed .env; re-runs the jobs
docker compose stop                       # stop, keep everything
docker compose down                       # remove containers and networks, KEEP the data
```

**Never `docker compose down -v`** on an installation you want to keep: `-v` deletes the database
volume.

Logs hold the client address and the request path but never query strings, sign-in tokens or private
links (`/view/<token>` is written as `/view/[private-link]`), and never a secret value.

## Backup and restore by hand

Until `dynodoctl` exists, a backup is a database dump plus the secrets directory. A dump alone cannot
start an installation: the secrets (`PASETO_LOCAL_KEY` seals sessions, `SESSION_SECRET` signs cookies)
and `.env` are needed too. Keep both, in different places.

```sh
mkdir -p backups
docker compose exec -T postgres sh -c \
  'PGPASSWORD=$(cat /run/secrets/postgres_password) pg_dump -h 127.0.0.1 -U dynodoc -Fc dynodoc' \
  > backups/dynodoc-$(date +%F).dump
```

(In Windows PowerShell `>` rewrites the bytes; run the same command from `cmd /c "..."` or from Git
Bash.) Test a backup before you need it: restore into a scratch database, not over production.

```sh
docker compose exec -T postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) createdb -h 127.0.0.1 -U dynodoc restore_check'
docker compose exec -T postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) pg_restore -h 127.0.0.1 -U dynodoc -d restore_check --no-owner --exit-on-error' < backups/dynodoc-2026-10-22.dump
docker compose exec -T postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) dropdb -h 127.0.0.1 -U dynodoc restore_check'
```

To **restore over the installation** (disaster recovery, or rolling back an upgrade): stop the
writers, recreate the database, load the dump, start again. `up` re-applies the runtime login and
its grants, which a restored database does not carry.

```sh
docker compose stop proxy web api
docker compose exec -T postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) psql -h 127.0.0.1 -U dynodoc -d postgres -c "DROP DATABASE dynodoc" -c "CREATE DATABASE dynodoc"'
docker compose exec -T postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) pg_restore -h 127.0.0.1 -U dynodoc -d dynodoc --no-owner --exit-on-error' < backups/dynodoc-2026-10-22.dump
docker compose up -d --wait
docker compose run --rm database-admin verify-snapshots --all      # the history verifies
```

Do not restore a backup onto a machine that is meant to run next to the original: a restored copy
has the same installation identity.

The `database-admin` service (profile `maintenance`, not started by `up`) runs `engine-cli` with the
owner login: `docker compose --profile maintenance run --rm database-admin verify <document-id>`,
`... check-runtime`, `... verify-snapshots --all`. (`run --rm database-admin ...` alone also works.)

## Upgrading

An upgrade is new images plus `up`, because `init` runs on every start and applies migrations:

```sh
docker compose pull                     # a release: new images (or rebuild: compose.build.yml)
docker compose up -d --wait             # secrets-init, preflight, db-roles, init, then the services
```

Before you upgrade, **take a backup** (above). Migrations only move forward and an older image
refuses a newer database ("database schema differs from this binary"), so the way back is to restore
the backup into the old images. Read the release notes first: some releases need a rehearsal on a copy
of your data. `dynodoctl upgrade` will automate this; today the steps above are the procedure.
Base images are pinned by digest; `node scripts/pin-bases.mjs` shows which pins are out of date.

## Images

| Image | Built from | Size on disk | Cold build (12 cores, shared) | Platforms of the base |
|---|---|---|---|---|
| `api` | `images/Dockerfile.api`: Rust 1.96 build, `debian:bookworm-slim` runtime; `engine-api` and `engine-cli` | 182 MB | 23 min (cached: seconds) | amd64, arm64 |
| `web` | `images/Dockerfile.web` target `runtime`: `node:24-bookworm-slim` | 536 MB | 9 min | amd64, arm64 |
| `tools` | the same file, target `tools`: `web` plus the settings generator and checkers (about 100 KB more; shares every other layer) | 536 MB (shared) | 10 s | amd64, arm64 |
| `proxy` | `images/Dockerfile.proxy`: `nginxinc/nginx-unprivileged:stable-alpine3.24` plus openssl | 92 MB | seconds | amd64, arm64 |
| `acme` | `images/Dockerfile.acme`: `alpine:3.24` plus lego v5.5.2 | 103 MB | seconds | amd64, arm64 (lego also arm/v6, arm/v7) |
| `postgres` | `postgres:16` as published | 642 MB | pulled | amd64, arm64 |

Every base image is pinned by digest (the digest of the multi-platform index, so one pin serves
amd64 and arm64). `node scripts/pin-bases.mjs` shows which pins are out of date and `--update`
rewrites them; rebuild and run the tests afterwards. The API image is built for amd64 and arm64 by
the engine repository's CI; the web, proxy and acme images are plain multi-platform builds
(`docker buildx build --platform linux/amd64,linux/arm64`). Only amd64 was built and run here.

For a release, push the images and generate the overlay that names them by digest:

```sh
node scripts/make-release-overlay.mjs --registry registry.example.org/dynodoc --version 1.0.0
docker compose -f compose.yml -f compose.release.generated.yml up -d --wait
```

`node scripts/sbom.mjs` writes an SPDX bill of materials per image with the Docker Scout CLI. It
lists operating-system packages and the Node dependencies of the web image; the API image's Rust
crates are not in it (the binary carries no dependency table; building it with `cargo auditable`
is part of P0-6). Signing is a later task (P3-1).

## Optional parts

- **Legacy `.doc` import.** Generate the settings with `--with-converter`, then
  `COMPOSE_PROFILES=converter` in `.env` (or `docker compose --profile converter up -d`). The
  converter has no network except to `web`. Without it, `.doc` upload answers with a request to save
  as DOCX.
- **No outbound connections from the web server** (nothing needs a mail server or an identity
  provider): `docker compose -f compose.yml -f compose.no-egress.yml up -d --wait`. Email sign-in
  then needs a relay the web container can reach by name, so for an isolated installation set
  `AUTH_EMAIL_LINK=off` until another sign-in method exists.
- **Receipt authority key** (`DYNODOC_REF_SIGNING_KEY_FILE`, signed shared-branch writes) is not wired
  into this bundle yet.

## Troubleshooting

| You see | Likely cause | What to do |
|---|---|---|
| `Set DYNODOC_TLS_MODE in .env` | The choice is required | Add `DYNODOC_TLS_MODE=evaluation`, `provided` or `acme` |
| `secrets-init` exits 1: "secrets/x is missing" | `generate-env` was not run here, or a file was removed | Run step 2; never copy secrets between installations |
| `secrets-init`: "database_url does not match runtime_database_password" | A secret was edited by hand | Restore the generated pair; the bundle fixes the login, host and database names |
| `secrets-init`: "tls/fullchain.pem is missing or empty" | `provided` mode without a certificate | Put the files in `tls/` or change the mode |
| `preflight` exits 1: "invalid configuration (N problems)" | A setting in `.env` is wrong | `docker compose logs preflight`; fix each line (the same message the web server prints) |
| `db-roles` exits 1: "password authentication failed" | `secrets/postgres_password` is not the password the database was created with (a secrets directory from another install, or restored from the wrong backup) | Put the right file back. The owner password is set only when the database is first created |
| `init` fails: "database schema differs from this binary" | The images are older than the database | Use the matching (newer) images, or restore a backup into these |
| `proxy` keeps restarting, log says "TLS mode 'provided': the certificate cannot be used" | The certificate files are wrong | Read the lines above that message; fix `tls/`; run the two commands under "provided" |
| Browser: 421 "Misdirected request" | You used a name that is not `ORIGIN` or `DYNODOC_ALT_ORIGINS` (an IP address, another DNS name) | Use the exact `ORIGIN`, or add the name to `DYNODOC_ALT_ORIGINS` |
| Every form submit fails with "Invalid origin" | `ORIGIN` differs from the address in the browser (scheme, name, port) | Make `ORIGIN` exactly what the browser shows, then `docker compose up -d` |
| Sign-in page: "Email sign-in is awaiting administrator setup" | No mail transport is configured | Set the `SMTP_*` lines (deployment guide 7.2), check with `check-mail.mjs` |
| "We could not send your sign-in link" | The mail server refused or did not answer | `docker compose logs web` shows `mail ... class=...`; run `check-mail.mjs` for the cause |
| Nothing answers from another computer | The default is `127.0.0.1` | Set `DYNODOC_BIND` (see Exposure) |
| "port is already allocated" / "ports are not available" | Something else uses 80 or 443 | Set `DYNODOC_HTTP_PORT` / `DYNODOC_HTTPS_PORT` and put the port in `ORIGIN` |
| 502 right after `up` | The web server is still starting | Wait; `docker compose ps` until `web` is healthy |
| `acme`: no certificate, log shows a challenge error | The CA cannot reach port 80 on the name, or `DYNODOC_BIND` is `127.0.0.1` | `docker compose logs acme proxy`; open port 80; set `DYNODOC_BIND` |
| Browser warns about the certificate | `evaluation` mode, as designed | Use `provided` or `acme` for real use |
| After `docker compose down -v` everything is empty | `-v` deleted the volumes | Restore from the backup and the secrets directory |

## What is and is not in this build

*Hardening in place, each checked by `scripts/smoke.mjs`:* only the proxy publishes ports, on
127.0.0.1 by default; internal networks without a route out; every service non-root, with a
read-only root file system, all capabilities dropped, `no-new-privileges`, memory and process
limits and log rotation; one exception, `secrets-init`, runs as root for a few seconds with three
capabilities, no network and a read-only root; the API and web hold no database owner credential;
PostgreSQL asks for a SCRAM password on every connection; the runtime login cannot run DDL, touch
the migration table or change the event history; no secret value appears in `docker inspect`,
`docker compose config`, the logs or `.env`.

*Not in it:* TLS between the proxy, web, API and PostgreSQL (they share private networks); log
shipping and metrics; automated backups, restore verification and upgrades with rehearsal
(`dynodoctl`); high availability; operator-issued sign-in links and identity-provider sign-in; the
air-gap acceptance run; image signing. The database owner is the PostgreSQL image's bootstrap
superuser; it is held only by the jobs and by whoever runs `docker compose exec postgres`. Anyone who
can run Docker on this machine controls everything in it; treat the host accordingly.

## Tests that ship with the bundle

| Command | What it proves |
|---|---|
| `node scripts/smoke.mjs` (or `scripts/smoke.sh`) | The whole stack in `evaluation` mode: ports, redirect, headers, hardening, roles, sign-in by email through a throwaway mail sink, Chromium, restarts, down/up, backup and restore, failures that must stop the stack |
| `node test/proxy-tls.mjs` | `provided` and `evaluation` certificates: a bad key refused, renewal and replacement with no failed request; server-sent events unbuffered; WebSocket upgrade passed |
| `node test/acme-pebble.mjs` | `acme` mode against Pebble, the Let's Encrypt test server: HTTP-01, hand-over, renewal with no failed request |
| `docker run ... sh /t/tls-provided-cases.sh` | Sixteen good and bad certificate chains and the messages they give |
| `node scripts/pin-bases.mjs` | Which base-image pins are out of date |
| `node scripts/make-release-overlay.mjs` | The release overlay (images by digest) from a registry |
| `node scripts/sbom.mjs` | An SPDX bill of materials per image (Docker Scout CLI) |

## Files in this directory

```
compose.yml               the stack
compose.build.yml         build the images from source
compose.no-egress.yml     optional: web without a route out
bundle.env.example        the bundle's own settings
images/                   Dockerfiles: api, web (+ tools), proxy, acme
proxy/                    nginx configuration, templates, certificate and reload scripts
acme/                     the ACME client loop
jobs/                     scripts the one-shot jobs run
scripts/                  smoke test, pin-bases, release overlay, SBOM
test/                     tests and test-only helpers (the mail sink is not part of the product)
tls/  config/             your certificate; your non-secret files
```
