# Self-hosted edition: operating the single-node bundle with `dynodoctl`

Status, 23 October 2026: `dynodoctl` v1 (task P0-5 of the [self-hosted edition plan](SELF-HOSTED-EDITION.md)), revised
after an independent adversarial review (three high and ten medium findings, all addressed; see section 8). It is
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
| `state/upgrade-<UTC>.json`, `rehearsal-<UTC>.json`, `restore-*.json` | Records of what ran, with timings and results. An upgrade record is written **before** `.env` is changed (`result: "switching"`) and finished afterwards, so an upgrade that was cut short is still found by `status` and `rollback`. `pg_restore`'s own messages are never stored in them: they can quote rows of your data |
| `state/restore-journal.json` | Exists only while a database swap is unfinished (section 4, `restore --into-live`). While it exists every command that could change the databases refuses; `dynodoctl recover` reads it |
| `state/.lock` | The run lock: one command that changes the installation runs at a time |
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
and progress goes to standard error. A bug that makes the program panic is reported the same way (`internal error: ...`,
exit 1), never as a silent exit. An interrupt that arrives twice, or during a command that cannot unwind, ends the
process at once with status 130 (section 3, "Interrupts").

## 3. Global options

`--dir DIR` (default `$DYNODOCTL_DIR`, else the current directory) | `--project NAME` (Compose project; the bundle's own
name `dynodoc-selfhosted` applies when omitted) | `-f FILE` (extra compose file, repeatable) | `--json` |
`--confirm PHRASE` (answer a typed confirmation in advance, repeatable, the exact phrase) | `--yes` (answers the plain
`yes` question of `install`) | `--ca-file FILE` (extra trusted certificate authorities for the checks that look at the
certificate, repeatable).

Typed confirmations: `REPLACE LIVE DATA` (restore over the live database), `I HAVE NO BACKUP` and `SKIP REHEARSAL`
(upgrade), `ROLLBACK` (rollback, and the offer after a failed upgrade). The phrase must match exactly; a different case
or a stray word does not count. Without a terminal and without `--confirm`, a command that needs one refuses (exit 3):
text piped into standard input never confirms anything (tested with a piped input, not only with the scripted fake).

`--allow-insecure-files` accepts a file or directory that holds settings, secrets or backups when its access cannot be
restricted to the current user (section 4, "Files and access"). It is off by default and prints a warning when used.

**The run lock.** Commands that change the installation or its databases (`install` without `--dry-run`, `configure set`
and `apply`, `backup`, `restore-verify`, `restore`, `rehearse-lite`, `upgrade` without `--plan`, `rollback`, `recover`)
take `state/.lock` first. A second one fails at once, naming the process and the time, instead of racing the first. A lock
left by a process that no longer exists is replaced. `status`, `support-bundle`, `verify-delivery`, `configure show` and
`configure check` do not take it.

**Interrupts.** Ctrl-C and SIGTERM (and on Windows Ctrl-Break and a closing console) do not kill `restore`, `upgrade`,
`rollback` or `recover` where they stand. The first one is noted; the sequence stops at the next step boundary and puts
things back (section 4, `restore --into-live`). The commands that touch the databases run in their own process group, so
the terminal's Ctrl-C does not reach them half way either. A second interrupt ends the program at once (status 130):
every destructive change is one transaction behind the journal, and `dynodoctl recover` finishes or undoes it. Other
commands end at the first interrupt, as any program does.

