# Self-hosted edition: operating the single-node bundle with `dynodoctl`

Status, 22 October 2026: `dynodoctl` v1 (task P0-5 of the [self-hosted edition plan](SELF-HOSTED-EDITION.md)). It is
source and tests in the application repository (`crates/dynodoctl`). It is not released, not signed and not packaged
with the bundle; today it is built from source (`cargo build --release -p dynodoctl`). It operates the bundle in
`infra/selfhosted` (Profile A: one machine, Docker Compose). Profiles B and C, the add-in manifest builder
(`addin build`, task P0-7), air-gapped import and signed bundles are not part of it.

This page is the command reference, the exit codes, the decisions behind the tool and the runbooks for install, backup,
restore, upgrade and rollback. The bundle itself is described in [`infra/selfhosted/README.md`](../infra/selfhosted/README.md);
the wider design is in the [deployment guide](SELF-HOSTED-DEPLOYMENT-GUIDE.md), chapter 9.

## 1. What it is, and what it needs

One program, no runtime besides Docker. It runs on the machine that runs the bundle (or any machine that can run
`docker` against it) and drives `docker compose` and the containers of the bundle. It needs Docker Engine or Docker
Desktop with Compose v2.24 or newer. It does not need Node, Python, `psql`, `pg_dump`, `openssl` or `curl` on the host:
database work happens inside the PostgreSQL container, and the HTTPS checks are made by the program itself.

Run it in the bundle directory (the one with `compose.yml`), or pass `--dir`:

```sh
cd /srv/dynodoc            # the unpacked release, or infra/selfhosted of a checkout
dynodoctl status
dynodoctl --dir /srv/dynodoc status
```

Files it keeps in the bundle directory, besides the bundle's own `.env`, `secrets/`, `tls/` and `config/`:

| Path | What |
|---|---|
| `backups/dynodoc-<UTC>[-label]/` | One backup each: `database.dump`, `manifest.json`, `SHA256SUMS`, and `config/` (the settings and secrets copy) unless it was sent elsewhere |
| `state/env-<UTC>.bak` | The previous `.env`, kept before every `configure set` (mode 0600) |
| `state/pre-upgrade-<UTC>/` | `.env` and the release overlay as they were before an upgrade |
| `state/upgrade-<UTC>.json`, `rehearsal-<UTC>.json`, `restore-*.json` | Records of what ran, with timings and results |
| `support/dynodoc-support-<UTC>.tar.gz` | Support bundles |
| `compose.release.generated.yml` | The release overlay (images by digest), when the installation came from a release |

## 2. Exit codes

| Code | Meaning | Examples |
|---|---|---|
| 0 | Done, and every check passed | `status` with nothing to warn about; a backup that verified |
| 1 | The command ran and a check or step failed | an unhealthy service; `verify-snapshots` failed on a restored copy; a rehearsal that failed |
| 2 | Wrong arguments | an unknown option; `restore` without `--into-live`; a value a setting does not accept |
| 3 | Refused before changing anything | no confirmation typed; a damaged backup; preflight failed; a conflicting setting; the stack is not installed |
| 4 | Finished with warnings only | `status` on an evaluation installation; `verify-delivery` with an evaluation certificate |

Errors are printed on standard error as `ERROR: <what happened>` and `  What to do: <next step>`. With `--json`
standard output carries exactly one JSON document (for an error: `{"ok": false, "error": {"what", "todo", "exit_code"}}`)
and progress goes to standard error.

## 3. Global options

`--dir DIR` (default `$DYNODOCTL_DIR`, else the current directory) | `--project NAME` (Compose project; the bundle's own
name `dynodoc-selfhosted` applies when omitted) | `-f FILE` (extra compose file, repeatable) | `--json` |
`--confirm PHRASE` (answer a typed confirmation in advance, repeatable, the exact phrase) | `--yes` (answers the plain
`yes` question of `install`) | `--ca-file FILE` (extra trusted certificate authorities for the checks that look at the
certificate, repeatable).

