# Decision 019: the desktop app is a standalone product with a local workspace

Date: 26 October 2026. Owner: Verne. Status: built for 0.0.1b (pre-alpha); acceptance in
[ACCEPTANCE-0.0.1b.md](../ACCEPTANCE-0.0.1b.md). Extends decisions 012 (local-first), 016 (desktop
and mobile apps) and 017 (signing the native apps in).

## The question

Until 0.0.1a the desktop app was a client: every screen needed a Dynodoc server (the hosted
pre-alpha server, or one the person runs). Nothing ran on the computer itself. The owner's request
for 0.0.1b: one installer; the first launch works with no command line, no daemon, no account and no
network; the workspace is real (import Word, Excel and PowerPoint files, history, drafts, change
requests, reviews, merges, restore, export); signing in to the web is an addition for syncing.

## Where the data lives: bundled PostgreSQL, managed by the app

Two routes were open.

1. **A SQLite backend behind the engine's storage traits.** The engine has no such traits. The
   centralized workspace (documents, the hash-chained event log, drafts, change requests, reviews,
   people, issues) is written against PostgreSQL: `sqlx` with the `postgres` driver in every query,
   47 migrations that use `plpgsql` triggers (the append-only event log, the guards on audited
   erasure, the restricted runtime login's explicit grants), `pgcrypto` and `pg_trgm`. A SQLite port
   would be a second implementation of the engine's rules with its own bugs, and the hash chain and
   the review rules are exactly what must not drift between a local copy and a server.
   The engine does have a SQLite store (`engine_core::repository::local`, decision 012), but it is a
   different thing: a portable repository of signed multi-file commits for offline exchange. It has
   no drafts, change requests, reviews, issues or roles, and the client's screens read those. It
   stays what it is (`engine-cli repository` and `sync`, for devices that exchange signed commits).
2. **Ship PostgreSQL and run it.** The same engine, the same migrations and the same tests as the
   hosted workspace, so a file imported locally behaves exactly as it does on a server, and the
   hosted workspace and the local one can exchange work (see "Syncing").

Route 2 is built. The cost is size: the Windows PostgreSQL tree is pruned from 102 MB to about 59 MB
(no wx and curl libraries, no contrib, no translations, only `plpgsql`, `pgcrypto`, `pg_trgm` and
the UTF-8 converters; the ICU data alone is 27 MB and `postgres.exe` links it). The installer is
large for that reason and says so.

What the app does, in `src-tauri/src/local/`:

- **One cluster per person**, in `<per-user data folder>/workspace/pgdata`, created by `initdb` at
  the first launch (UTF-8, `C` collation, data checksums, SCRAM logins only), on a loopback port
  chosen at every start. Passwords (database owner, restricted runtime login), the engine's token
  key and its service credential are generated once and kept in the operating system's credential
  store (Windows Credential Manager, macOS Keychain, Linux Secret Service). With no credential store
  (headless Linux) they fall back to a file only the person can read; provider keys never do.
- **Elevation.** PostgreSQL refuses to run with administrator rights. The app starts and creates the
  cluster only through `pg_ctl` and `initdb`, which on Windows run the server with a restricted token
  when they were started elevated. This path is not exercised by the acceptance run (it needs an
  interactive UAC prompt).
- **Crash safety.** A server an earlier run left running is adopted, not started twice. One that was
  killed mid-write recovers from its write-ahead log at the next start (`pg_ctl start -w` waits for
  it); `fsync`, `synchronous_commit` and `full_page_writes` stay on. The engine runs in a Windows job
  object that ends with the app, and an engine process recorded by an earlier run is ended before a
  new one starts.
- **Migrations at every start**: `engine-cli migrate`, `grant-runtime`, `check-runtime`, so an
  update's new tables arrive with the app and the engine still refuses a schema it does not know.
- **The engine** (`engine-api`) is a sidecar on a loopback port with the keys in its environment
  (never in an argument), restarted with a backoff if it dies. The page never sees its address: the
  app signs in the one local person with the engine's own sign-in route (the app plays the web
  tier's part) and keeps the token itself.
- **The client's own state** (captured changes, unpushed commits, agent threads, commit messages,
  folder bindings, links to servers, the agent's usage ledger) lives in a second database of the
  same cluster, `dynodoc_app`, behind the same routes the hosted pre-alpha service has, so the
  client code is the same for the local workspace and for a server.
- **Backup and restore** are cold copies of the cluster: the app stops the database for a moment,
  writes one zip (the data directory, the secrets needed to open it, and a manifest with the
  versions), and restores by the reverse. A backup opens only with the same PostgreSQL major version
  on the same kind of operating system; that is stated where it is offered.

## What stays out

- **The Node web workspace is not shipped.** The client is the interface; the engine is the API.
  Importing and exporting Office files is the web workspace's own TypeScript (importers, exporters,
  the push planner), bundled for a worker (`client/office/`), so both produce the same blocks and
  files. The worker has no network. Legacy `.doc` needs the LibreOffice converter service and is not
  supported locally.
- **The Office add-ins** need HTTPS and a hosted origin; they keep working against the hosted server.
  In local mode the app watches a folder instead (see below).

## Folder watch

A project can be bound to a folder. The app looks at it every two seconds (names, sizes, times; no
hooks, so it works on network and synced drives), reports a Word, Excel or PowerPoint file once it
has stopped changing, and the page turns a save into changes with the web workspace's push planner:
a copy exported by Dynodoc carries its document and revision in its properties; any other file is
aligned with the project's blocks by content. The result is an ordinary draft, reviewed and merged
like any other. The app never writes into the folder except for an exported copy, and never
overwrites a file.

## Web sign-in and syncing

Dynodoc's own sign-in (decision 017) is reused: the app opens the server's `/app-signin` page in the
system browser with a PKCE challenge, the person signs in there with Google or email, the server's
confirmation page hands a one-time code to the app on a loopback address (`127.0.0.1:<port>`), and
the app redeems it with its verifier. The `dynodoc://signin` link of 0.0.1a remains the fallback.
The session is kept in the credential store per server; the app never sees a password. A server
that does not offer the loopback hand-over still works through the link. Any Dynodoc server works,
not only the hosted one.

Syncing a local project with a remote one goes through the server's ordinary workspace API (the
same routes the web workspace uses), not through shared project history (schema 36), which the
hosted server keeps off. Push sends the local files' new revisions as one change per file;
pull reads the remote state and brings in what changed there; a block changed on both sides is a
conflict the person resolves (the same four-card conflict as a change request). The link remembers
the revision each side had at the last sync, so a repeated sync sends and reads only the difference.

## Models

The Agent screen calls the model providers the person configures, with their own keys, from the
app's Rust process only (decision in `docs/AGENT-0.0.1b.md`): the page has no network.