**A project name belongs to one directory.** Compose finds a project by its name alone. `install`, `restore` and `upgrade`
refuse when containers of the same project name exist for another working directory (the label
`com.docker.compose.project.working_dir`), because they would otherwise stop, migrate or overwrite the other
installation. Give each installation its own `COMPOSE_PROJECT_NAME` in `.env` (or `--project`).

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
- **`--overlay` is for a first install only.** On an installation that exists it would change which images run and then
  start them, so the migrations would run with no backup, no rehearsal and no digest check. It is refused (exit 3);
  `dynodoctl upgrade --version V --overlay FILE` does all three.
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
`config/` (mode 0600). A failed step removes the half-written directory, and a directory that a crash left without a
manifest (marked `.dynodoctl-incomplete`) is removed at the next backup. Existing backups are never overwritten.
`--keep N` removes the oldest routine backups after the new one succeeded: **only backups of this installation** (the same
Compose project and origin in the manifest, so a shared `--to` directory never has another installation's backups counted
or deleted), only directories with a valid manifest; the backup just made is never removed; pre-upgrade backups have
their own count. `--verify` restores the new backup into a throwaway database and runs both history checks; if that fails
nothing is removed. `--secrets-to DIR` sends the copy of the settings and secrets to another directory, so the dump and the
secrets need not travel together; if the backup then fails, that copy is removed too.

**What the checksums mean.** `manifest.json` records the size and SHA-256 of the dump and of the settings files, and
`SHA256SUMS` records them again; a restore checks the files against both records and the records against each other. That
detects damage and a careless edit. It does **not** authenticate a backup: whoever can write the backup directory can
rewrite both records together. Authentication needs a signature, which is future work (see "Trust" below).
**Secret files carry no per-file hash.** A plain SHA-256 of a human-chosen mail password or a TLS key would let anyone who
reads the manifest test guesses offline. The manifest holds one salted digest over all secret files together
(`secrets_salt`, `secrets_digest`) to detect a swapped or damaged secrets copy, and nothing else about them.

**Files and access.** Files this tool writes that hold settings, secrets, dumps or reports are created readable by their
owner only: mode 0600 on Linux and macOS. On Windows mode bits do not exist, so the tool applies an access list with the
system `icacls` (inheritance removed, one entry for the current user, the other entries removed) and **reads it back to
check**. If that cannot be done (a file system without access lists, such as a FAT volume or some network shares), the file
is not kept and the command fails with the reason, unless `--allow-insecure-files` was given, in which case a warning says
the folder's own permissions apply. A directory that already existed (a `--to` or `--secrets-to` you chose, such as
`/tmp`) is never re-permissioned; only directories the tool creates are made private. Windows administrators and the
SYSTEM account can still read the file through their own privileges; the access list keeps out other ordinary accounts.

**The backup is not encrypted.** dynodoctl does not invent cryptography, and the repository has no passphrase-sealed
format for arbitrary files (the key-file format seals one signing key). The dump holds every document; the secrets copy
lets anyone run the installation. Both are written with mode 0600, and the report says so loudly. Keep them on another
machine, apart from each other, on storage you control. Encryption (age or the at-rest envelope) and remote targets are
future work.

### Trust: restoring a backup runs its SQL

**A backup is code.** `pg_restore` loads a dump by running the SQL inside it, as the database owner, which is a PostgreSQL
superuser, inside the database container. A dump that someone changed could contain `COPY ... FROM PROGRAM` (a command run
inside the container), `ALTER SYSTEM`, a function in an untrusted language, an event trigger, or a new superuser. The
manifest is a plain file too: a hostile backup can also name files to write when the settings are put back. So:

- **Restore only backups you made, or can account for.** Treat a backup file like a program someone handed you. Keep your
  backups where only you can write.