Typed confirmations: `REPLACE LIVE DATA` (restore over the live database), `I HAVE NO BACKUP` and `SKIP REHEARSAL`
(upgrade), `ROLLBACK` (rollback, and the offer after a failed upgrade). The phrase must match exactly; a different case
or a stray word does not count. Without a terminal and without `--confirm`, a command that needs one refuses (exit 3).

## 4. Command reference

### `install`

Preflight, settings and secrets, certificate mode, images, `docker compose up -d --wait`, next steps. Idempotent.

```sh
dynodoctl install --from ./release --origin https://docs.example.org --tls provided \
    --cert fullchain.pem --key privkey.pem --admin-emails you@example.org \
    --smtp-host smtp.example.org --smtp-from docs@example.org
dynodoctl install --origin https://localhost:8443 --tls evaluation --http-port 8080      # a trial
dynodoctl install --dry-run ...                                                           # preflight and plan only
```

- **Preflight** (read-only): Docker daemon, Compose 2.24 or newer, memory (the daemon's: on Docker Desktop that is the
  VM), clock against the daemon's, free disk, a writable directory, the HTTP and HTTPS ports on the bind address, and
  whether the origin's name resolves (a failure for `acme`, a warning otherwise). A failed check refuses (exit 3) and
  names the fix.
- **The certificate mode has no default.** `--tls evaluation|provided|acme` is required. `provided` needs `--cert` and
  `--key`. `acme` needs `--acme-email`, `--acme-ca` (no default authority) and an explicit `--acme-accept-tos`.
- **Settings and secrets are made by the bundle's own generator**, `frontend/researcher/scripts/dynodoc-env.mjs`, which
  dynodoctl calls and does not re-implement (section 5). `--generator auto` (default) uses the `tools` image if it is on the
  machine, else Node 20 or newer with the script of a source checkout.
- **Never overwrites.** An existing `.env` and `secrets/` are kept; a half-installed directory is refused; a setting that
  differs from the one already in `.env` is refused (`configure set` changes it). Running it again on an installation
  re-applies `up -d --wait` and nothing else.
- **Images:** from a registry (`--registry`, `--version`, optionally `--overlay` with the release's
  `compose.release.generated.yml`) it pulls; from a source checkout `--build` builds them (about 25 minutes cold);
  otherwise the images must already be on the machine, or the command refuses and prints the build command.
- **Mail password:** `--smtp-user` makes the generator create an empty `secrets/smtp_password`; give the password with
  `--smtp-password-file FILE` or `--smtp-password-stdin` (never an argument), or fill the file and run again. The
  install stops before starting if it is empty.
- **The first administrator**, as it works today: people in `ADMIN_EMAILS` may use `/admin` once they have signed in by an
  emailed link. Email sign-in needs a working mail transport; without one nobody can sign in, the first administrator
  included. There is no operator-issued sign-in link yet (task P0-3), and the first request for a link creates the
  account, so limit who can reach the installation (`DYNODOC_BIND`, a firewall, a VPN). `install` prints exactly this.

### `configure`

```sh
dynodoctl configure show
dynodoctl configure set smtp-host=smtp.example.org smtp-from=docs@example.org --mail-to you@example.org --apply
dynodoctl configure check [--mail-to you@example.org] [--send]
dynodoctl configure apply
```

Settings it manages: `origin`, `tls-mode`, `bind`, `http-port`, `https-port`, `smtp-host`, `smtp-port`, `smtp-security`,
`smtp-from`, `smtp-user`, `brand`, `org-name`, `telemetry`, `auth-email-link`, `admin-emails`, `dev-auth`, `acme-email`,
`acme-ca`, `acme-accept-tos`. Secrets are never settings: they are not listed, shown or accepted.