- What dynodoctl does about it is defence in depth, not a sandbox, and not authentication:
  1. Every path a manifest names for the settings copy is confined: only `.env`, the release overlay, and one plain file
     directly under `secrets/`, `tls/` or `config/` are accepted (no `..`, no absolute path, no drive or network prefix, no
     hidden file). The dump's own name must be a plain file name in the backup directory. Anything else refuses the whole
     backup before a command runs.
  2. Files are written without following symbolic links, and never over an existing file.
  3. The files are checked against both checksum records (see `backup`).
  4. **The dump's SQL is rendered and screened before it runs.** `pg_restore --no-owner -f -` writes the script without
     connecting to any database; it is held in a private temporary file, read once, and removed. The screen refuses
     the word `PROGRAM` (as in `COPY ... FROM PROGRAM`), `ALTER SYSTEM`, `CREATE`/`ALTER`/`DROP` of a role, user or group, a
     database or tablespace, a language, an event trigger, a foreign data wrapper, server or user mapping, a publication or a
     subscription; `SET ROLE` and `SET SESSION AUTHORIZATION`; `CREATE EXTENSION` of anything but `pgcrypto`, `pg_trgm` and
     `plpgsql` (the extensions the schema uses); functions in any language but `sql` and `plpgsql`; file, large-object and
     server-control functions (`pg_read_file`, `lo_import`, `lo_export`, `pg_reload_conf`, `dblink`, and similar); anonymous
     `DO` blocks; `COPY ... TO` anything and `COPY` from a named file; `session_replication_role`; the settings that name a
     program or a library (`archive_command`, `shared_preload_libraries`, and similar); and `psql` meta-commands in the
     script. Row data (the `COPY ... FROM stdin` blocks) is skipped, so a document that talks about these things does not
     trip it.
  5. The screen is a list of what a Dynodoc dump never needs, applied to text. It does not understand SQL: a determined
     author can write a statement the patterns do not name, and `SECURITY DEFINER` functions and `plpgsql` bodies in a
     legitimate dump are not analysed. **Running pg_restore as a less privileged role was considered and not chosen**: the
     dump's ownership, `SECURITY DEFINER` functions and `ALTER DEFAULT PRIVILEGES` need an owner, and a restore that fails
     half way on privileges is worse than a screen that is honest about its limits.
- A screened-out backup is refused with the rule and a short excerpt of the SQL (never row data), before any service is
  stopped and before anything is created.

### `restore-verify`

```sh
dynodoctl restore-verify [--backup latest|NAME|DIR] [--config-from DIR]
```

Loads a backup into a **throwaway database** in the running PostgreSQL (`dynodoc_restore_<time>_<random>`, never the live
one), checks the counts against the manifest, runs `engine-cli verify-snapshots --all` and `engine-cli verify` for each
document, prints a restore report (counts, hash chain verified, schema level, timings) and removes the database even when a
step failed. Needs the stack running. Only a database this run created is dropped by it: the name carries random text, and
a failed `create database` (the name already existed) leads to no drop. The same holds for `rehearse-lite`. The tool can drop
only names that match `dynodoc_(restore|restoring|prev|rehearse)_...`; `dynodoc`, `postgres` and the templates are never
among them.

### `restore --into-live`

```sh
dynodoctl restore --into-live [--backup NAME|DIR] [--config-from DIR] [--no-safety-backup] [--keep-previous]
```

Replaces the live database with a backup. It asks for the typed phrase `REPLACE LIVE DATA` (or `--confirm`); without it
nothing happens (exit 3). Order:

1. Refuse if `state/restore-journal.json` exists (an earlier swap is unfinished: `dynodoctl recover`), if the backup's
   files do not match both checksum records, or if the Compose project belongs to another directory.
2. Put back `.env` and `secrets/` from the backup if the directory has none (confined and exclusive, see "Trust").
3. Start PostgreSQL; run the `db-roles` job (the runtime login must exist before a dump with its grants is loaded).
4. Ask PostgreSQL what the live database holds: missing, empty, or present. **A failed question stops the restore** (it
   used to be read as "empty", which skipped the safety backup).
5. Check the backup again and **screen its SQL** (Trust, item 4). A backup that fails is refused here, with no downtime.
6. Stop proxy, web and api, **then** save the current database as a `pre-restore` backup (unless empty or
   `--no-safety-backup`), so the copy is of a database nobody is writing to.
7. Load the dump into a **new** database beside the live one (a damaged dump fails here; the live database is untouched).
8. **Write `state/restore-journal.json`**, then rename the live database away and the new one into place **in one
   transaction** (`begin; alter database dynodoc rename to <prev>; alter database <new> rename to dynodoc; commit;`), so a
   crash or a kill cannot leave the server without a live database.
9. `up -d --wait` (db-roles and init re-apply the grants); verify counts, history (`verify-snapshots --all` and `verify` for
   each document) and `check-runtime`.
10. Drop the replaced database only if everything verified (or keep it with `--keep-previous`), then remove the journal.

**If anything fails after the writers were stopped**, the tool undoes what it did and says what is left: before the swap it
drops the partly loaded copy and starts the stack again; after the swap, a failed start, a failed count, a failed
verification, a failed `check-runtime` or an interrupt **swaps the old database back** (the same single transaction with the
roles exchanged), keeps the rejected copy as `dynodoc_restoring_<...>` for you to inspect, restarts the services and removes
the journal. If even the swap back fails, the journal stays, the report prints the exact commands (the same ones as
section 6.8), and `status` and every command that could change the databases refuse until `dynodoctl recover` has run. The
report says `live_changed` and `swapped_back`.

Install files that differ from the backup's copy are kept and reported. `restore` without `--into-live` is a usage error
that points at `restore-verify`.

### `recover`

```sh
dynodoctl recover
```

Reads `state/restore-journal.json` (written before every swap and removed only when the installation is consistent), looks at
which databases exist, and does the one safe thing: if the swap never happened it drops the loaded copy; if it happened and
the copy was not yet verified it swaps the old database back; if the copy had verified it finishes the clean-up; it starts the
services; then it removes the journal. A state it cannot decide (no live database and no candidate, or all three databases
present) is printed with the manual step and the journal is kept. Run it after a power cut, a killed terminal, or whenever
`status` reports an interrupted swap. With no journal it says so and does nothing.

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

Order: plan; pull the new images and **check them against the release** (below); a **mandatory pre-upgrade backup**; the
rehearsal on a copy of that backup; save `.env` and the overlay in `state/pre-upgrade-<UTC>/` and **write the upgrade
record (`result: "switching"`)**; write the new version; `docker compose up -d --wait` (the `init` job migrates); check
that every service is healthy; print how to roll back. A failed pull, digest check or rehearsal stops before anything is
switched (exit 1 or 3) and leaves the installation as it was. If the switch itself fails, or an interrupt arrives before the
services were touched, the saved `.env` and overlay are put back. If the process dies after the record was written, `status`
reports an upgrade that did not finish and `rollback` finds it (the record used to be written only at the end, so an upgrade
cut short said "nothing to roll back" although `.env` had been switched).

**The gates cannot be bypassed with a flag.**

- `--local` means "the images already installed". It cannot be combined with `--version`, `--registry` or `--overlay`
  (exit 2): `--local --version X` used to switch the version with no overlay, no digest check and no `--allow-unpinned`.
- A release upgrade needs `--overlay`, or `--allow-unpinned` (trust the registry tag; a warning says so). If an overlay from
  the installed release is in the directory and you pass `--allow-unpinned`, the upgrade is **refused**: the old pinned
  digests would keep running while `DYNODOC_VERSION` moved on. Pass the new release's `--overlay`, or move the old overlay
  out of the directory on purpose.
- The digest check reads the configuration Compose will actually run (`docker compose config`, overlay applied, the
  `maintenance` jobs included) and demands that **every service image is pinned by a full digest** (`@sha256:` and 64 hex
  characters, whichever file pins it), that each image the overlay names is the one Compose runs, and that each pulled
  image's `RepoDigests` contain **exactly** that repository and digest. It names the services that are not pinned. A digest
  that merely contains the expected text, or the right digest under another repository, does not pass. If the
  configuration cannot be read, the images are not trusted.
- `install --overlay` on an existing installation is refused (see `install`).

`--skip-backup` needs the phrase `I HAVE NO BACKUP`; `--skip-rehearsal` needs `SKIP REHEARSAL`; both are recorded in
`state/upgrade-<UTC>.json`. If the new version does not become healthy the tool offers the rollback (phrase `ROLLBACK`, or
`--auto-rollback`); while an interrupt is pending it does not ask.

`rollback` restores the pre-upgrade backup into the previous version. **It checks first and changes nothing until the
checks pass**: the upgrade record, the backup (found, both checksum records, the settings copy), the unfinished-swap journal
and the confirmation come before any file is touched (it used to rewrite `.env` and the overlay and only then discover a
damaged backup, while saying "nothing was changed"). Then it puts the saved `.env` and overlay back and runs the same safe
restore as `restore --into-live`. If that restore fails without having replaced the database (or after it swapped back), the
`.env` and overlay from before the rollback are put back and the services restarted with them, because the old data is still
the live data. Data written after the pre-upgrade backup is lost. Rolling back is a restore because migrations only move
forward and an older image refuses a newer database.