`set` validates each value, keeps `state/env-<UTC>.bak` (mode 0600), writes `.env`, and runs the web tier's own
configuration check (the `preflight` job, the same validator the web server runs). If the check rejects the change, the
previous `.env` is put back (exit 3). With `--mail-to` it also does the real mail handshake (`check-mail`, from the web
server's network position, without sending); a failure takes the change back. `--apply` runs `docker compose up -d --wait`
(the jobs run again and services whose configuration changed are recreated); if the stack does not become healthy the
previous settings are put back and started. Related settings follow each other: `smtp-host` turns the SMTP transport on and
needs `smtp-from`; an `origin` with a port also sets the published HTTPS port; `tls-mode=acme` needs the authority, the
contact and the accepted terms.

### `status`

```sh
dynodoctl status [--json] [--max-backup-age-days 2] [--backup-dir DIR] [--no-volumes]
```

Services and health, image versions, the certificate the proxy serves (mode, subject, expiry, whether the chain is
trusted), the schema level (migrations applied and the highest), event and document counts, the database size, volume
sizes, the newest backup and its age, and warnings: evaluation TLS in use; `DEV_AUTH=true`; a bind address other than
loopback; no backup, or one older than N days; backups on the same disk as the installation; a certificate expiring in
under 30 days; leftover scratch databases from an interrupted check. Problems (a service down, a failed job, a certificate
expiring in under 7 days or expired) give exit 1; warnings only give exit 4. The JSON has `schema_version: 1` and the
top-level keys `generated_utc, directory, project, overall, services, versions, tls, database, volumes, backup,
warnings, problems`.

### `backup`

```sh
dynodoctl backup [--to DIR] [--secrets-to DIR] [--label NAME] [--keep 7] [--verify] [--no-config]
```

A consistent `pg_dump -Fc` taken inside the PostgreSQL container (one snapshot; the password is read inside the
container and never appears on a command line), checked with `pg_restore --list`, with a `manifest.json` (versions,
images, schema level, counts before and after the dump, checksums), `SHA256SUMS`, and a copy of `.env`, `secrets/`, `tls/`,
`config/` (mode 0600). A failed step removes the half-written directory. Existing backups are never overwritten.
`--keep N` removes the oldest routine backups after the new one succeeded (only directories with a valid manifest; the
backup just made is never removed; pre-upgrade backups have their own count). `--verify` restores the new backup into a
throwaway database and runs both history checks; if that fails nothing is removed. `--secrets-to DIR` sends the copy of the
settings and secrets to another directory, so the dump and the secrets need not travel together.

**The backup is not encrypted.** dynodoctl does not invent cryptography, and the repository has no passphrase-sealed
format for arbitrary files (the key-file format seals one signing key). The dump holds every document; the secrets copy
lets anyone run the installation. Both are written with mode 0600, and the report says so loudly. Keep them on another
machine, apart from each other, on storage you control. Encryption (age or the at-rest envelope) and remote targets are
future work.

### `restore-verify`

```sh
dynodoctl restore-verify [--backup latest|NAME|DIR] [--config-from DIR]
```

Loads a backup into a **throwaway database** in the running PostgreSQL (`dynodoc_restore_<time>`, never the live one),
checks the counts against the manifest, runs `engine-cli verify-snapshots --all` and `engine-cli verify` for each
document, prints a restore report (counts, hash chain verified, schema level, timings) and removes the database even when a
step failed. Needs the stack running.

### `restore --into-live`

```sh
dynodoctl restore --into-live [--backup NAME|DIR] [--config-from DIR] [--no-safety-backup] [--keep-previous]
```

Replaces the live database with a backup. It asks for the typed phrase `REPLACE LIVE DATA` (or `--confirm`); without it
nothing happens (exit 3). Order: check the backup's checksums; put back `.env` and `secrets/` from the backup if the
directory has none; start PostgreSQL; run the `db-roles` job (the runtime login must exist before a dump with its grants
is loaded); save the current database as a `pre-restore` backup (unless empty or `--no-safety-backup`); stop proxy, web and
api; load the dump into a **new** database beside the live one (a damaged dump fails here and the live database is
untouched); swap the names; run `up -d --wait` (db-roles and init re-apply the grants); verify counts, history and
`check-runtime`; drop the replaced database only if everything verified (or keep it with `--keep-previous`).

Install files that differ from the backup's copy are kept and reported. `restore` without `--into-live` is a usage error
that points at `restore-verify`.

### `rehearse-lite`

```sh
dynodoctl rehearse-lite [--backup latest|NAME|DIR] [--version V --registry R --overlay FILE]
```

Restores the latest backup into a scratch database, mirrors the database ACL on it (the db-roles step: PUBLIC revoked,
CONNECT granted to the runtime login), runs the **target release's** `engine-cli migrate`, `grant-runtime` and
`check-runtime`, verifies the history, starts the target API image against the copy and waits for `/healthz`, then removes
the scratch database. The target images come from process variables for these commands only; `.env` is not changed. "Lite"
means one host, the live PostgreSQL server (a scratch database, never the live one), no load. The report is saved in
`state/`.

### `upgrade` and `rollback`

```sh
dynodoctl upgrade --version 1.1.0 --registry registry.example.org/dynodoc --overlay compose.release.generated.yml
dynodoctl upgrade --plan ...            # what would happen, nothing changed
dynodoctl upgrade --local               # the mechanism with the installed images (development, configuration-only)
dynodoctl rollback
```

Order: plan; pull the new images and **check each against the digest in the overlay** (a tag without an overlay is refused
unless `--allow-unpinned`); a **mandatory pre-upgrade backup**; the rehearsal on a copy of that backup; save `.env` and the
overlay in `state/pre-upgrade-<UTC>/`; write the new version; `docker compose up -d --wait` (the `init` job migrates);
check that every service is healthy; print how to roll back. A failed pull, digest check or rehearsal stops before anything
is switched (exit 1 or 3) and leaves the installation as it was.

`--skip-backup` needs the phrase `I HAVE NO BACKUP`; `--skip-rehearsal` needs `SKIP REHEARSAL`; both are recorded in
`state/upgrade-<UTC>.json`. If the new version does not become healthy the tool offers the rollback (phrase `ROLLBACK`, or
`--auto-rollback`).

`rollback` restores the pre-upgrade backup into the previous version: it puts the saved `.env` and overlay back and runs
the same safe restore as `restore --into-live`. Data written after the pre-upgrade backup is lost. Rolling back is a restore
because migrations only move forward and an older image refuses a newer database.

### `support-bundle`

```sh
dynodoctl support-bundle [--out DIR] [--log-lines 400] [--keep-emails] [--list]
```

One `.tar.gz` with versions, `docker compose ps` and `config`, recent logs, `.env` with secret-looking values hidden,
disk and memory, certificate summaries, the migration level, backup names and ages, and `status --json`, plus a
`manifest.json` with a checksum for every entry. It prints exactly what it contains. **It never contains** a file from
`secrets/` (one guard refuses every such read), a private key, a backup, or document content. This installation's own
secret values are loaded into memory only to be hidden wherever they appear; on top of that, URL passwords, bearer and
PASETO tokens, `password=`-style pairs, private-key blocks and long hex keys are replaced. Email addresses are replaced
unless `--keep-emails`. `--list` shows the inventory and writes nothing. Read the archive before sending it.

### `verify-delivery`

```sh
dynodoctl verify-delivery [--origin URL] [--connect HOST[:PORT]] [--http-port N] [--tls-mode M] [--mail-to ADDR] [--ca-file FILE]
```

Looks at the installation from the operator's machine, as a browser would: HTTPS reachable at the origin; TLS 1.2 or
newer; the chain valid for the name (an evaluation certificate is a warning, any other untrusted chain a failure); the
expiry horizon (warning under 30 days, failure under 7); plain HTTP redirects to the origin; the sign-in page; exactly one
content-security-policy naming the origin, HSTS, `nosniff`, `no-referrer`, `SAMEORIGIN`, no version banner, `noindex`;
`/healthz`; another host name gets 421; the API refuses anonymous requests; the marketing pages answer 404; the front page
sends visitors to sign-in; what is served names no outside host; and, when run in the install directory with SMTP
configured, the mail handshake (nothing is sent).

The outside-host check reads the sign-in page and follows its scripts, inline `import()` calls and static imports (up to
60 files, eager files first). A hosted-service name, analytics host or CDN in a file the sign-in page loads, or any tag
loading from another host, **fails**. The same names only in lazily loaded route modules **warn** and name the open item:
the web build still contains the compiled marketing routes, so `dynodoc.online` appears in six route modules (acceptance
criterion 6 of Phase 0, task P0-6). Documented optional references (the Office.js CDN, the optional Resend transport, the
engine repository link) are warnings.

The check uses the Mozilla roots built into the program; the operating system's trust store is not consulted. For a
company certificate authority pass `--ca-file`. The add-in manifest check is a named stub that reports `NOT BUILT` (task
P0-7) and does not change the result. Exit 1 if anything failed, 4 if only warnings, else 0.

## 5. Decisions

| Question | Decision | Why |
|---|---|---|
| Language and packaging | Rust, crate `crates/dynodoctl` in the root workspace, one binary | Deployment guide DQ1 recommends a static binary so Profile A needs no Python or Node on the host; the engine is Rust and the platform matrix is the same; `docker compose` is driven through a small runner with a scripted fake, so every command is tested without Docker. No async runtime; every dependency was already in `Cargo.lock` (`clap`, `serde`, `serde_json`, `chrono`, `ring`, `hex`, `base64`, `rustls`, `webpki-roots`, `flate2`, `regex`, `libc`, `windows-sys`, `tempfile` for tests). |
| Licence | The crate inherits the workspace's `license-file` like the engine crates, but **it is not part of the public MIT engine mirror** | The licensing document (4.1) lists `dynodoctl` as proprietary. Leave `crates/dynodoctl`, its `members` line and its `Cargo.lock` entry out when engine sources are mirrored to `ArefinAlter/dynodoc-engine`. |
| Settings and secrets | Call the bundle's generator (`dynodoc-env.mjs`) through the `tools` image or Node; do not re-implement it | One source of truth for file formats, secret shapes and the validator. The generator is run into a fresh work directory inside the bundle directory and `.env` and `secrets/` are then moved up, because the generator `chmod`s its target and a bind-mount root belongs to root on Docker Desktop (found by running it). |
| Backup method | Logical `pg_dump -Fc` in the container | One consistent snapshot, restorable into a newer 16.x, checkable with `pg_restore --list`, no replication privilege. A physical base backup (`pg_basebackup`, pgBackRest) is Profile B work (P2-9). |
| Backup encryption | None; mode 0600 and a loud warning; `--secrets-to` separates the secrets | See `backup`. |
| Restore | Load beside, then swap names; keep the ACLs (`--no-owner` only); history is verified with the owner login | A failed load leaves the live database untouched. `--no-acl` drops the `REVOKE EXECUTE ... FROM PUBLIC` statements the migrations rely on, and `grant-runtime` then refuses the copy; a copy restored without grants gives the runtime login nothing to verify with (both found on the real bundle). |
| Verification | `verify-snapshots --all` plus `verify` for each document, on the restored copy | The hosted release routine and acceptance criterion 3. |
| Confirmations | Typed phrases, accepted from `--confirm` or a terminal only | A script cannot confirm by accident; `--yes` covers only the install question. |
| TLS client | rustls with a verifier that records the result and continues | An evaluation certificate can still be inspected; trust is reported, not assumed. |

## 6. Runbooks

Each ends with a record: who, when, version, checks run, results. The tool writes most of it to `state/`.

### 6.1 Install (Profile A)

1. Get the bundle (an unpacked release, or `infra/selfhosted` of a checkout) and the images (a registry, or `--build`).
2. `dynodoctl install --dry-run ...` and read the preflight and the plan. Fix what fails.
3. `dynodoctl install ...`. Expect about a minute and a half for a first start with images present.
4. `dynodoctl status`, then `dynodoctl verify-delivery` (add `--ca-file` for a company authority).
5. `dynodoctl backup --verify --secrets-to <another disk>` and copy both to another machine.

### 6.2 Daily and weekly

`dynodoctl backup --keep 14` from cron or a systemd timer (it needs the same rights as `docker`); `dynodoctl status
--json` into your monitoring (exit 1 problems, 4 warnings); `dynodoctl restore-verify` weekly; `dynodoctl verify-delivery`
after every change to the proxy, the certificate or the origin.

### 6.3 Restore drill (do this before you need it)

`dynodoctl restore-verify` on the newest backup. The report is the evidence for your recovery objective: it lists the
duration of each step, the counts compared with the manifest and that the hash chain verified.

### 6.4 Disaster recovery (acceptance criterion 3)

The installation is gone: the PostgreSQL volume deleted, or a rebuilt host. You have the backup directory and the settings
and secrets copy.

1. Put the bundle directory back (unpack the release; images from the registry or built).
2. `dynodoctl --project <name> restore --into-live --backup <backup directory> --config-from <settings copy> --confirm "REPLACE LIVE DATA"`.
3. Read the restore report: counts match, history verified, `check-runtime` passed. Expect writes made after the backup to
   be lost.
4. `dynodoctl status`, `dynodoctl verify-delivery`. Take a new backup.

Do not restore onto a machine that is meant to run beside the original: a restored copy has the same installation identity.

### 6.5 Upgrade from release N to N+1 (acceptance criterion 4)

1. Read the release notes (migrations, expected time). `dynodoctl upgrade --plan --version N+1 --registry R --overlay ...`.
2. `dynodoctl upgrade --version N+1 --registry R --overlay compose.release.generated.yml --secrets-to <another disk>`.
3. `dynodoctl status` and `dynodoctl verify-delivery`. Keep `state/upgrade-<UTC>.json`.
4. To go back: `dynodoctl rollback`. Write down that data written after the pre-upgrade backup is lost.

### 6.6 If an upgrade fails

A failed pull, digest check or rehearsal changes nothing: fix the cause and run again. A new version that does not become
healthy: answer the rollback offer with `ROLLBACK`. If the rollback itself fails, the pre-upgrade backup is still in
`backups/` and `dynodoctl restore --into-live --backup <it>` can be repeated; the database that was replaced is kept until a
restore verifies.

### 6.7 Support

`dynodoctl support-bundle --list`, then without `--list`; read the archive; send it. For a suspected leak use
`dynodoctl status --json` and the guide's chapter 9.13.

## 7. Troubleshooting

| You see | Cause | What to do |
|---|---|---|
| `HTTP port N on ... is in use` | Another program holds it (IIS, WSL, another stack) | `--http-port` / `--https-port`, and the port in the origin |
| `the settings generator failed: EPERM ... chmod '/bundle'` | An older dynodoctl on Docker Desktop | Use this version: it generates into a work directory |
| `secrets/smtp_password is empty` | A mail login was given without its password | `--smtp-password-file`, or fill the file and run again |
| `the backup is damaged or incomplete` | A checksum or file is wrong | Use an earlier backup; nothing was changed |
| `PostgreSQL is not running` | The stack is down | `docker compose up -d --wait`; for a lost volume use `restore --into-live` |
| `grant-runtime failed on the copy` | The copy was restored without its ACLs (older dynodoctl) or the target release changed the runtime allow-list | Rehearse with this version; if it still fails, do not upgrade and report the message |
| `the rehearsal needs a backup to copy` | `--skip-backup` with no backup on disk | Take a backup, or also skip the rehearsal (confirmed) |
| `verify-delivery`: `What is served names no outside host` | A file the sign-in page loads names an outside host | The message lists the file and the host |
| `verify-delivery`: route modules name the hosted site | Known open item (criterion 6) | A warning; closes with task P0-6 |
| Scratch databases listed by `status` | A check was interrupted | `docker compose exec postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) dropdb -h 127.0.0.1 -U dynodoc <name>'` |

## 8. What was tested, and what was not

See the status paragraph in [SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md), section 13.2 (P0-5): unit tests against a
fake runner and a fake network, thirteen mutation checks, and a run of every command against the real Docker bundle on
Windows 11 with Docker Desktop. Not run: a Linux host, arm64, a real certificate authority, a real mail server, a real
upgrade from one release to a newer one, remote backup targets, a signed bundle, an air-gapped host, high availability.