### `support-bundle`

```sh
dynodoctl support-bundle [--out DIR] [--log-lines 400] [--keep-emails] [--list]
```

One `.tar.gz` with versions, `docker compose ps` and `config`, recent logs, `.env` with secret-looking values hidden,
disk and memory, certificate summaries, the migration level, backup names and ages, and `status --json`, plus a
`manifest.json` with a checksum for every entry. It prints exactly what it contains. **It never contains** a file from
`secrets/` (one guard refuses every such read), a private key, a backup, or document content. This installation's own
secret values are loaded into memory only to be hidden wherever they appear. On top of that these patterns are replaced:

- URL user information, up to the **last** `@` before the path (so `redis://:pw@host`, a password that contains `@`, and a
  bare token before the `@` are hidden; the user name of `user:password@` is kept);
- the values of query parameters such as `code=`, `state=`, `access_token=`, `token=`, `key=`, `sig=`, `session=`;
- bearer tokens of any length, PASETO and JWT tokens, private-key blocks;
- every `name=value` or `name: value` whose NAME has a sensitive part (split at `_`, `.`, `-` and capital letters, plural
  ignored): password, passwd, pwd, passphrase, secret, token, credential, bearer, authorization, cookie always; key,
  apikey, session, code, signature, auth, private, pass, jwt, otp unless the value is a short number or a boolean. The
  value is the quoted string to its closing quote, or the rest of the line when unquoted, so `password="two words"`, a
  whole `Cookie:` header with every pair, and `Authorization: Basic ...` are hidden. A name that ends in `_file`,
  `_path`, `_dir`, `_seconds`, `_ttl`, `_size` or `_count` is not (it is a path or a number), and a JSON document stays
  valid JSON;
- a 20-character or longer blob (hex or base64) that follows the word `secret`, `token`, `password`, `key` or
  `credential`, and any 48-digit or longer hex run (image digests written `name@sha256:...` are kept; any other name that
  merely ends in `sha256:` is not);
- email addresses, unless `--keep-emails`: ASCII, `%40`-encoded, full-width `＠` and addresses with letters of any script.

**What it does not recognise:** a secret that has no sensitive name next to it, is not one of this installation's own secret
values and is shorter than a long hex run; an email written as "name at example dot org". Redaction can hide too much by
design (a `status_code=200` survives, a `session=ok` does not). `--list` shows the inventory and writes nothing. Read the
archive before sending it.

### `verify-delivery`

```sh
dynodoctl verify-delivery [--origin URL] [--connect HOST[:PORT]] [--http-port N] [--tls-mode M] [--mail-to ADDR] [--ca-file FILE]
```

Looks at the installation from the operator's machine, as a browser would: HTTPS reachable at the origin; the connection
negotiated TLS 1.2 or newer (this client offers 1.2 and 1.3 only, so whether the server would also accept 1.0 or 1.1 is
**not tested**, and the check says so); the chain valid for the name (an evaluation certificate is a warning only when the
certificate mode, from `--tls-mode` or `.env`, is `evaluation`; in any other mode it is a failure, and the message names it);
the expiry horizon (warning under 30 days, failure under 7); plain HTTP redirects to the origin; the sign-in page; exactly
one content-security-policy naming the origin, HSTS (the directive name is case-insensitive), `nosniff`, `no-referrer`,
`SAMEORIGIN`, no version banner, `noindex`; `/healthz`; another host name gets 421; the API refuses anonymous requests; the
marketing pages answer 404; the front page sends visitors to sign-in; what is served names no outside origin; and, when run
in the install directory with SMTP configured, the mail handshake (nothing is sent).

**What the server says about itself never relaxes a check.** The evaluation allowance comes only from the operator's
statement of the certificate mode, not from the certificate's subject or a response header. The plain-HTTP redirect is
parsed as a URL: it must be `https`, the origin's host, the origin's port, with no user name in it; a `Location` that merely
starts with the origin (`https://docs.example.org.evil.example/`, `https://docs.example.org@evil.example/`) fails.

The outside-origin check reads the sign-in page and follows its scripts, inline `import()` calls and static imports, and
also absolute references to the installation's own origin (up to 60 files, eager files first). **Every absolute `http(s)`
origin found anywhere in what was read is reported, once each, whatever its name**, except: the installation's own origin;
the origin of the operator's own `DYNODOC_SUPPORT_URL`; and `www.w3.org` (XML namespace identifiers in SVG and MathML,
which are never fetched). An origin in a file the sign-in page loads, or in any tag the browser loads by itself, **fails**.
The same origins only in lazily loaded route modules **warn** and name the open item: the web build still contains the
compiled marketing routes (acceptance criterion 6 of Phase 0, task P0-6). Documented optional references (the Office.js CDN,
`accounts.google.com`, `resend.com`, the engine repository link) are warnings. **A crawl cut short by the 60-file cap, or a
file that could not be read, is a warning, never a pass**; the message says how much was left unread.

`--connect` takes `HOST`, `HOST:PORT`, `[::1]`, `[::1]:8443` or a bare IPv6 address (`::1`). Every request has an overall
deadline (30 s) as well as per-read timeouts, and the bytes read are capped (6 MiB), so a server that sends one byte every
few seconds, or a hostile chunk size, ends the check with an error instead of a hang or a crash.

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
| Trust in a backup | A restore trusts the backup; the tool confines paths, cross-checks two checksum records and screens the rendered SQL, and says plainly that this is not authentication | `pg_restore` runs the dump as a superuser. A least-privileged restore role was rejected: ownership, `SECURITY DEFINER` functions and default privileges need an owner, and a half-failed restore is worse than an honest screen. Signed backups are future work. |
| Swap and recovery | Journal first, one-transaction rename pair, automatic swap back on any failure, `recover` for the rest | Two separate renames left a window with no live database; a failure after the swap used to leave the unverified copy live. |
| Concurrency and interrupts | `state/.lock` for mutating commands; a signal sets a flag honoured at step boundaries; destructive children run in their own process group | A second run, a Ctrl-C or a kill between two steps must not leave a half-done restore. |
| Files and access | Owner-only on every platform, enforced and read back on Windows with `icacls`; refuse when it cannot be done (`--allow-insecure-files` accepts) | A step that says "private" must be true. |
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
restore verifies. If the program was killed during an upgrade, `dynodoctl status` says the upgrade did not finish; check the
services, then `dynodoctl rollback` (it finds the record) or `docker compose up -d --wait` to go on.

### 6.7 Support

`dynodoctl support-bundle --list`, then without `--list`; read the archive; send it. For a suspected leak use
`dynodoctl status --json` and the guide's chapter 9.13.

### 6.8 Undo a restore

A `restore --into-live` that finished and verified dropped the database it replaced (unless you gave `--keep-previous`).
What you can still do depends on what you kept:

1. **The safety backup.** Unless you gave `--no-safety-backup`, the restore first saved the database as it was:
   `backups/dynodoc-<UTC>-before-restore/` (kind `pre-restore`; the last three are kept). To undo, restore that backup:
   `dynodoctl restore --into-live --backup dynodoc-<UTC>-before-restore`. Writes made after the first restore started are
   not in it.
2. **`--keep-previous`.** The replaced database is still on the server as `dynodoc_prev_<UTC>_<random>`. Stop the services and
   swap the names back in **one transaction**, then start; run these in the bundle directory (replace the two names with the
   ones in the restore report; `docker compose exec` runs inside the PostgreSQL container, where the password is read from the
   secret file and never typed):

   ```sh
   docker compose stop proxy web api
   docker compose exec -T postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) psql -h 127.0.0.1 -U dynodoc -d postgres -v ON_ERROR_STOP=1 -c "select pg_terminate_backend(pid) from pg_stat_activity where datname = '\''dynodoc'\'' and pid <> pg_backend_pid()"'
   docker compose exec -T postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) psql -h 127.0.0.1 -U dynodoc -d postgres -v ON_ERROR_STOP=1 -c "begin; alter database dynodoc rename to dynodoc_undone_restore; alter database dynodoc_prev_<UTC>_<random> rename to dynodoc; commit;"'
   docker compose up -d --wait
   dynodoctl status
   ```

   The restored data is now in `dynodoc_undone_restore`. When you are sure, drop it:
   `docker compose exec -T postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) dropdb -h 127.0.0.1 -U dynodoc dynodoc_undone_restore'`.
   (dynodoctl itself only drops names that start `dynodoc_restore_`, `_restoring_`, `_prev_` or `_rehearse_`; the name above
   is yours, so the tool will not touch it.)
3. **Neither.** Restore the newest backup you have. There is no other copy of the data that was replaced.

After any undo: `dynodoctl status`, and take a new backup.

### 6.9 A swap was interrupted (power cut, killed terminal, crash)

`dynodoctl status` reports "an interrupted database swap is recorded", and `restore`, `upgrade` and `rollback` refuse until
it is dealt with. Run `dynodoctl recover`. It prints what it found and what it did:

| What exists on the server | What `recover` does |
|---|---|
| `dynodoc` and the loaded copy (the swap never happened) | drops the loaded copy, starts the services |
| `dynodoc` and the previous database, copy not yet verified | swaps the previous database back in one transaction (the rejected copy is set aside as `dynodoc_restoring_...`), starts the services |
| `dynodoc` and the previous database, copy verified | drops the previous database, starts the services |
| the previous database and the loaded copy, no `dynodoc` (only if the rename pair was not one transaction) | renames the previous database to `dynodoc`, drops the loaded copy, starts the services |
| anything else | prints the manual step and keeps the journal |

By hand, the equivalent of the second row is the transaction in 6.8, item 2, with the names from
`state/restore-journal.json` (`live`, `restoring`, `previous`: rename `live` to `restoring`, then `previous` to `live`). When
the databases are right and the services are up, delete `state/restore-journal.json`; `recover` does that itself.

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
| `verify-delivery`: `What is served names no origin but its own` | A file the sign-in page loads names another origin | The message lists each origin once and where it was found |
| `verify-delivery`: route modules name another origin | Known open item (criterion 6) | A warning; closes with task P0-6 |
| `verify-delivery`: files not read | The crawl hit its 60-file cap or a file did not answer | A warning, not a pass: the message says how much was left |
| Scratch databases listed by `status` | A check was interrupted, or a rejected restore copy was kept | `docker compose exec postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) dropdb -h 127.0.0.1 -U dynodoc <name>'` |
| `an interrupted database swap is recorded` | A restore was cut short | `dynodoctl recover` (section 6.9) |
| `another dynodoctl is already running` | `state/.lock` is held by a live process | Wait for it; if the process is really gone and the message still appears, delete `state/.lock` |
| `containers of the Compose project ... already exist for another directory` | Two bundle directories share a project name | Give this one its own `COMPOSE_PROJECT_NAME` in `.env`, or `--project` |
| `the dump contains SQL that a Dynodoc backup never has` | The screen found a rule it refuses (Trust) | Do not restore that backup; find out how it differs from the ones you made |
| `the manifest names a path outside the bundle` / `is not at an allowed path` | A backup that was altered, or not made by dynodoctl | Do not use it |
| `--local cannot be combined with --version` | A release upgrade needs its overlay | Drop `--local`, or drop `--version` |
| `could not be restricted to the current user` (Windows) | The folder's file system has no access lists | Use an NTFS folder, or pass `--allow-insecure-files` knowingly |

## 8. What was tested, and what was not

See the status paragraph in [SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md), section 13.2 (P0-5): unit tests against a
fake runner and a fake network, thirteen mutation checks, and a run of every command against the real Docker bundle on
Windows 11 with Docker Desktop. Not run: a Linux host, arm64, a real certificate authority, a real mail server, a real
upgrade from one release to a newer one, remote backup targets, a signed bundle, an air-gapped host, high availability.
