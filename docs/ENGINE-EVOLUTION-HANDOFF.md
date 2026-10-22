# Engine evolution handoff

## Source: self-hosted Phase 0, single-node bundle (23 October 2026)

P0-4 is built in source and was run end to end in Docker on this PC (Docker Desktop 29.5, Compose 5.1.4, amd64, Windows 11). Nothing is
deployed or published. Files: `infra/selfhosted/` (Compose project, four image Dockerfiles, hardened nginx proxy, one-shot jobs,
smoke and TLS tests, README quick start); deployment guide and edition plan updated.

**Topology.** `postgres` 16, `api`, `web` (`DYNODOC_MODE=selfhosted`), `proxy`; on demand `acme`, `converter`, `database-admin`,
`mailcheck`. Four one-shot jobs run on every `up` and stop the stack on failure: `secrets-init`, `preflight` (the web configuration
check, in the `tools` image target), `db-roles` (creates the runtime role; needs psql, which the API image lacks) and `init` (migrate
as the owner role, `grant-runtime`, `check-runtime`). Only the proxy publishes ports, on 127.0.0.1 unless `DYNODOC_BIND` says
otherwise; networks `published`, `frontend` (internal), `engine` (internal), `conversion` (internal) and `egress` (web and acme, for
mail). The API and PostgreSQL are unreachable from the host and from the proxy. The runtime containers never receive the owner
credential.

**TLS modes** (`DYNODOC_TLS_MODE` is required, no default): `provided` (key match, dates, names, chain order and passphrase
checked, the proxy refuses to start with the reasons; graceful renewal, a bad replacement is refused and the old certificate keeps
serving), `evaluation` (self-signed with SANs, labelled in the subject, a header and the log; HSTS 300 s), `acme` (lego 5.5.2, HTTP-01
through the proxy, terms must be accepted in the env file). Tested: provided 16 good and bad chains; renewals with 0 failed requests;
ACME **against Pebble only** (17 of 17), not against a real CA.

**Hardening, measured with `docker inspect` and the live process status:** every service non-root (postgres 999, api and web 10001,
proxy 101), read-only root, all capabilities dropped (effective set 0; `secrets-init` keeps three), `no-new-privileges`, memory and
pid limits, rotated logs, SCRAM on every PostgreSQL connection; the runtime role cannot CREATE (including TEMP), DROP, ALTER,
TRUNCATE, DELETE or UPDATE the event log or create extensions. Secrets are copied by `secrets-init` into one volume per service
(Compose `secrets:` ignores uid, gid and mode, so a 0600 file is unreadable and an open one is refused by the API).

**Smoke results** (`scripts/smoke.mjs`, checks a to k plus extras): clean build run 106 passed, 0 failed (345 s; first `up` 60 s);
release path with images pulled by digest from a local registry 105 passed, 0 failed; 69 checks through the Git Bash launcher. They
cover the single published port, redirect to the configured origin even with a forged Host, one CSP with HSTS and nosniff, TLS 1.0
and 1.1 refused, 421 for other hosts, 413 over 16 MiB, the hosted CSP maps identical to `infra/demo`, sign-in through a test-only mail
sink, an admin from `ADMIN_EMAILS` reaching `/admin`, every service restarted with data intact, `down` then `up` idempotent, four
failure injections each stopping `up`, a by-hand backup and restore with `verify-snapshots --all`, and a Chromium run (sign in,
create a document, see it listed). Four manual mutations (bind 0.0.0.0, HSTS, capabilities not dropped, an extra schema grant)
each failed the expected checks. Images: api 182 MB (cold Rust build 23 min), web 536 MB, proxy 92 MB, acme 103 MB; base images
pinned by digest (`pin-bases.mjs`), SBOMs from Docker Scout (api shows no Rust crates until `cargo-auditable`, P0-6).

**Not run / not verified.** A Linux host (bind-mount ownership, rootless Docker, SELinux, ufw), arm64, a real CA, a real mail
server, an upgrade from release N to N+1 (only the `init` re-run is exercised), the converter image build, air-gap, HA, any GitHub
Actions job. **Not independently reviewed** (the measured evidence above is the author's). Observations in the app, unchanged: `GET`
and `PUT /telemetry` answer 405 with metrics off while the guide says 404; the first sign-in-link request creates the account; without
working SMTP nobody can sign in, and `DEV_AUTH=true` lets anyone sign in as anyone (both documented). The first administrator today
comes from `ADMIN_EMAILS` plus an emailed link; operator-issued links are not built. Questions for the owner: accept the two extra
networks; keep `DYNODOC_TLS_MODE` mandatory; lego v5 as the ACME client (an nginx module was not evaluated); a LAN-reachable default
instead of 127.0.0.1.

## Source: self-hosted Phase 0, SMTP mail transport (22 October 2026)

P0-3 is built in source for the web tier (the operator-issued sign-in link, the outbox and the `dynodoctl` call are not). A mail
transport abstraction has three providers selected by `DYNODOC_MAIL_TRANSPORT`: `resend` (the hosted behaviour, pinned by a golden
test written before the refactor and unchanged after it), `smtp` and `none`; the default is `resend` when `RESEND_API_KEY` is set,
otherwise `none`. With `none` the interface says so plainly: email sign-in is unavailable and an invitation shows its link to the
inviter to copy, never a silent drop. A self-hosted installation with email sign-in on and no transport that can send is a startup
error unless `AUTH_EMAIL_LINK=off` (a warning in hosted; `DEV_AUTH=true` also only warns, so the existing e2e stacks keep working).

**SMTP.** Library: nodemailer 10.1.0, exact pin, the only new dependency, loaded only when the transport is `smtp`; licence MIT-0, zero
runtime dependencies, published with npm provenance; the integrity hash in the lockfile equals the registry tarball's (checked again
independently when merging) and `pnpm 10.34.5 install --frozen-lockfile --lockfile-only` accepts the hand-added lockfile entries
(pnpm 11 had rewritten the whole file, so those three entries were added by hand). An in-house client was rejected as security-
critical parsing. Settings (documented in SELF-HOSTED-DEPLOYMENT-GUIDE.md 7.2): `SMTP_HOST`, `SMTP_PORT` (587 / 465 / 25 by
security mode), `SMTP_SECURITY` = `tls` | `starttls` (default) | `none-local`, `SMTP_USER` with `SMTP_PASSWORD[_FILE]`, `SMTP_FROM`,
`SMTP_FROM_NAME`, `SMTP_REPLY_TO`, `SMTP_CA_FILE`, `SMTP_TIMEOUT_SECONDS`; fixed in code: 3 attempts, 10 recipients, 256 KiB, TLS
1.2 minimum. Certificate and host-name verification are always on (`NODE_TLS_REJECT_UNAUTHORIZED=0` only warns and mail still
verifies); credentials are never sent in clear; a server that omits STARTTLS is refused; `none-local` accepts only loopback or
private addresses, checked at send time. 4xx and network errors retry with jitter, 5xx never; header injection, quoted addresses,
too many recipients and oversize messages are refused before any connection. Logs carry provider, outcome, a short error class and
a message id, never a recipient or secret. `scripts/check-mail.mjs` performs the real handshake and prints plain diagnostics (for the
future `dynodoctl`); `dynodoc-env.mjs` writes `AUTH_EMAIL_LINK=off` by default or a checked SMTP block.

**Behaviour beyond the brief (hosted):** the Resend path now logs one line per send (no recipient, no secret); `RESEND_API_KEY`
without `EMAIL_FROM` counts as not configured instead of attempting a send that fails; a hosted invitation response gains
`invitation_link` only when no email went out; the invitation text in self-hosted mode names the brand and the sign-in methods that
exist (hosted wording pinned unchanged). Existing tests changed only where the new rule makes a complete self-hosted setup need mail;
no assertion was loosened.

**Evidence (local; Actions of this repository cannot run).** vitest 84 files, 943 passed, 4 skipped (Unix file modes) on the merged
tree, including 51 integration tests against an in-process SMTP server with a real self-signed certificate made at test time
(greeting, EHLO, STARTTLS, AUTH PLAIN/LOGIN, 4xx then success, 5xx, silence, AUTH only before TLS, wrong host name, STARTTLS
downgrade); prettier, eslint, svelte-check, build and the hosted-string audit clean; Chromium e2e on an isolated stack (the sign-in link
arrives at the configured origin and signs in once, a refusal is shown plainly, the invitation is emailed with the inviter as Reply-To
and an escaped title, the no-transport invitation link is shown): 4 new specs; the whole hosted Chromium suite 128 passed, 6 skipped
by design, 0 failed. Twelve manual mutations each failed a test (certificate verification off, STARTTLS not required, plaintext
to a public address, CR/LF in headers, HTML escaping, retry on 5xx, TLS minimum, recipient in the log, the sign-in rule and others).
**Not independently reviewed** (the author's mutations are the only adversarial check so far). **Not verified:** any real mail server
or provider (Postfix, Exchange, a hosted relay, a private CA), IPv6 targets, the Unix file-mode tests on Linux, the CI job. Not built:
outbox or queue, operator-issued sign-in links (so P0-3 is only partly done), XOAUTH2, DKIM signing (guidance only), bounce handling,
a cap on concurrent sends (a flood of sign-in requests can open many SMTP connections; the API's per-address limits still apply).
Questions for the owner: bump nodemailer to 10.1.7 or later once it has aged; add a small semaphore on concurrent sends.

## Source: Android app built and run on an emulator (22 October 2026)

The Tauri shell now has a committed, hardened Android project (`apps/desktop/src-tauri/gen/android`) and was built, installed
and exercised on an Android 16 x86-64 emulator (Google APIs image, WebView 133, hardware acceleration worked). Toolchain, all
under `E:\android`, nothing permanent in the environment: Temurin 17.0.20.1, command-line tools 15859902, platform 37, build-tools
37.0.0, NDK 28.2.13676358, Gradle 9.6.1 (wrapper pinned by sha256, jar checked against the published hash), AGP 9.3.1, Kotlin 2.2.10,
Tauri CLI 2.12.3. Tauri's own CLI cannot finish on this PC (no symlink right without Developer Mode), so `scripts/android-build.ps1`
compiles with the CLI, copies the library and runs Gradle: same APK. Debug universal APK about 45 MiB, release (R8) unsigned 10.9 MiB.
minSdk is 29 (raised from 24 to remove the pre-Q storage-permission and task-hijack cases; check the device share before a store
release), target 37.

**Merged manifest audited by script on real aapt2 / apkanalyzer / apksigner output** (`check-apk-manifest.mjs`, fixtures from the real
APK, exit 2 on anything it cannot parse; attributes keyed by namespace so a foreign-namespace attribute cannot hide an exported
component): INTERNET only, MainActivity exported with three `dynodoc://open/...` filters (no `autoVerify`) and one AndroidX receiver
guarded by DUMP, backup off, cleartext off, extraction rules complete, empty task affinity, debuggable only in debug, debug key only.
The Kotlin adds no JavaScript bridge (tested). Downloads: only a plain https file from the exact site (scheme, host, port) after a real
touch in the last 5 s, at most one per 3 s, cleaned names, executable types refused, **no cookies ever sent**, so signed-in
downloads and `blob:` exports are NOT supported on the phone yet (a toast says to use the browser). Main-frame requests to another
origin get a 403 page with a way back (this closes cross-origin form POSTs and history navigation, which the usual hook does not
see); a dead renderer re-creates the activity instead of killing the app; Back goes back through the site then to the background.

**Emulator results.** `android-probe.mjs` (real taps, WebView over adb and the DevTools protocol): 13 phases and 72 checks pass:
20 commands from a hosted page refused; external links, `_blank` and `window.open` open the system browser once and the window
stays; mail, local and credential links do nothing; a 40-link flood is rate limited; iframes never reach the shell; about 25 hostile
deep links dropped (dot segments collapse before the parser: `../workspace` is accepted as `/workspace`, which is normalisation, not
a bypass); POST and history bypass refused; renderer kill; re-creation with a server saved (blank-window defect found and fixed);
rotation, dark mode, offline, keyboard, the phone address validator. Screenshots were viewed: 44 px targets, 16 px fields, no
horizontal scroll, cut-out in landscape. Defects found by running and fixed: the window opened the server root (the hosted root
redirects to the marketing site, so the window stayed empty: now `/workspace`, desktop too), a second launcher load denied, `target=_blank`
silently dropped by the WebView, Back closing the app, content under the system bars and the keyboard, downloads ignored, the stock
offline page. Gates: shell tests 107, script tests 44, Kotlin unit tests 10, clippy and fmt clean. An independent review found no
Critical or High issue; its Medium (downloads) and Lows are fixed.

**Not done / not verified.** **Signing in inside the app is impossible today**: the emailed link and Google sign-in open the browser, a
phone has no menu and the launcher is unreachable once a server is saved; it needs App Links (assetlinks.json plus the release key)
or a one-time hand-over code, which is the next slice. Changing the server on a phone needs the app data cleared. No arm64
execution, no physical phone, no Play signing or AAB, no CI run (the `desktop.yml` Android job is written and unrun because the
account billing blocks Actions), `gradle/verification-metadata.xml` was written on Windows with the Linux and macOS aapt2 hashes
added by hand (the first Linux build is the real test). iOS is untouched. One request to crates.io carried the owner's e-mail address
in its User-Agent during the build (disclosed by the agent; nothing else was sent). Questions for the owner: the Android sign-in
route; whether to accept public downloads only until a safe in-app fetch exists; whether to enable Developer Mode on this PC.

## Source: self-hosted Phase 0, web tier configuration (22 October 2026)

P0-1 (hosted assumptions behind configuration) and the web half of P0-2 (configuration schema, startup validation, `*_FILE`
secrets, environment-file generator) are built in source. Nothing is deployed; hosted behaviour is meant to be identical.

**What exists.** One validated, frozen configuration module for the SvelteKit server, read once at start, stopping with one
aggregated message that never contains a secret (typo-like `DYNODOC_*` names are warnings). `DYNODOC_MODE` is `hosted` (default)
or `selfhosted`; the settings are `ORIGIN`, `DYNODOC_ALT_ORIGINS`, `ENGINE_API_URL`, `DYNODOC_BRAND_NAME`, `DYNODOC_ORG_NAME`,
`DYNODOC_ORG_NOTICE` or `DYNODOC_NOTICE_FILE`, `DYNODOC_SUPPORT_URL`, `DYNODOC_TELEMETRY`, `AUTH_EMAIL_LINK` and
`PUBLIC_WORKSPACE_URL` (full table in SELF-HOSTED-DEPLOYMENT-GUIDE.md 7.2). The five web secrets (`SESSION_SECRET`,
`WEB_SERVICE_KEY`, `RESEND_API_KEY`, `GOOGLE_CLIENT_SECRET`, `CONVERTER_KEY`) follow the API's `NAME` / `NAME_FILE` rules
(both set is an error, 16 KiB, UTF-8, trailing line breaks trimmed, group or other access refused on Unix). In `selfhosted` mode
the marketing pages, sitemap, `llms*.txt` and `/source` answer 404, `/` goes to sign-in or the projects page, nothing is indexable,
telemetry is off (no browser request, the route answers 404), the CSP `connect-src` comes from the configured origins, the
organisation notice shows on sign-in and the workspace footer, and email sign-in can be switched off (the route answers 403). A
Node script `dynodoc-env.mjs` generates a commented env file and random secrets (mode 0600, never overwrites, never prints a
secret) and `check-config` validates an existing set without starting the server. `audit-hosted-strings.mjs` scans the build.

**Decision for the owner / release hazard.** A hosted process with no `SESSION_SECRET` and no `WEB_SERVICE_KEY` still starts with
warnings (the Vercel homepage runs without secrets), but a secret that IS set and is short, blank, a repeated pattern or a
documented placeholder now stops the process in both modes. The old rule was length 32, checked on first use. **Before any hosted
web release, run `check-config` against the production environment.** Say if hosted should require the secrets too.

**Evidence (local; GitHub Actions of this repository cannot run, so the changed `ci-frontend.yml` steps are unrun).** vitest 73
files, 743 passed, 4 skipped (Unix file modes, also run as 35 plain assertions on Linux); prettier, eslint, svelte-check (after
`svelte-kit sync`) and the build clean on the merged tree; Chromium on an isolated stack: 21 new specs (selfhosted 12, no-email 2,
hosted-pin 7) and 87 existing hosted specs passed, 2 skipped by design (one workspace spec failed once under load and passed
3 of 3 alone); 95 hosted responses (5 hosts, 18 paths, telemetry) compared before and after: status, headers and head tags identical
apart from asset hashes. Four manual mutations were each caught.

**Not met: acceptance criterion 6** ("no file in the shipped images contains `dynodoc.online`"). 47 files outside third-party
notices still do: 22 compiled marketing pages (unreachable in selfhosted but in the same build), 14 source maps of them, 3 server
files every mode loads (hosted preset, SEO constants, robots body; dormant in selfhosted), and 8 static files (`security.txt`, a
social image, five add-in packages built for the hosted origin, the Google zips). No script that every page loads names a hosted
host or operator fact. Closing it needs an image build that leaves the marketing routes and source maps out (P0-6) and per-
installation add-in packages (P0-7). **Not verified:** CI, Windows secret-file permissions, the Vercel adapter at runtime.
Other open points: the two CLI scripts carry `@ts-nocheck`; `check-config` needs files the web image does not contain (the future
`dynodoctl` or a tools image must ship them); the brand name does not reach logo images, the web manifest or most titles;
`DEV_AUTH=true` in selfhosted is a warning, not an error; a self-hosted Office pane still loads the Office.js CDN (P0-7).

## Source: approvals bound to the revision seen, AI-agent proposals inbox (22 October 2026)

Closes the gap recorded on 20 October ("an approval bound to the revision the reviewer saw") and builds the reviewer side of the
agent write path. **Migration 0045 (schema 45) is new and NOT rehearsed or released**; like 0043 and 0044, rollback is a restore
of the dump, and `grant-runtime` must be run again after it migrates (the review table gained an identity column, so the runtime
role needs USAGE on its sequence; `check-runtime` fails until then).

**Approval binding.** A review names the draft's revision counter it was given for. `revision` is required for `approved` and
`changes_requested` (missing: 400 `revision_required`; the request moved: 409 `request_changed`, nothing written). The counter
only moves forward (a trigger), and the check runs under the document row lock that already serialises every writer; an SQL
experiment confirmed the lock orders a reviewer against a writer in both directions. An approval counts only while
`draft_revision >= content_revision`; a stale approval stays as history, is not counted and needs a new approval. One SQL
(`latest_counted`) serves review state, the merge check, lists and agent status. Every writer that changes draft content bumps
the counter (editor save, pull, rebase, agent propose; a share and a partial merge deliberately do not), and **reopening a
declined or withdrawn request now bumps it too**, so an old approval cannot be revived (the I5 case: a manager declines an agent
request, its principal reopens it, the old approval no longer counts). The audit row `document.change_request_changed` lists the
reviews that lapsed. Same-reviewer ordering uses a sequence, not `created_at` (a transaction-start time). Reviews written before
0045 have no revision and keep the old timestamp rule; the web app was the only caller of the route and sends the revision.

**Proposals.** `GET /projects/{id}/agent-proposals` (account session, members, bounded pages, metadata only; unsent drafts only to
the person the agent acts for) and `GET /projects/{id}/agent/proposals` (an agent key's own list, `content:read`). Adopting an agent
draft now requires the revision too, and the audit records both the person who adopted and the agent that proposed. A principal can
discard an unsent agent draft (closed as withdrawn, kept in history, audited). In the app: an "AI agent" badge wherever an agent
authored a request, an "AI proposals" tab with filters and a stale-base warning, the review dialog showing who proposed and through
which client with the agent's reasons as plain text, Approve sending the revision on screen, and a calm "this changed while you were
reading" reload that never resends by itself. MCP `status` does not use the new list route yet.

Evidence (local; GitHub Actions of this repository cannot run until the account billing is fixed): `reviewer_proposals_test` 21 tests
(stale 409, missing revision, approve-change-merge, partial merge, two reviewers racing, approval versus edit across 14 concurrent
trials, own request, replay on another request, agent never counted with every other lock off, adopt, discard, reopen, pull and rebase
lapse, demoted reviewer, sequence order against 8 concurrent double submits); full workspace tests passed except the known
Windows-only timing test; clippy, fmt and OpenAPI drift clean; vitest 63 files and 614 tests; the Chromium e2e suite 125 passed,
2 skipped of 127 with `AGENT_GATEWAY=on`. An independent adversarial review found no Critical or High issue; its Medium and Low
findings (a withdraw button that could not work, two invariants with no test, reopen reviving approvals, the lapse audit overstating,
`created_at` ordering, an unvalidated upload link) are fixed and each has a test that failed under a manual mutation. **Not
verified:** migration 0045 on a production copy (the column scan takes a short exclusive lock; the NOT VALID check is never
validated by design), real concurrency across two API processes, Firefox, WebKit or phones for the new screens, a screen reader,
the copy by a non-technical administrator, any real MCP client. Questions for the owner: legacy approvals without a revision keep
the timestamp rule (invalidate them all at deploy instead?); adoption keeps the agent as author so the principal can never approve
(move authorship to the person?); should an agent request always need one approver other than the principal; filters by client or
principal and the "formatting inferred" lines are not built; should MCP `status` call the new list route.

## Source: desktop client shell and mobile preparation (21 October 2026)

`apps/desktop` is a Tauri 2 project (crate `dynodoc-desktop`, product name Dynodoc, bundle identifier `online.dynodoc.client`, version taken
from its `Cargo.toml` only) with its own Cargo workspace and lockfile; the root `Cargo.toml` excludes `apps/`, and at the root `cargo
metadata` and `cargo fmt --all --check` still see only the six engine crates. It is a shell: it does not bundle the web app but opens the
configured server (the hosted workspace or a self-hosted origin) in the system web view. A launcher page shipped inside the app (plain HTML,
CSS and JavaScript, strict CSP) lets the person choose the server, shows "This computer's key" and accepts a pasted sign-in link. Security
model, in Rust and tested: a server's pages get no native access at all (the capability files name only the main window's local content and
have no `remote` section; every command also checks that the caller is the launcher); the window may load only the launcher or the configured
origin (the launcher only when the shell asked for it), every other PUBLIC web link opens in the system browser (a page's own requests at most once per 3 s and five a minute, never a mail link or a local address; only a person's trusted click may), everything else is dropped, and `window.open` and `target=_blank` follow
the same rule without ever creating a window; server addresses are validated (https, or http for localhost, 127.0.0.1 and [::1]; no credentials,
path, query, fragment, backslash, percent escape or trailing dot; internationalised names shown as punycode); `dynodoc://open/workspace`,
`.../projects/<uuid>` and `.../documents/<uuid>` are the only accepted links and always open on the configured server, never carrying a token; the
sidecar runs only `engine-cli --version`, `device keygen --out <local data>/device-key.json --encrypt --keystore` and `device public-key <that
file>` with standard input closed, logging off, a time limit (20 s; 90 s for key creation) and a 16 KiB output limit; every web view permission is
denied; one instance; no devtools feature and no shell, fs, http or updater plugin. No tray icon, no auto-update (it needs signing keys), no
analytics. Mobile: mobile entry point, `staticlib`/`cdylib`/`rlib`, a phone capability file without the tool commands, the sidecar declared only in
desktop overlays, icon sets, Android minSdk 24, iOS 15.0, macOS 12.0. Documents: `apps/desktop/README.md`, `DESKTOP-AND-MOBILE-APPS.md`, decision 016.

Evidence, Windows 11 x86-64: 91 unit and configuration tests (55 before the review fixes below) with fmt and clippy clean, including the real `engine-cli` creating a DPAPI-bound
key and refusing to overwrite it; manual mutations of the `@` check, the navigation rule, the sidecar allowlist, a `remote` capability and the CSP
each failed a test (a mutation of a redundant deep-link character check did not, because later checks reject the same inputs). `scripts/smoke.ps1`
(window, config folders, saved server, single-instance deep link, refused links, clean exit 0) and `scripts/security-probe.mjs` (71 checks in four phases, from
inside WebView2: every command and plugin command refused for a server page, real clicks on `target=_blank` and plain external links, dropped
schemes, denied permissions, the launcher's CSP, real key creation) pass; debug NSIS and MSI installers built, never installed; a release build runs,
exits cleanly and ignores the debug-only test-home variable. Linux, in an Ubuntu 24.04 container with a stub sidecar: fmt, clippy `-D warnings`, 90
tests, release build, `.deb`, `.rpm` and AppImage bundles, and the release binary under Xvfb stays up, creates its folders and forwards a second
launch to the first. Android: `cargo check --lib --target aarch64-linux-android` passes. Defects found by running it: a click on `<a
target="_blank">` does nothing in WebView2 (fixed by a small injected script that calls `window.open`); `rust-ini` 0.21.2 and 0.21.3 break
`tauri-plugin-deep-link` 2.6.2 on Linux (the lockfile pins 0.21.1); WebView2 sends the request of a navigation before the shell cancels it
(documented, not fixable here).

**Security review fixes (21 October 2026, after an independent review of the branch at 50b1afd; no Critical or High findings; main was merged first and both ledger entries kept).** Page-initiated hand-offs to the system browser are now limited (`handoff.rs`): a page may open only a PUBLIC `http(s)` address, at most once per 3 s and five per minute, never a mail link or a loopback, private, link-local, mapped, NAT64 or `.local`-style address; only a person's trusted click (the injected script passes a marker that carries a start-up secret) or a menu item may. The launcher loads only for a navigation the shell started (one-shot grant), and the public key is read only when asked, never on page load. Key creation and reading use the long key-store limit; a cut-off or damaged key file gets "delete this file in <path> and create the key again" and is never overwritten. The window title names the server; the saved address is sealed (SHA-256 in the local data folder) and an edited config is shown for confirmation, not opened. The cold-start path uses `try_state` as a guard (Round 2 removed the early-link queue: the deep-link plugin keeps the latest link and the shell reads it at the end of start-up). `about:blank` is no longer special, debug logs redact queries and the click secret, `withGlobalTauri` is off. Documented, not fixable here: a `dynodoc://` link navigates the window (checked: WebView2 runs the page's `beforeunload` and shows its leave-page dialog), and a pasted sign-in link signs in whichever account issued it (the web `/login` page still needs a click; showing the account email is server work). The workflow uses `--locked` everywhere, notes that tag runs need a protected GitHub environment before any signing secret, and runs `scripts/harden-android.mjs` after `tauri android init` (Tauri's template has no `allowBackup`, which Android defaults to true, and enables cleartext in debug; the script fixes both and fails if either is left true; run here against a generated project). Evidence: 91 tests (90 on Linux); the probe now has four phases (hostile server page including a 40-link flood that gets one hand-off, an edited config, a launcher that does not read the key, a damaged key file), 71 checks, and passes also with `--unicode-home`; `smoke.ps1` checks the title, the edited config and `engine-cli device keygen --encrypt --keystore` in folders with an accented letter, Bengali letters and both, using an `engine-cli` built after the Windows non-ASCII fix (not compared with an unfixed binary); the Linux container build was repeated after the fixes (fmt, clippy `--locked`, tests, release build, `.deb`/`.rpm`/AppImage). Mutations that each now fail a test: the second lock always passing or removed from a command, the permission handler allowing, the hand-off scheme filter removed, `.devtools(true)`, the limiter gap 0, private ranges not refused, the marker secret always matching, the launcher always allowed, an unsealed config trusted, and an extra deep-link shape (the whitelist test also catches shapes no list mentions). One probe run hung once with the app process gone mid-run and no crash event; three later runs passed, and the probe now fails after 60 s instead of waiting.

**Round 2 of the review fixes (21 October 2026, re-review of f4e6872 by the independent reviewer).** (1) The injected click script no longer calls anything a page can replace: every built-in it uses is captured at injection (uncurried calls, brand-checked getters, no `closest`, `URL`, regular expressions or array methods), the secret sits in one closure variable and comes from the operating system's random source (`getrandom`); it is per run, not rotated per page load (the script text is registered once). `scripts/link-targets.test.mjs` runs the real script in a Node realm against a hostile page (12 tests; two controls run the previous script and must see it leak), and the probe has a hostile-prototype page (call/apply/bind, Reflect.apply, closest, link and event getters, encodeURIComponent, URL, window.open). Clicks have their own hand-off budget (1 per second, 10 per minute) apart from the page's (1 per 3 s, 5 per minute); menu items have none; neither bucket can spend the other's. The README no longer says the secret is 'known only to the injected script' without saying what that rests on, and says what a real click on a page-controlled link can still do. (2) `public_host` is documented as spelling only (DNS rebinding and any private-pointing name stay invisible); a best-effort denylist refuses 11 wildcard-DNS services and names spelling an IPv4 address. (3) WebKitGTK/WKWebView (and perhaps Android) report frame navigations without saying which is a frame: `about:srcdoc`, `about:blank`, `data:` and the server's `blob:` are let through there and never take the launcher grant, which now also requires the launcher's own page (`/` or `/index.html`); stated as unverified on WebKit and Android. (4) The Android workflow reads the merged manifest of the built APK (`scripts/check-apk-manifest.mjs`, aapt2 or apkanalyzer) and fails on allowBackup or cleartext not explicitly false, testOnly, a network security config, or debuggable in a release build; `harden-android.mjs` also rejects `usesCleartextTraffic` set true in a build file and a network security config that does not switch cleartext off. The checker's parsers are tested on sample output only; real aapt2 output and a Gradle build are unverified. (5) Dead code removed (the early-link queue), deep links come from one static table with a by-hand copy test and a test that `parse` holds no string literal. Evidence: 97 desktop tests; 28 node tests (12 + 8 + 8); `smoke.ps1` 29 checks; probe 79 checks, with and without `--unicode-home`, on an idle machine (four runs on a loaded machine, about 0.4 GB free, failed one or two timing-dependent checks each and one lost the app process in phase B; the cause of the latter and of one leave-page failure is not found); root `cargo fmt --all --check` and `clippy --workspace --locked` clean; Android `cargo check --lib` clean. Mutations that each fail a test: the previous injected script (3 probe checks fail), `window.open` called directly, `link.href` read directly, `.call` on a built-in, `closest`, the trusted-click check dropped, an `["open","settings"]` row in the table, an inline literal path in `parse`, and in the hand-off and navigation rules a mail link for a page, an unlimited click budget, clicks spending the page budget, the wildcard-DNS list unused, the secret always matching, any host for a page, the grant opening any launcher-origin page, frame-like addresses never or always let through. Not done: the Linux container run (the volumes were removed and a fresh build was not cheap), any WebKit or Android run, any GitHub Actions job.

**Not verified:** no GitHub Actions job has run. The account's Actions refuse to start any job ("recent account payments have failed or your
spending limit needs to be increased"), and `workflow_dispatch` cannot start a workflow that is not on the default branch, so `desktop.yml`
(manual or tag `desktop-v*` only, never on push) has never executed; it parses and GitHub lists its six jobs. Also not verified: macOS and iOS
builds (the Apple targets cannot be checked on Windows), the Android Gradle build and `tauri ios init` (`tauri android init` was run here with stand-in SDK folders), the real `engine-cli` on
Linux, key creation through the macOS Keychain or the Linux Secret Service, the installers installed, downloads and printing in any web view,
Android and iOS web view behaviour, physical devices. Nothing is signed or notarised. Questions for the owner: fix the Actions billing; confirm
`online.dynodoc.client`; keep the paste-a-sign-in-link workaround or build the browser-to-app hand-over first (server work); allow clipboard
read for the web app's origin; the Windows signing route; try the Arm runners.

## Source: browser and device compatibility, installable web app (21 October 2026)

Second slice of decision 016. The researcher app now has a browser matrix in Playwright (Chromium, Firefox, WebKit, a Pixel-class
and an iPhone-class emulation; the extra projects are opt-in through `BROWSER_MATRIX`), a new `browsers` job in `ci-frontend.yml`
that runs a 14-test smoke subset on those four (estimated 10 to 12 runner minutes warm, about 20 cold, in parallel; not measured on
CI), and an installable web app: a complete `site.webmanifest` (id, scope, standalone, shortcuts, 192, 512 and maskable 512
icons), an offline page, a service worker, an "Install app" entry in the profile, and iOS and Android head tags. Support, install
steps per system and limits are in [BROWSER-SUPPORT.md](BROWSER-SUPPORT.md).

Service worker, in five rules: it precaches only the offline page and two icons; it stores only a 200, same-origin, public,
non-HTML file under `/_app/immutable` or `/fonts` (never a `Set-Cookie`, `private` or `no-store` reply); it never answers a
non-GET request, `/api`, `/auth`, `/admin`, the Office panes, `/view`, `/print`, telemetry, event streams, range or authorization
requests (the browser handles them as if no worker existed); no page is ever stored, and only a missing network gets the offline
page; caches are versioned and cleaned on activate, and an update waits for the person to press Update now, through the existing
leave-page warning, so an editor is never swapped under unsaved work. **One policy line added:** `manifest-src 'self'` in the
SvelteKit CSP (without it the page's `default-src 'none'` blocked the manifest on every engine, so the app could never install);
the `worker-src` line is unchanged. The demo nginx config gained entries so the worker script and offline page get a policy that
lets the worker fetch the app's own files; without them the worker does not install and the site keeps working. Any other front
end (Vercel) needs the same rule.

Defects found by running the other engines and phones, and fixed: on phones the comments, version history and help were hidden
below 700 px with no other way in, the bell overlapped command icons, seven dialogs ran off the right edge, 14 to 20 fields were
under 16 px (iOS zooms the page on focus), controls were 22 to 36 px, the ribbon heights were fixed; a spreadsheet cell could not
be edited on iOS (no double-click on touch); 27 `100vh` uses got a `100dvh` partner; copy falls back to `execCommand`; in WebKit
and Firefox a remote update replaced the whole document and threw the caret to the end, so typing split across paragraphs
(fixed with a selection map, 7 unit tests). A new touch layer gives 16 px fields and 44 px controls on coarse pointers only.

Evidence (Windows host; Chromium 148, Firefox 150, WebKit 26.4): gates pass on the merged tree (prettier, eslint, svelte-check
0/0, vitest 60 files and 580 tests, production build with the worker and manifest in the output). Playwright: Chromium full 121
passed, 2 skipped of 123 (baseline was 108 of 108); Firefox full 117 passed, 3 failed that pass alone, 3 skipped; WebKit full per
file, all pass except two; Pixel 46 passed; iPhone 45 passed, 1 skipped. New: `pwa.spec` 10 tests and `browsers.spec` 5, green on
all five projects; behind an nginx 1.27 container the 8 worker tests passed. **Not fixed:** WebKit `changes.spec` "teammates
push" (multi-user editing) fails every run at varying steps, unexplained beyond the caret bug; WebKit `office.spec` "live pages
reflow" fails by a line 1/16 px above a page boundary. **Not verified:** any real device, Safari itself, WKWebView, Android
WebView, the operating systems' install flows, the iOS keyboard, zoom and notch behaviour, the `-webkit-` and `dvh` additions
(inspection only), Linux WebKit, and `check-proxy-headers.mjs` (needs Linux). Playwright emulation is not a device. Documented
limits: the phone toolbar plus ribbon leave about two sheet rows; spreadsheet editing on a phone is read, select and formula bar
or double tap; presentation editing on a phone was not exercised; Firefox desktop cannot install. Questions for the owner: accept
`manifest-src`; a host-specific manifest (the start URL `/workspace?source=pwa` suits the app host and redirects off-scope from
the marketing host); a compact phone toolbar; a real-device pass; real screenshots for the manifest.

## Source: processor and operating-system parity, prebuilt command-line tools (21 October 2026)

First slice of decision 016 (devices, processors, desktop and mobile apps). The public engine repository now runs
`platforms.yml` (push to main, pull requests, manual) on Linux aarch64, macOS aarch64, macOS x86-64, Windows x86-64 and Windows
aarch64 (tier 2, non-blocking), next to the existing Linux x86-64 CI. Each job builds the workspace and runs the whole test suite
against PostgreSQL 16 (a service container on Linux, Homebrew on macOS, the image's PostgreSQL 17 on Windows x86-64, the pinned
EDB 16 zip under emulation on Windows aarch64); clippy with `-D warnings` runs on macOS aarch64 and Windows x86-64 because of
`cfg` code. `release-cli.yml` (tag `cli-v*` or manual) builds `engine-cli` and `engine-api` natively for six targets and packs
tar.gz/zip archives with `SHA256SUMS`, unsigned. The support matrix, what CI proves per platform and the limits are in
[PLATFORM-SUPPORT.md](PLATFORM-SUPPORT.md).

Evidence (engine run 39493540615, all six jobs green): 1,285 tests passed on Linux aarch64, macOS aarch64, Windows x86-64 and
Windows aarch64, 1,284 on macOS x86-64 (the timing test below excluded), 4 ignored everywhere. The three key-store providers were
run for real: Secret Service through a private gnome-keyring on Linux x86-64 and aarch64, a temporary keychain on both macOS
architectures (the login keychain is checked to stay clean), DPAPI on both Windows architectures. Release run 39493540654: all
six targets built, each ran `--version`/`--help` and an `engine-api` no-config refusal, then again from the unpacked archive.
Archives were also downloaded and run here for Windows x86-64, Linux x86-64 (ubuntu:22.04 container) and Linux aarch64 (QEMU).
Linux builds target glibc 2.34, macOS 11.0, Windows uses the static CRT.

Real defects found by running on the other platforms and fixed (ported to this repository; same files in both): on Windows the
key-file access check failed closed for any folder or user name with letters outside the console code page (icacls prints `?`);
an Administrator process left explicit SYSTEM and Administrators entries on key and token files, so 12 tests failed on the runner.
`private_file.rs` now compares non-ASCII names by their ASCII letters and the positions of the others, and removes those two
entries by well-known SID before granting the user. **Known limit of the first fix:** two different local accounts whose names
differ only in non-ASCII letters at the same positions cannot be told apart by that check; an ASCII name is still compared exactly,
and several entries or none still fail closed. Not applicable here: the `integrations/**` line-ending rule (this repository already
normalises to LF for everything).

**Not verified:** the tag path (`gh release create`) never ran, because it would publish in the public repository; a manual run
of a workflow that is not on the default branch cannot be started, so the release workflow became dispatchable only at the merge.
The macOS and Windows aarch64 archives ran only inside their own build jobs. A hidden passphrase prompt needs a terminal and was
not tested. The test `sse_delivers_a_new_event_within_100ms` (a 100 ms bound, not weakened) is excluded from the blocking run on
Windows x86-64 (failed 3 of 3), Windows aarch64 (2 of 2) and macOS x86-64 (flaky, 1 in 5) and recorded in its own non-blocking step;
it passed on every Linux aarch64 and macOS aarch64 run and still blocks there. Questions for the owner: test the tag path once
with a throwaway tag and delete that release; use the pinned PostgreSQL 16 on Windows x86-64 too; rework the SSE test to a
CI-tolerant bound or keep the exclusion; keep the glibc 2.34 floor (ubuntu-22.04 builders will be retired eventually). A multi-architecture container image
(linux/amd64 and linux/arm64, each built natively and started on its own processor; engine run 39531119708) was added
afterwards; its tag-only publish path to GHCR has not run.

## Source: MCP write tools, connector Phase 1 client side (20 October 2026)

The MCP server gains `propose`, `submit` (also `action=withdraw`) and `status` for the proposals of the merged write path, as an
**opt-in**: `engine-cli mcp --write` or `DYNODOC_WRITE=on` (the plugin's `.mcp.json` passes it as off by default). Without it the
server asks for the read scope only, lists the six read-only tools (their definitions are pinned byte for byte by a golden file,
4,983 bytes) and refuses a call to a write tool. With it the session asks for read, propose and submit together, falling back to
read if the service refuses the set. The eight-tool profile is 8,580 bytes of definitions per turn (about 3.6 KB more than the
read profile). Every propose, submit and status answer says the change is a proposal awaiting review and that nothing is live;
`awaiting_person` says a person must submit; stale refs are listed with the instruction to read again. A fresh `request_id` is
made per logical call and kept across the 401 renewal and the single resend after a dropped connection, so a retried propose is
idempotent. More than 200 operations or a 1 MiB body is refused before any request; unknown fields are rejected at any depth. The
Claude Code plugin skill (now 0.2.0) teaches when to propose and when only to read, the compact operations with examples, dry run
first for bulk edits, re-reading after a stale base, and never saying a change is merged.

Evidence: about 36 new `engine-mcp` tests (definitions, contract against the spec's section 16 transcripts where the spec file is
present, mock-server cases for success, dry run, stale base, idempotent retry, 429, policy refusals, token never in a log or result,
nothing called unprompted) and about 12 new `engine-cli` tests including a real-process write session; three manual mutations each
failed their test. **Not verified:** no real MCP client or model has used these tools, the connector has not run against the
real `engine-api` (the tests use stand-ins written from the route code), and the plugin's `${VAR:-default}` expansion in Claude
Code is unchecked. Questions for the owner: is 8.6 KB of definitions per turn acceptable or should the write tools be deferred or
trimmed; split `--write` into propose-only and propose-plus-submit; whether the model may pass `request_id`; a "list my proposals"
route so `status what=proposals` can exist; amend the protocol spec with the listed deviations.

## Source: AI agent screens in the web app (20 October 2026)

Two screens now sit on the merged agent APIs; nothing on the server changed. **Signing keys:** enrolment asks "Who is this key for?"
(me, by default, or an AI agent with a required name); an agent key shows an "AI agent" badge and one permission, "Read documents",
which it can be given per project (no person scopes, no all-projects mode, mixed classes refused client-side and mapped from the
server's errors). **Project settings, "AI agents":** a project owner or manager sees whether agents are available on this
installation, turns them on or off, sets the role floor, the kinds and the two budgets (saved as the server's full replacement with
the loaded revision; a conflict reloads the form), reads the last seven days of usage, sees the per-file overrides (read-only), and
can press "Stop all AI agents now", which asks first and says exactly what it does (agents off, every agent key's access to this
project and every open agent session ended, people's own keys untouched, turning agents on again restores nothing). Anyone else
sees one explanatory line.

A real defect was found and fixed on the way: the same-origin proxy (`api/engine/[...path]`) did not forward `If-Match`, so every
browser save of the policy would have been refused with a 400; it now does, with a test.

Evidence: 540 unit and component tests in 55 files, Prettier, ESLint, svelte-check (0 errors, 0 warnings) and a production build
pass; the new Playwright spec (enrol as an agent and grant, save, a conflict from a second save, the kill switch, an editor seeing no
controls, an axe scan of both sections with no violations) passed with the existing key and project specs, 8 of 8, on a local copy of
the CI isolated stack. **Not verified:** the stack ran with the agent gateway off (the "turned on" wording is covered by unit tests only),
no real AI client, only Chromium, no screen reader, no review of the copy by a non-technical administrator. Not built: file override
editing, a date range for usage, and the write side of agents (reviewer badges and the proposals inbox belong with the write path).
Questions for the owner: whether members below manager may read the policy (today they are refused, so they cannot see even a
read-only state); whether an agent key's name should be required (the server treats it as optional); the role names in the floor
select (the screen uses Viewer, Contributor, Reviewer, Editor, Manager, Owner).

## Source: at-rest envelope encryption library, Phase 0 core (20 October 2026)

The pure core of Phase 0 of [END-TO-END-ENCRYPTION.md](END-TO-END-ENCRYPTION.md) section 7 is built as `engine_core::at_rest`:
the `DYNOENC` stored form (AES-256-GCM, a per-value key and nonce from HKDF-SHA-256 over the project data key and a
per-value salt, an authenticated header), a `KeyProvider` trait with a file provider (KEK versions in `kek-<n>.key` files,
rotation by adding a version, owner-only check on Unix), a deterministic test provider and a `DekCache` with a TTL and size
bound. **Nothing uses it**: no `EnvelopeStore`, no `data_key` tables, no migration, no backfill and no storage path changed.
This is service-decryptable at-rest protection (claims level 1); it is not end-to-end encryption and the documentation says so
wherever it names it. [AT-REST-ENVELOPE.md](AT-REST-ENVELOPE.md) records the format, the hierarchy and what it does and does not
protect.

Refinements of the design, made before anything is stored: the AAD and salt are length-framed (the design's plain concatenation is
ambiguous: table `ab` + column `c` equals table `a` + column `bc`); the header version, data-key id and KEK version are
authenticated; and, from review, the deterministic salt for content-addressed tables also binds `SHA-256(plaintext)`, so
sealing different bytes under an address that was not verified gives a different key and nonce instead of repeating them
(equal plaintexts still give equal envelopes, which the store's "same address, same bytes" check needs).

Evidence: 68 tests (48 unit, 13 property with fixed seeds including arbitrary bytes into the parser, 7 vector) plus Clippy and
rustdoc with warnings denied, and the Unix permission test in a Linux container. Vectors (13 envelopes, deterministic-salt
envelopes, wrapped DEKs, rejects, KEK files) come from an independent Python generator using the `cryptography` package and
checked against RFC 5869 and GCM known answers; the Rust code reproduces every byte, and breaking the salt label or the AAD
fails the vector tests. No external cryptographic review has been done and the vectors share the author's reading of the design.

Not built: the Vault and cloud-KMS providers (the trait is synchronous; a network provider needs an async form), the storage
wrapper and its migrations, backfill (the design's M-a and M-c), key rotation jobs, crypto-erase integration, tier 0c.
Owner questions: add a ciphertext length field to the header now (today a cut or extended value fails authentication instead of
parsing as a distinct error); keep the authenticated but unused `kek_version` header field; accept the 256 MiB plaintext and
name limits; accept plain-hex KEK files (a passphrase-wrapped variant is listed in the design).
## Source: agent review-gated write path, connector Phase 1 server side (20 October 2026)

An AI agent can now **propose** changes on the server, and nothing it does reaches the team version. Migration
`0044_agent_write.sql` (schema 44) adds the `content:propose` and `content:submit` scopes (each held beside `content:read`, which
a preview needs; neither implies the other), `agent_identity` (the identity row under which an agent's drafts are recorded: born disabled, address in the
reserved `agents.invalid` domain, one live per person and client product, revoked with the person, never a member, reviewer or
key holder), `agent_proposal_receipt` (append-only: counts, stale refs, the agent's own reasons, request digest; no document
text), `device_key_class.client_kind`, and the write fields of the project policy: `write_mode` (**`none` by default**, then
`draft` or `change_request`), `submit_requires_person` (true), `proposals_per_day` (20), `open_proposals` (5) and
`ops_per_proposal` (500); the kill switch sets `write_mode` back to `none`. Routes under `/projects/{id}/agent/` (mounted with
the others under `AGENT_GATEWAY=on`): `POST files/{file}/propose` (the spec's name is `.../proposals`; a Phase 0 test pins that
one as absent), `POST proposals/{id}/submit`, `POST proposals/{id}/withdraw`, `GET proposals/{id}`. A proposal is a
`workspace_draft` created by the agent identity with the compiler's output stored as `draft_edit` rows; **no route of this
slice appends to the event log** (the gate test compares row counts and chain heads). The person an agent acts for sees, edits,
submits and withdraws those drafts through the existing routes (seven consumers accept the agent's principal; `share` into the team
log is refused for an agent's draft), nobody else sees an
unsubmitted agent draft, a request an agent wrote is `mine` for its principal so that **the principal cannot approve it (I5)**
and an approval forced in some other way is not counted, the agent identity is refused as an approver (I4) by the database and by
an explicit check, and requests carry an additive `agent` object. The merge audit row names the agent as contributor (layer B);
the events keep the merger as actor (layer C, sealed attribution, is not built). Evidence: `tests/agent_write_test.rs` (32 tests
under the restricted runtime login: gate, authority re-read per request, ownership and uniform answers, route table, I5 with one
other reviewer and with none, stale bases, idempotency with concurrent duplicates, budgets and kill switch, hostile text,
identity, disable/erase/document-erasure cascade, a seeded differential test of the route against `engine_agent::compile`,
OpenAPI conformance) plus unit tests; controls were removed one at a time to see their tests fail (listed in
[AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md) section 11.3). An independent adversarial review found no Critical or High issue;
its fixes (no `share` of an agent's draft, bounded write locks and a lock-free dry run, a write scope only beside the read scope,
no bypass of the approvals by the principal, per-person budgets, a file that moved while a write waited, the reserved-domain test
and migration 0044's checks, whole proposal ids in DCT, audit attribution) are in and tested. **Not built:** MCP write tools, the app (no screen: badges,
inbox, adopt action, policy tab), OAuth, layer C, the evals, macros, `agent.denied` audit rows, `diff proposal=`, DLP.
**Not run** against a real client, under load, or on a production copy; migration 0044 has not been rehearsed or released.
Also not built: an approval bound to the revision the reviewer saw (a contributor who changes a request between a reviewer's page
load and click gets the approval counted; to be closed with the reviewer screens).
Raised for the owner: the approvals a file requires apply to an agent's request as configured, zero included (the project
default), and its principal cannot bypass them (`override_rules` is refused for them, another manager's is not) - say if agent
requests should always need one other approver, which would leave a one-person project unable to merge its agent's work; the
proposal id is server-chosen, not the first `request_id`; the proposal budgets are per person and project over all client
products; the budget defaults are the spec's numbers and apply only once `write_mode` is set.

## Source: agent project policy v0 and management routes, connector Phase 1 (20 October 2026)

The main open risk of the Phase 0 read routes is closed in source: **agents are disabled until a project owner enables them.**
Migration `0043_agent_policy.sql` (schema 43) adds `project_agent_policy` (no row means disabled; `enabled` defaults to false;
`min_role` floor, `kinds`, `calls_per_minute`, `read_bytes_per_day`, `revision`, and `sessions_valid_after` for the kill
switch) and `file_agent_policy` (`inherit`, `none`, `read`). Both are current-value tables in the runtime select/insert/update
class (no delete); history is the `operation_audit` row of every change. With `AGENT_GATEWAY=on`, every agent read route and
the access view is refused with the same `403` as a non-member unless the project is enabled, the member's role is at or above
the floor and the grant has room in its per-minute and per-day budgets (counted from the call ledger; `429` otherwise); a file
set to `none`, or of a kind the policy leaves out, is the same `404` as an absent file everywhere, including `files`, `find` and
the 8-hex lookup. The policy is read on every request. This supersedes the warning below that any member, even a viewer, can
read once the gateway is on.

Management routes (account session only, `401` for any other token, mounted whether or not the gateway is on):
`GET`/`PUT /projects/{id}/agent-policy` (owner or manager, `If-Match` revision, `409` on conflict), `POST
/projects/{id}/agent-policy/kill` (disables, revokes every agent-class grant of the project, cuts off every earlier session,
purges the process's cached states, audited), `GET /projects/{id}/agent-usage` (calls, bytes, denials by UTC day and client
claim), `PUT /documents/{id}/agent-policy` (file owner or manager). The web app has no screen for any of it, token issuance is
not refused for a disabled project, and the write-side policy fields (`write_mode`, clients, proposal budgets, DLP, egress,
retention) are not built. Evidence: `tests/agent_policy_test.rs` (22 tests under the restricted runtime login) and the unchanged
18 read-route tests, whose setup now enables agents. **Not run** against a real client or under load. Raised for the owner: the
role field is a floor (`min_role`), not the ceiling the brief named, because every role may read; the caller is told `forbidden`,
not a distinct `policy_denied`; whether token issuance and key grants should also be refused while agents are disabled; the
file value is the spec's `none`, not `deny`. An independent security review the same day led to: refusal ledger rows throttled to
one a minute per grant and outcome, file overrides loosenable only by a project owner or manager, a budget query that no longer
scans the day, a cut-off trigger that also refuses clearing it, a per-person admission bucket for the management routes, `PUT`
keeping omitted fields (and requiring `enabled`), bounded usage dates and a strict session cut-off.

## Source: agent operation compiler, connector Phase 1 library (20 October 2026)

The pure core of connector Phase 1 is built in `engine-agent`: a strict parser for the operation grammar of
[AI-AGENT-PROTOCOL-SPEC.md](AI-AGENT-PROTOCOL-SPEC.md) section 8, `compile` (resolves refs and sub-addresses, applies
the stale-base rules of section 9, turns text edits into minimal rich-text patches that keep marks, makes stable ids and position
keys, and validates every produced operation with `ops::validate_op` and `governance::authorize` against the state as it
evolves, so what comes out always applies), `preview` (a bounded dry-run DCT diff) and a Rust port of the editor's position-key
function `between()`. Operations compiled: `set_text`, `replace`, `replace_all`, `insert`, `delete`, `move`, `set_type`,
`set_cells`, `clear`, `set_shape`, `set_notes`. Output equals what `edit_draft` stores. **It stores nothing and nothing calls it
yet**: no propose route, agent identity, draft, receipt, idempotency, submit, review badge, policy or MCP tool exists (the rest of
Phase 1). Not compiled (reported as `unsupported_op`): `fill`, adding or deleting slides, deleting a sheet or any container,
macros, and `set_text` with `fmt=md`.

Evidence: 93 new tests (183 in the crate): grammar and limits, per-operation behaviour, a differential suite that applies compiled
operations with `apply_payload` and compares text and structure to an independent reference model over deterministic random edit
scripts (a one-off scale run covered 1,500 document and 1,000 sheet scripts), hostile-input tests, previews bounded by the byte
budget, and position-key properties. The `between()` vectors (10,000 pairs plus five 1,600-step chains) were produced by running the
real TypeScript function (TypeScript 5.9.3 transpiling `changes.ts` under Node 24) with a committed generator; the Rust port reproduces
every one, and a test fails if `changes.ts` changes. The TypeScript original has two quirks, mirrored and guarded: a key pair can
sort at or above its right neighbour, and after 200 positions the fallback grows one character per call at the end of a list, so
`key_between` refuses with `limit_exceeded` after about 1,000 inserts at one spot.

Never run against `edit_draft`, a database or a change-request merge, and no model has produced operations; no token saving is
measured. Raised for the owner: how `$n` handles several blocks created by one `insert`; whether `replace` is case-sensitive by
default (the compiler: yes; `find`: no); the style of inserted replacement text; rejecting versus flagging hidden characters in
agent writes (the compiler rejects); the missing `cycle` error in the section 10 table; the grammar for `fmt=md`.

## Source: self-hosted Phase 0, API retention job and secret files (20 October 2026)

Two API-side tasks of [SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md) Phase 0 are built: **P0-8**, the retention job, and the
API half of **P0-2**, `*_FILE` secret loading with startup validation. Nothing else in Phase 0 exists (web tier parametrisation,
SMTP, the bundle, `dynodoctl`, image pipeline, add-in manifest generator, air-gap harness).

**Retention job** (`engine-api/src/retention.rs`). Every API process runs a loop; each pass takes a PostgreSQL session advisory
lock on its own connection, so one replica prunes and the others skip (a crashed holder releases it by closing the connection).
It deletes in bounded batches (`for update skip locked`, a pause between batches, a time budget per pass), one table at a time,
and a failing table does not stop the rest. It prunes only expired rows: magic links, refresh tokens (expired or revoked beyond
the window), request-rate windows, device enrollment challenges, `web_metric` and idempotency keys. It never touches a history,
event, snapshot, audit or erasure table, and needs no migration (the runtime role already holds delete on exactly these tables;
a test asserts it). Settings: `RETENTION_JOBS` (on), `RETENTION_INTERVAL_SECONDS` (3600), `RETENTION_BATCH_ROWS` (1000),
`RETENTION_AUTH_DAYS` (7), `RETENTION_METRICS_DAYS` (30); bad values stop startup. Per-process counters appear under `retention`
in `GET /admin/overview`. Rotated but unexpired refresh rows are kept, because `revoke_login` still accepts such a secret to end
its login.

**Secret files and validation** (`secrets.rs`, `config.rs`). `DATABASE_URL`, `PASETO_LOCAL_KEY` and `WEB_SERVICE_KEY` accept
`NAME_FILE` (regular file, at most 16 KiB, trailing newline trimmed; setting both is an error; on Unix a group- or
world-readable file is refused unless `DYNODOC_ALLOW_OPEN_SECRET_FILES=true`, which exists because Docker and Kubernetes secret
mounts are 0444 or 0644). Startup validates every secret and setting and reports all problems in one error; errors name the
variable and never print a value. Plain variables behave byte for byte as before, so the live environment file is unaffected.

Evidence: 22 unit tests, 8 database tests under the restricted runtime login (expired versus live rows of every kind, batch
bounds, a row-count diff of every other table, two concurrent passes giving one run and one skip, master switch, survival of a
missing table), startup tests, and the Unix permission checks run in a Linux container.

**Before the next production release:** retention is on by default, so the first pass after deployment will delete the whole
backlog of expired authentication rows in batches (rehearse it on the production copy, or release with `RETENTION_JOBS=off` and
switch it on afterwards). Owner questions: keep that default; keep rotated-but-unexpired refresh rows; keep the open-secret-file
escape hatch or require mode 0400; whether the counters belong in `/admin/overview` or in a future `/metrics`. Not done: the web
tier's `_FILE` loading and eager `SESSION_SECRET` check, `_FILE` in `engine-cli`, the env-file generator, passing the new
variables through `infra/demo/compose.yml`, and removing the host script's own `web_metric` prune.

## Source: agent connector Phase 0, read-only server, MCP and Claude Code plugin (20 October 2026)

Connector Phase 0 ([AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md)) now exists end to end in source, read-only and **off by
default**.

**Server (schema 42 source).** Migration `0042_agent_read.sql` adds the `content:read` scope to device sessions and grants,
`device_key_class` (one immutable row per key enrolled as an agent) and the append-only `agent_call` metadata ledger (no
text, no tokens; rows leave with an audited document erasure, or after 90 days by the owner role; a request that never
resolved a file is rate-limited but not recorded). `AgentAuth` accepts only a project device session from an agent-class
key with an active `content:read` grant for that project, re-reading everything per request. Routes under
`/projects/{id}/agent/` (files, outline, blocks, `blocks:read`, cells, sheet profile, slides, find, diff, access) and
`/.well-known/dynodoc-agent` are mounted only when `AGENT_GATEWAY=on`; off, they and the OpenAPI entries are absent. They call
the `engine-agent` library, answer JSON by default and Dynodoc Compact Text on request, honour ETag and `If-None-Match` after
authorisation, and verify the event chain exactly as the other read routes do. A bounded state cache (64 states, 32 MiB,
5-minute expiry, purged on erasure) is keyed by document, revision and chain hash.

**MCP and plugin.** The new `engine-mcp` library is a hand-written MCP server over stdio (six read-only tools, no new
third-party crate) and `engine-cli mcp --remote … --project … --device-key …` runs it, getting a project device session
through the existing challenge protocol. The Claude Code plugin is in `integrations/claude-code/dynodoc/` and the setup guide
in [AI-AGENT-CLAUDE-SETUP.md](AI-AGENT-CLAUDE-SETUP.md).

**Review.** An independent read-only security review of the routes found no Critical or High issue. Fixed from it: the
ledger could be filled by guessing file ids and could not be pruned; repeated refs multiplied rendering work before the byte
budget applied; a disconnected client freed its admission slot while its work ran; the public OpenAPI document advertised the
gateway while off; erased text could linger in a process cache; request spans logged search text. Left open and recorded: the
file listing does per-file role work in Rust, agent reads share the per-project budget with people, class rules for keys are
enforced in code rather than by constraints, and permissive CORS also covers these routes (bearer-only, no cookies).

**Not built, and what to know before using it.** Propose, submit and status for proposals (Phase 1), OAuth and remote MCP
(Phase 2), ChatGPT and other clients. **There is no project or file agent policy yet**: with the gateway on, any project
member, even a viewer, can enrol an agent key and read what they can already read, so keep `AGENT_GATEWAY` off outside a
staging stack until a default-deny policy exists. The Signing keys screen has no "use for an AI agent" option (the API
accepts the class). Nothing was run against a real MCP client or a production-copy database, and migration 0042 has not been
rehearsed or released: it needs a schema-42 release by the usual routine. Vendor-specific plugin facts are marked unverified
in the guide.

## Source: horizon record specification and read-only planner, pruning P0 and P1 (20 October 2026)

Phases P0 and P1 of [DESIGN-COMMIT-PRUNING.md](DESIGN-COMMIT-PRUNING.md) are built; **nothing deletes,
signs or changes a stored format**. [HORIZON-RECORD-V1.md](HORIZON-RECORD-V1.md) specifies the record
(canonical JSON, address, signature domain, Merkle roots, invariants H1 to H10), the retention
policy schema `dynodoc.retention-policy.v1` and the planner, with every decision taken. In
`engine-core`: `repository/horizon.rs` (record types, hashing, the H-checks), `horizon/policy.rs`
(strict JSON loader that lists every problem with a JSON pointer and a code) and `horizon/plan.rs`
(the planner: heads, holds, pins, all merge bases including criss-cross, a recent window by count
and by time, one-level v2 and v3 reference closure, both tiers, object accounting).
`LocalRepository::horizon_plan` runs it on a read-only connection in one snapshot, and
`engine-cli repository DB PROJECT horizon-plan --policy FILE [--list N] [--as-of-ms MS] [--out FILE]
[--format json|text]` prints the canonical plan (never overwrites `--out`). The plan names the
reference instant explicitly instead of reading a clock.

Evidence: 21 unit tests (each H-rule has a negative test that fails for that rule only), 7 SQLite
tests (merge bases equal the repository's own for every commit pair; database and write-ahead log
byte-identical before and after planning), 2 CLI tests, 5 property tests at fixed seeds over linear,
branchy, criss-cross and v2 and v3-heavy histories (heads, window and pin retention, closure,
merge-base equivalence, hash stability across insertion orders, idempotent re-planning); breaking
the planner nine different ways was caught every time. Vectors (36 Merkle roots, records,
envelopes, 4 valid and 31 invalid policies, 10 planner scenarios) come from an independent Python
generator with its own planner and are reproduced byte for byte. The pure planner took 12.7 s for
100,000 in-memory commits in a debug build; the end-to-end 60 s target is not measured.

Not done: everything from P2 (local schema 4, applier, skeleton-aware import and merge bases,
verify, archive, hold storage, signing flows, sync, the shared service). Policy files are JSON only
(no YAML parser). The plan hash was reproduced on Windows only; the Linux CI run of the vector test is its first cross-platform check.

Raised for the owner: merge bases computed on retained bodies alone differ from the full graph even
when the base is retained and agree only once skeleton rows are kept (as the design says for
topology drop); whether skeleton and horizon rows count against the local SQLite quotas; where local
holds are stored; whether the v2 and v3 closure should be recursive (it is one level today); the
local role vocabulary (owner and device locally, owner and authority on the shared service).

`engine-cli` now runs its command line on a 32 MiB thread: in a Windows debug build its argument
parser needed almost exactly the 1 MiB main-thread stack, and each new subcommand overflowed it
(the merge-policy work added a dev-profile `opt-level = 1` for the same reason; both are kept).

## Source: merge-policy framework, phases N0 and N1 (18 October 2026)

[DESIGN-NODE-CONFLICT-RESOLUTION.md](DESIGN-NODE-CONFLICT-RESOLUTION.md) phases N0 and N1 are built as a
**behavioural no-op**. `engine-core` gains a `merge_policy` module: the `dynodoc.merge-policy.v1`
data format with a strict JSON loader, canonical form and SHA-256 hash; the `Strategy` trait
(merged, prefer, conflict, plus the evidence used); a registry; a laws harness (L1 to L8) that every
strategy is held to; evidence recording; the general invariants INV-G1 to G8 as flags (or conflicts
on request, never silent repairs); and the default policy `dynodoc.default.v1`, whose three
strategies call the existing logic. `workspace_merge::merge_with_policy` and
`convergence::converge_with_policy` return the usual plan plus the evidence. `merge` itself now runs
through a field selector whose plain form calls the unchanged `select`. The CLI gains `--explain`
on `repository compare`, `merge` and `verify-merge` (JSON plus a text rendering per file);
`--merge-policy FILE` works with `compare --explain` only.

Proof of no change: 1,500 random histories with four resolution sets each (6,000 comparisons) give a
byte-equal state, conflict list and operation list for `merge_with_policy` and `merge`; the generator
reaches every decision path (430 conflicted merges, 2,568 decisions, 1,377 explicit resolutions), and
making the rich-text strategy always conflict fails the test. `converge_with_policy` equals `converge`
over 600 random states. The four built-in strategies hold their promised laws over 4,000 cases each.
Policy hashes are pinned by six vectors from an independent Python generator.

Not built: any new strategy (N2 onwards), review gating (N5), a policy-aware `merge_strict`, commit
format v4, `merge-policy validate|hash` subcommands. One opt-in rule, element-wise comments and
suggestions (the N1 row asks for it), does change merge results, and a v3 commit can record only
`dynodoc.same-file.node-field.v1`, so `merge` accepts no policy yet. The debug `engine-cli` binary
needed `opt-level = 1` in the dev profile because its argument parser used nearly the whole 1 MiB
Windows main-thread stack.

Raised for the owner: `richtext::merge` is symmetric only up to mark order (frozen behaviour, so the
default rich-text strategy does not promise L1); the design's `type` filter names editor block types
but node types differ, so it matches either; a document's family (document, spreadsheet,
presentation) is not stored and is derived from section kinds, which N3 will need as a stored flag;
`CommentState` has no id, so element-wise comments compare full content; `validate_integrity` runs
three times per policy merge, a cost for N6.

## Source: agent connector read-path library (18 October 2026)

The pure part of connector Phase 0 ([AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md)) is built as the MIT crate
`crates/engine-agent`: refs, hashes, revision tokens, A1 and slide addressing, Dynodoc Compact Text
(DCT v1) with escaping and byte budgets, strict body parsers, and the `files`, `outline`, `read`,
`find` and `diff` tools over a document state (documents, workbooks, decks), with a JSON mirror and
the 26-code `AgentError`. It does no I/O. **No route, MCP transport, plugin, migration or agent
identity exists yet**; propose, submit and status (Phase 1) are not built.

Evidence: 83 tests (41 unit, 16 conformance, 2 golden with 55 committed transcripts, 12 hostile-text
parse-back tests over 49 known injection strings, 12 property tests at fixed seeds) plus Clippy and
rustdoc with warnings denied, in both repositories. Mutation checks (breaking the `@` guard or the
TAB escape) make the hostile and golden tests fail. Dependencies added: `regex` (linear-time `find`,
spec 7.5) and `similar` 3.2.0 (same pin as `engine-core`).

Byte benchmark (one release run, [agent-dct-bytes-20261018.json](benchmarks/agent-dct-bytes-20261018.json)):
over the 49 golden calls DCT is 19,467 bytes against 40,654 for the crate's own JSON mirror (2.09 times).
That is a payload ratio only. It is not a token saving and not a comparison with another tool; no
tokenizer or model was run, and the file says so.

Decisions made where the spec contradicts itself or the code (all in the crate rustdoc, 17 in total):
a header never holds document text (the spec's own examples put a sheet name there); TAB is escaped
everywhere; control, tag, bidi, zero-width-space, word-joiner and byte-order-mark characters are
dropped while ZWJ, ZWNJ, LRM and RLM are kept because Bengali, Persian and emoji need them; a list is
one block (`UL<n>`/`OL<n>`) because that is what the engine stores; `more` is absent when a response
is complete. Not implemented: markdown cell-format annotations on ranges. Owner questions: whether
zero-width characters may ever be stripped, whether reading a cell should cover only its first text
container, and the spec's diff for sheets and decks (left undefined; the crate extends it).

## Source: transparency log core and verifier library (18 October 2026)

Phases W0, W1 (core only) and W2 (verifier library only) of
[DESIGN-WITNESSES-AND-TRANSPARENCY.md](DESIGN-WITNESSES-AND-TRANSPARENCY.md) are built as a pure module,
`crates/engine-core/src/transparency/`: RFC 6962 Merkle tree with inclusion and consistency proofs,
`tlog-tiles` storage (memory and directory stores, crash-safe size marker), signed-note
checkpoints with log signatures and cosignatures, the six leaf types with blinded tags, the
inclusion bundle, a validating trust-policy loader, and assurance levels L0 to L3 with rollback
and split-view detection. **Nothing uses it yet**: no outbox, worker, migration, HTTP endpoint,
CLI verb, journal v2, sync integration, witness service or monitor exists (W1-server, W2-sync,
W3 to W7). No existing hash, signature, event or checkpoint format changed.

Evidence: 79 tests (53 unit, 15 property including random sizes to 10^5 and a 10^6 case run on
demand, 11 vector tests) plus Clippy and rustdoc with warnings denied. The vectors come from an
independent pure-Python generator (own RFC 8032 and RFC 6962 implementations, checked against
RFC 8032 vectors, the eight published RFC 6962 roots and the Go `note` package example) and the Rust
code reproduces them byte for byte. Both implementations come from one author, so the vectors
catch implementation bugs but not a shared misreading of a standard.

**Unverified (no network text was available when written; check before any interoperability
claim):** the cosignature type byte `0x04` and the `cosignature/v1` signed message and body layout,
the `tlog-tiles` path spelling and entry-bundle framing, the signed-note forbidden-character set,
and the RFC 9162 verification algorithms against a published proof vector. **Independent security
review is required before the verifier makes any trust decision in the product.**

Owner decisions raised by the build: whether fork evidence (split view, inconsistency) must always
reject even when the policy says `warn`; whether a future-dated cosignature counts (today the
freshness window is symmetric); which checkpoint L3 compares against (today only quorum-verified
ones); how to express a log-only policy (threshold 0 is refused); fixing the `seq` (u64
big-endian) and `doc_uuid` (16 raw bytes) tag encodings before any tag is published.

## Source: memory-hard key derivation for encrypted key files (18 October 2026)

`dynodoc-key-v1` files written for a passphrase now use Argon2id (RFC 9106, 3 passes, 64 MiB,
4 lanes, version 0x13) instead of PBKDF2. PBKDF2 files stay readable, and key-store-bound files
keep PBKDF2 because their wrapping secret is a random 256-bit value. The Argon2id parameters are
bounded when read (19 to 256 MiB, 2 to 6 passes, 1 to 8 lanes) and authenticated in the
associated data; the PBKDF2 associated data is unchanged, so earlier files and the first vector
still verify. A new `device rewrap` command moves an existing passphrase file to Argon2id without
the key touching the disk unencrypted. One dependency was added (`argon2` 0.5, RustCrypto,
MIT OR Apache-2.0) and the dev profile optimises it, since key derivation is unusably slow
unoptimised.

Evidence: an independent vector produced by the reference Argon2 C implementation and OpenSSL
(`gen_dynodoc_key_v1_argon2id.py`) is decrypted and reproduced byte for byte by the Rust code;
bound, authentication and format tests; the whole engine-cli suite and
Clippy pass. Format, bounds, timings and residual risks: [KEY-FILE-FORMAT.md](KEY-FILE-FORMAT.md).
Not done: hardware-token keys (needs a device and a platform API to test against).

## Source: engine PR integration and dependency migrations (18 October 2026)

Reviewed the thirteen open standalone-engine PRs with four independent read-only reviews, then
merged twelve through one integration PR (merge commits, authors kept): #14, #16, #17 (conflict
handling for parent cycles, blocks placed in deleted containers and clashing variable names),
#29 (names freed in the same change), #30 (merge-law property tests), #38 (restore a moved or
renamed block), #27 (inline atoms in the rich-text merge), #34 (node rows follow the event fold),
#35 (per-link pseudonyms in public copies), #37 (no canonical replay for member settings,
metadata and draft saves), and the two dependency PRs #5 (ulid 3) and #22 (axum 0.8), which
needed code migrations (`Ulid::generate`, brace path parameters, native async extractors).
#13 was closed by its author. **#39 (keep each side's insert runs together) is held**; see below.

Patches added on top of the contributor work, each with a test that fails without it:

| PR | Problem found in review | Fix |
|---|---|---|
| #34 | every op read `current_fields` and created blocks did a lookup; property test still accepted the null-field difference | creates skip the lookup, only field edits read fields; the test accepts only the removed-choice marker; hand test for a removed choice that is edited, deleted and restored |
| #27 | atoms align by position, so a deletion or swap on one side plus an edit on the other moved the edit to a different image | both sides changing the atom at one slot differently is a conflict; a literal U+FFFC is refused |
| #35 | nested editor `blockId`s and an orphan `parent_id` still carried a real ULID into every link; unlabelled HMAC | those ids get the same pseudonyms; the HMAC input is domain-separated |
| #14 | a cycle through a draft-new block could not be repaired with "team" | the repair picks a block the team also has |
| #16 | one scan per orphaned block under the document lock | all blocks in the missing container leave in one pass |
| #17 | typed names unchecked; rejecting a draft-only block orphaned its children | names must be non-empty, trimmed, at most 128 bytes; the draft-only subtree is removed with it |
| #14, #16, #17 | contradictory explicit choices ran to the 2N+1 pass budget under the lock | meeting the same repair twice reports the conflict and stops |
| #30 | random seeds (red runs not reproducible), thin laws, no proof the generator reaches its cases | fixed seed with environment override; identity, label-conflict, determinism and replay laws; coverage floors |
| #38 | a second nameless restore failed; a restore that kept its name came before its holder's deletion | clearing a name is never a clash; such a restore waits for the holder to leave |

Behaviour to know. Public-copy node ids change per link (clients must use ids from the response).
Member settings, metadata and draft saves now succeed when canonical history does not replay;
content reads and review transitions still return `replay_failed`
([HISTORY-RECOVERY.md](HISTORY-RECOVERY.md)). Deleted blocks may be moved and renamed (not edited)
so a restore can bring them back elsewhere.
Siblings with exactly equal position keys may render in a different order in a shared link than
in the owner's view, because a public copy lists blocks by pseudonym and the viewer breaks ties by
list order; the viewer does not sort by id.

**Held: #39.** It flips insert runs that do not interleave, loses deleted or moved siblings as
separators and grows keys on every pull, and its "team run first" rule contradicts the
documented tie-break (order same-position inserts by node id and mark them for review). It needs
a design decision and fixes before it can merge.

**Owner decision still open.** The merge changes (#14 to #17, #21, #27, #38) alter the result of
`merge` for existing inputs, and `verify_same_file_resolutions` re-runs it for strategy
`dynodoc.same-file.node-field.v1`. No v3 commits exist in production, so nothing breaks today.
Before the first v3 commit is created anywhere, either bump the strategy id (the
[node-conflict design](DESIGN-NODE-CONFLICT-RESOLUTION.md) requires a new id for a different
algorithm) or freeze merge behaviour.

Not deployed. Production runs the schema 41 release; none of these changes alter the schema.

## Source: engine PR integration (15 October 2026)

Reviewed all 24 open standalone-engine PRs. Fourteen were merged through one integration PR
([#36](https://github.com/ArefinAlter/dynodoc-engine/pull/36), merge commits, authors kept) after
a green engine CI run on the combination: #12 intra-doc links, #15 selected blocks bring new
parents and required child deletions, #18 node table checked against verified replay, #19
`verify --expect-head`, #20 unused `merge.rs` removed (its integrity cases are tests in
`integrity.rs`), #21 sorted removed choices and size-bounded text diffs, #23 tower-http 0.7, #24
proptest 1.12, #25 anyhow 1.0.104, #26 page setup and image attributes merge key by key, #28
synthetic corpus and benchmarks, #31 `replay_failed` error naming document and event, #32 admin
counters by error code and edit kind, #33 generated-history property test. Independent PRs that
touched neighbouring lines were resolved by hand; both sides of each test conflict were kept.

The application tree carries the same change set (`projection.rs`, `counters.rs`, the benchmark
examples, tower-http 0.7, no `merge.rs`). Mirrored paths are identical apart from the known
engine-only differences: crate and workspace metadata, `Taskfile.yml`, `ci.yml`, the
`is_multiple_of` form in `audit.rs`, and the rich-text fixture path in `richtext_test.rs`.

**Held back.** #13, #14, #16, #17, #27, #29, #30, #34 and #35 need changes or a rebase on the
merged `main` (comments on #14 and #34). #22 (axum 0.8) needs the `async_trait` removal and a
route-syntax migration across about 99 routes. #5 (ulid 3) needs call-site migration.

**Owner decision.** #21 changes the diff spans that `verify_same_file_resolutions` replays for
strategy `dynodoc.same-file.node-field.v1` when a changed span exceeds 4,000 characters. No v3
commits exist in production; decide before any are created whether old spans must keep replaying
(then gate the new bound on a strategy revision).

Not deployed: production still runs `f62d62c` at schema 41; none of these changes alter the
schema.

## Live: schema 41 release (14 October 2026, 08:45 UTC)

The VPS workspace, API and admin site run application `f62d62c`, schema 41: receipt-authority
epochs and rotation (no authority installed), and the in-house spreadsheet formula engine that
replaced `hyperformula`. Rehearsal on a production copy, a verified backup and unchanged
event/snapshot digests preceded activation. Evidence, rollback and an environment note:
[release record](operations/2026-10-14-schema41-release.md).

## Source: formula engine, authority rotation, key files, compression and design suites (14 October 2026)

Source is PostgreSQL schema 41 (migration 0041) and local repository schema 3; schema 41 was
deployed to production the same day (see the entry above).

**Formula engine.** `hyperformula` (GPL-3.0-only or commercial) is removed; spreadsheet
calculation uses an in-house, dependency-free engine in
`frontend/researcher/src/lib/workspace/formula/`. 458 function names, incremental
recalculation, reference rebasing for structural edits, Excel-evaluated test corpus. Evidence,
differences and bundle size (−646 KB): [FORMULA-ENGINE-REPLACEMENT.md](FORMULA-ENGINE-REPLACEMENT.md).
Decision 013's open point is resolved.

**Receipt-authority rotation (schema 41).** [REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md):
authority epochs, dual-signed rotation statements, a recorded recovery path, revocation
markers, API additions and client rules that refuse a silent break. The authority is still not
installed in production ([decision](RECEIPT-AUTHORITY-DECISION.md)).

**Key files and compression (CLI/local).** `dynodoc-key-v1` passphrase-encrypted key files with
optional Windows DPAPI, macOS Keychain and Linux Secret Service binding
([KEY-FILE-FORMAT.md](KEY-FILE-FORMAT.md)); local repository schema 3 stores objects compressed
(addresses and quotas unchanged), `compact` re-encodes existing objects
([LOCAL-STORAGE-BUDGETS.md](LOCAL-STORAGE-BUDGETS.md),
[DESIGN-PHYSICAL-PACK-FILES.md](DESIGN-PHYSICAL-PACK-FILES.md)).

**Design suites (not implemented).** [EXPANSION-ROADMAP.md](EXPANSION-ROADMAP.md) indexes about
19,500 lines: self-hosted edition, AI-agent connectors (Claude first), end-to-end encryption,
and the remaining engine items (commit pruning, cross-document merge, node-conflict resolution,
witnesses, large-file and installed-host acceptance). Decision 015 records the customer profile
and the copyright line (Verne Ltd.).

**Verification.** Rust fmt and Clippy clean; all tests pass except the Windows-only SSE timing
test. Frontend Prettier, ESLint and svelte-check clean; unit tests pass (461). The isolated
authenticated browser suite result is recorded in the release record below.

**Still open.** Pack files with shared dictionaries and commit pruning, hardware keys
(memory-hard derivation shipped 18 October 2026), cross-document merges and automatic resolution of real node
conflicts, installing the receipt authority and witnesses, large-file and installed-editor
acceptance, and all three planned products above.

## Live: schema 40 release (13 October 2026, 11:36 UTC)

The VPS workspace, API and admin site run application `0f67372`, schema 40: per-project
device-key grants and the review permission, the profile grants editor and starred projects,
and legal pages without placeholders. Rehearsal on a production copy, a verified backup and
unchanged event/snapshot digests preceded activation; the receipt authority stays
uninstalled. Evidence, rollback and the behaviour change for new keys:
[release record](operations/2026-10-13-schema40-release.md).

## Source: licence removal, device-key grants, storage collector and merge apply (13 October 2026)

Source is PostgreSQL schema 40 (migration 0040); it was deployed to production the same
day (see the entry above).

**Licence and copyright.** The AGPL-3.0 text and metadata are removed from the private
repository, the frontend package and the generated API description: `LICENSE` is a proprietary
notice (Verne Ltd.), the workspace sets `license-file` and `publish = false`, and
`package.json` is `UNLICENSED`. The engine repository's copyright holder is Verne Ltd.
([decision 013](decisions/013-private-application-licence.md)). Open point: `hyperformula`
is GPL-3.0-only or commercial and ships in the browser bundle.

**Legal pages.** Undecided facts are omitted instead of shown as placeholders
([LEGAL-PAGES.md](LEGAL-PAGES.md)); the operator facts the owner supplied stay.

**Device-key grants (schema 40).** [Contract](DEVICE-KEY-GRANTS-API.md): new keys default to
"granted" mode and open only projects and scopes their owner grants; existing keys stay
"all" until switched; grant changes apply on the next request. A fourth scope,
`history:review`, is required to approve reviews. The profile UI manages grants and lists
starred projects; `GET /profiles/{id}/starred` is added. The CLI applies a current-user-only
ACL to key and token files on Windows and refuses over-permissive key files on Unix
(`--allow-insecure-key-file` opts out).

**Storage collector and merge apply (local repository).** `gc --plan-hash` removes only staged
objects that no stored commit references, after an exact reviewed plan; commits, signatures
and refs are never pruned ([LOCAL-RETENTION-PLAN.md](LOCAL-RETENTION-PLAN.md)). `merge
--same-file-merge clean` applies node-level merges of a file edited on both sides and writes a
new signed commit format (v3; old readers reject it) with a fixed vector
([PORTABLE-PROJECT-HISTORY.md](PORTABLE-PROJECT-HISTORY.md)). `verify-merge` re-runs it.

**Decision 014.** Browser-authored project-wide branches and a follow graph are not built, with
reasons ([decision](decisions/014-web-branch-authoring-and-follows.md)).

**Not done.** Pack files and commit pruning, encrypted key files and operating-system key
storage, applying merges across documents or resolving real node conflicts automatically,
server-side receipt-authority rotation, witnesses, large-file acceptance, marketplace
publication and installed-editor acceptance.

## Live: schema 39 release (13 October 2026, 03:02 UTC)

The VPS workspace, API and admin site run application `1b469a2`, schema 39: project issues,
labels and stars, the Verne Systems operator details on the legal pages, and `/source` as a
redirect to the public engine repository. Rehearsal on a production copy, a verified backup
and unchanged event/snapshot digests preceded activation. The receipt authority stays
uninstalled (see [the decision](RECEIPT-AUTHORITY-DECISION.md)). Evidence, rollback and
follow-ups: [release record](operations/2026-10-13-schema39-release.md). The entry below
describes this revision as source; it was deployed after CI passed.

## Source: operator details, private application, issues and retention plan (13 October 2026)

Source is PostgreSQL schema 39 (migration 0039); this was deployed to production the same day
(see the entry above).

**Operator and legal.** The public legal, trust, support and brand pages now carry the
owner's operator facts from one module (`frontend/researcher/src/lib/site/operator.ts`):
Verne Systems, trading as Verne; registered address 8/A Sher-e-Bangla Road, Zighatola,
Dhaka-1209, Bangladesh; company registration C-201411-1; trade licence
TRAD/DSCC/035498/2024; `info@vernesystems.com`; +880 1870 034384. The TIN and D-U-N-S
numbers are deliberately not published. Governing law (Bangladesh) and forum (Dhaka) are
shown as proposed terms pending counsel review. The policies remain drafts and are
"Not yet effective"; the effective date, hosting provider and location, DNS provider,
representative/DPO and log-retention values are still "Awaiting confirmation".

**Application stays private.** Only the engine, API and command-line tools are open source
(MIT, public repository `ArefinAlter/dynodoc-engine`). The site and workspace are no longer
described as open source or downloadable: `/source` is a 302 to the engine repository, the
footer, FAQ, `llms.txt`, JSON-LD and licenses page carry the owner's sentence, and the
compose file no longer mounts a source archive. The engine repository's README, NOTICE,
SECURITY and CODE_OF_CONDUCT use the company contact. **Open decisions for the owner:**
the private repository's `LICENSE`, `Cargo.toml`, `package.json` and `openapi.yaml` still
say AGPL-3.0; and `hyperformula` is GPL-3.0-only (or commercial) and ships in the
workspace bundle delivered to browsers, so a commercial licence or a replacement is needed
before the workspace can be called closed source.

**Receipt authority.** Decision recorded in [RECEIPT-AUTHORITY-DECISION.md](RECEIPT-AUTHORITY-DECISION.md):
the signing authority stays uninstalled in production (immutable key, no server-side
rotation or recovery, no consumer, no rehearsal, no custodian), with an enablement checklist.

**Issues, labels and stars (schema 39).** [Project issues API](PROJECT-ISSUES-API.md):
invite-only project issues with labels, assignees, immutable comments, optimistic edit
conflicts, per-project caps, audit entries and erasure rules; project stars. The Projects UI
gains an Issues tab, a labels manager, a star button and palette actions. Project-scoped
device tokens are not accepted on these routes. The engine API proxy now forwards `PUT`.

**Storage and convergence (local, read-only).** `engine-cli repository … gc-plan`
([LOCAL-RETENTION-PLAN.md](LOCAL-RETENTION-PLAN.md)) reports unreachable commits and objects
and what stays pinned; there is still no garbage collector. `compare` gains a dry-run
`same_file_convergence` report for files edited on both sides (node and field conflicts,
order-independent address, nothing stored or signed; see
[PORTABLE-PROJECT-HISTORY.md](PORTABLE-PROJECT-HISTORY.md)).

**Verification.** Application Rust: fmt and Clippy clean; all tests pass except the
Windows-only `sse_delivers_a_new_event_within_100ms` timing assertion (Linux CI is the
acceptance run). Frontend: Prettier, ESLint and svelte-check clean; unit tests pass;
isolated authenticated browser suite 103/103 (23 specs) on a schema-39 stack.

**Not done / still open.** Packing and incremental publication, a real retention policy and
collector, applying (not just reporting) same-file merges, per-project device-key grants,
operating-system key storage, Windows key-file ACL hardening, web authoring of project-wide
branches (a browser holds no signing key), follows/profile-starred lists, marketplace
publication, installed-editor acceptance, witnesses and large-file acceptance.

## Live: schema 38 release (12 October 2026, 19:18 UTC)

The VPS workspace, API and admin site now run application `49ada1e`, schema 38, with the
new public site, GitHub-style Projects/profile/admin and project-scoped device sessions.
A production-copy rehearsal and a verified backup preceded activation; row counts and
event/snapshot digests were identical before and after (two documents, 36 events).
Shared writers stay off and no receipt-signing key is installed, so shared branch
publication is refused. Read [the release record](operations/2026-10-13-schema38-release.md)
for evidence, rollback and next steps. CI on both repositories passed on the deployed
revisions (application `49ada1e`, engine `e5b9d94`). Older entries below that record production at
`858801c`/schema 29 describe the state before this release.

## Source: public site redesign, GitHub-style workspace, device sessions (13 October 2026)

This increment changes the public site, the signed-in Projects and admin interfaces,
the add-in marketplace kit and the API. Source is PostgreSQL schema 38 (migrations
0037 and 0038); this source was deployed to production later the same day (see the entry above).

**Public site.** The marketing pages are rebuilt as a "Paper and Ink" design system
([SITE-DESIGN.md](SITE-DESIGN.md)): warm paper palette, Geist and Geist Mono, mono
`[ LABEL ]` eyebrows, dropdown header, large footer, scroll reveals, count-ups, typing
diffs and eight original pencil-and-halftone dinosaur plates that animate in on scroll
and respect reduced motion. Pages: home, features, why-dynodoc, roadmap, changelog,
brand, help, compatibility, add-ins, support, and the legal/trust set (privacy, terms,
cookies, security, subprocessors, acceptable use, accessibility, DPA draft, licenses,
`/.well-known/security.txt`). All research/questionnaire framing is removed from the
public copy, FAQ, `llms.txt` and structured data. Legal pages remain drafts; every
operator, address, email, jurisdiction and effective-date field still reads
"Awaiting confirmation" and must be filled by the owner ([LEGAL-PAGES.md](LEGAL-PAGES.md)).
Public links now point at the public MIT engine repository because the application
repository is private.

**Workspace.** Projects, the repository page, file page and profile follow GitHub's
layout: global header with command palette (Ctrl/Cmd+K, `/`, `t`, `?`, `g p`),
file table with last-change summaries, README panel, formats bar, contributors,
pull-request list with Open/Merged/Declined counts, activity feed, contribution graph
and history list. A read-only **Branches** tab lists shared project history
(refs, generation, head commit, signed receipt metadata, reviews); the browser never
claims to verify a receipt. Creating and merging project-wide branches in the
browser, stars, issues and labels are not built.

**Admin.** `/admin` is reorganised as a left-nav console (overview with needs-attention
list, people/projects/content, audit log with filters and detail, storage, services,
traffic, database, danger zone). All existing controls and accessible names are kept.

**API (schemas 37 and 38).** Read-only APIs back the new UI ([contract](PROJECT-ACTIVITY-API.md)):
file `last_change`, project activity feed, contributors, profile activity calendar,
admin project overview and audit detail/filters; migration 0037 adds only indexes.
[Project-scoped device sessions](PROJECT-SCOPED-DEVICE-SESSIONS.md) (migration 0038):
a device holding an enrolled Ed25519 key signs a one-use challenge and receives a
one-hour token for one project with a subset of `project:read`, `history:read`,
`history:write`; renewal is another signature, so the key file is the only long-lived
credential. Token confusion is closed with a Paseto implicit assertion; the allow-list is
21 explicit handlers; every request rechecks key, account, generation and current role.
`engine-cli device keygen`, `sync --device-key` and `sync --refresh-token-file` are added.
Admin "sign out all sessions" now also revokes the account's device keys. An independent
review found no critical or high issue; accepted residual risks are an unencrypted key
file as a single factor for account-wide history write/approval until per-project key
grants or a review scope exist, no Windows ACL hardening, and no OS key store.

**Add-ins.** Manifests are production-named (Dynodoc for Word/Excel/PowerPoint), carry a
64 px icon and the support URL, and are covered by 32 offline invariant tests; the online
Microsoft validator accepted all three. Marketplace assets, listing copy, reviewer notes
and an owner checklist are in `docs/listings/` and `extensions/marketplace/`. Panes drop
"development" wording and make browser sign-in primary. No listing exists, none is
submitted, and installed-host acceptance has not been run.

**Verification.** Application Rust: all tests pass except the known Windows-only
`sse_delivers_a_new_event_within_100ms` timing assertion (385 tests; Linux CI is the
acceptance run). Clippy and rustfmt clean; OpenAPI regenerated. Frontend: lint,
Prettier and svelte-check 0 errors; 340 unit tests; 25 public browser tests; axe on 20
public pages; authenticated suite 99/99 on an isolated stack built before migration 0038.

**Not done:** a retention/GC plan command and same-file convergence reporting were
started and set aside incomplete (partial work retained outside the repository);
storage packing, offline convergence, witnesses and large-file acceptance remain open.
Per-project key grants, OS key storage and project-wide branch authoring in the web UI
remain open. Marketplace publication, legal operator details and real-host acceptance
need the owner.

## Source: engine PR review and application synchronization (12 October 2026)

Reviewed all ten open standalone-engine PRs and merged **#1, #2, #3, #4, #6,
#8, #9, #10 and #11**. PR #5 remains open: ULID 3 removes `Ulid::new()`;
[fresh CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38251056776)
fails with E0599. It needs an explicit call-site/API migration and fresh tests.
The engine and application retain ULID 1.2.1.

The merged updates use checkout v7, UUID 1.28.0 (the old PR title said 1.27.0),
clap 4.6.7, serde 1.0.229 and similar 3.2.0. PR #1's old failure was generated
OpenAPI metadata drift; updating its branch made the full workflow pass. PR #4's
overlapping lockfile changes were resolved to retain syn 3 for both derive crates.
The final lockfile matches the combined locally tested dependency graph.

Replay now decodes borrowed payload JSON. Merge canonicalization borrows states
without removed choices, retaining the newer cached tree-depth implementation
when resolving PR #9. Saving a named version avoids a redundant state load while
keeping the document/project locks, authorization and verified snapshot path.

PR #10 adds streaming `log::verify_from` for an explicitly trusted chain position.
Review caught two regressions in the proposed implementation: full-chain audits
could omit negative sequence rows, and a maximum-sequence anchor could overflow.
Both new regression tests failed against the original implementation and pass
with the corrections; all ten event-log tests pass. Genesis verification still
checks every stored row. A non-genesis trusted position intentionally excludes
earlier events; it needs an independently trusted anchor, not a database-only
claim of independent audit security.

The original combined candidate passed **358 Windows tests**, none failed,
ignored or filtered. After the audit fixes, targeted event-log tests, final
`task lint`, `task docs` and unchanged generated OpenAPI passed. Documentation
builds retain existing rustdoc warnings. Historical hash, snapshot/receipt and
rich-text replay fixtures were not regenerated. Similar 3.2 changes new diff
heuristics; the tested replay and merge cases pass, without claiming identical
hunks for all inputs or complete offline convergence.

Final engine source: `422e011`; application synchronization: `15f42c6`.
[Final engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38253849857) and
[application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38253873729) passed all **360 tests**
on those code revisions. Application package/license metadata is preserved.
Both frontend jobs, including authenticated browser workflows, passed in
[application frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38253873733).
Handoff/status updates follow separately so the evidence identifies tested code.

No migration, stored hash format or API schema changed (PostgreSQL 36 / SQLite 2).
The disposable PostgreSQL container was identified by its task label and removed;
existing development containers were untouched. The two clean task review
worktrees were removed after verification. Logs remain under ignored
application `target/engine-pr*.log`. No production data or real email was used.
No VPS deployment or installed-editor acceptance occurred. Production remains
recorded at `858801c` / schema 29; the preceding selected-history sync handoff and
roadmap acceptance ledger still describe the remaining product work.


## Previous increment: selected-history CLI synchronization (12 October 2026)

The operator CLI now connects local project history to the schema-36 API through
reviewed push plans, selected reachable-object transfer, authorized reuse of
published bytes, verified fetch and separate receipt-journal remote tracking.
Fast-forward pull advances only an explicitly selected local branch with exact
address/generation CAS; divergent local work remains intact. No command writes an
open/native file. Publication plans disclose private ancestry and recovery sources;
execution cannot add an unreviewed dependency. Retries reuse accepted objects and
commits; stale shared refs, corrupted objects and inconsistent receipt proofs fail
closed. See [the contract and procedure](SELECTED-HISTORY-SYNC.md).

This adds no migration/hash format change (PostgreSQL 36 / local SQLite 2). HTTP
origins, credentials, response sizes, request deadlines and cumulative work are
bounded. Initial authority trust comes from independently supplied configuration;
accepted heads remain in the separate journal. First trust is a checkpoint, not
proof of unseen older receipts. Multi-commit publication can partially succeed;
complete file-state validation/scans remain, and local quotas include partial work.

Code is pushed to main: application `47cb87f`, standalone engine `4ca7f16`.
[Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38248717652)
and [standalone engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38248983325)
passed on those exact revisions. Each Linux CI run passed all **356 tests** with
no failures, ignored or filtered cases. Both Rust workflows cover formatting,
Clippy, build/test and unchanged OpenAPI consistency; standalone also builds docs.
Both frontend build/test and authenticated browser workflows passed in
[frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38248717732).

The Windows standalone workspace run passed all **355 tests** before the final
up-to-date race regression was added, including the unchanged 100 ms SSE check.
After that final fix, task lint and all **seven CLI tests** passed. Six new sync
scenarios cover restricted-runtime real HTTP with two authors, privacy and private
ancestry disclosure, shared byte reuse, publish-only review, stale/lost-response
retries, fast-forward/local-CAS/divergence behavior, resumable corruption failure,
receipt rollback/fork/gap/key/revocation and a concurrent up-to-date response race.
Task docs passed with existing rustdoc warnings; the actual sync command help
smoke passed. No test threshold or coverage was relaxed. The dedicated PostgreSQL
container was verified by its task label and auto-remove setting, then stopped
and removed; existing development containers were untouched. Test HTTP listeners
and temporary SQLite databases were closed/removed by the tests. Logs remain in
ignored application target/sync-*.log. No real email or production data was used.

Production is still recorded at application `858801c`, schema 29, shared writers
off. No VPS rollout or installed Office/Google acceptance occurred in this increment.
Schema-36 deployment still needs a production-copy rehearsal, compatible rollback
images and operator receipt-key provisioning. Next priorities are project-scoped
device sign-in/token renewal and native key storage, Projects/native branch/review
adoption, incremental storage/GC, richer same-file convergence, independent audit
witnesses/rotation and measured large-file/service acceptance. The owner runs the
prepared installed-host worksheet. The full product roadmap is not complete.

## Previous increment: shared publication, protected refs and receipts (12 October 2026)

Schema 36 adds [shared project history](SHARED-PROJECT-HISTORY.md): explicit
candidate publication, reachable-object downloads, published multi-author ancestry,
fast-forward branch CAS and exact candidate/base reviews. Main applies current
project/file policy floors and independent approvals; contributors can advance
feature branches. Ref generation, signed receipt and audit entry commit atomically.
Receipts bind an operator-installed public authority and use the existing client
pin/journal contract. API signing keys must be explicitly mounted outside the DB;
there is no unsigned fallback or silent replacement of the installed authority.

Published metadata has per-project byte/row/branch quotas. Identical file roots
reuse reachability indexes. Contributors can acknowledge up to 256 objects from
an explicitly selected authorized publication without uploading their bytes again.
Private hashes remain inaccessible; mixed invalid/quota-limited batches roll back
and exact retries are free. Full graph scans, complete commit file manifests and
changed-root index amplification remain. Document erasure removes affected
published envelopes and descendants/reviews while retaining opaque receipt
commitments and generations. Affected heads become unavailable and never fall
back. Account erasure clears private work while retaining shared publications
under current access checks. Reading an older commit does not rewind any branch.

Final code is pushed to main: application `23ed7bf`, standalone engine `5f86db2`
(initial publication/receipt implementation `0d501ca` / `15cc6c4`).
[Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38225348149),
[standalone engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38225347973)
and [frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38225348138)
all passed, including both frontend build/test and authenticated browser workflows.
Linux ran all **350 tests**, including the unchanged 100 ms SSE deadline. Five new
restricted-runtime scenarios plus the core authority-installation test cover
publication/privacy, cross-author ancestry and content reuse, policy/base/role
checks, concurrent CAS, quota rollback, key/account/document erasure, immutable
boundaries, atomic receipts and independent client pin advancement/rollback.

Local standalone task lint and task docs pass; existing documentation warnings
remain. Contracts/types are regenerated with distinct shared-history operation
names and preserved standalone MIT/contact metadata. Frontend lint/Svelte checks
pass with zero diagnostics. The expanded object-reuse regression passes, including
rejection of private hashes, all-or-nothing quota failure, free retries and the
256-address boundary. Its initial 16 KiB request cap was too small for that boundary;
it is now 20 KiB. There is no migration/hash/commit format change in that follow-up.

The full Windows task test run again failed the existing 100 ms SSE assertion.
A complete run explicitly excluding only that test passed all **349 remaining
tests**, with no other ignored/filtered cases. No threshold or CI coverage was
relaxed; this is not a Windows latency guarantee. The actual operator CLI migrated
an isolated database to schema 36, installed a synthetic public authority and
provisioned a restricted runtime login. A mismatched DER key prevented API startup;
the matching key served health and the final API contract. That hidden test process
was stopped, and the task-owned PostgreSQL container and synthetic key scratch
directory were removed. Existing development containers were untouched. Ignored
verification logs remain under application target/. No real email, production data
or installed Office/Google host was used.

Production remains recorded at application `858801c`, schema 29, shared writers off.
No VPS upgrade was made in this pass. Schema 36 needs matching runtime grants,
receipt-key provisioning, a production-copy rehearsal and schema-compatible
rollback images before deployment. The owner runs installed-editor acceptance
using the prepared worksheet; source/package tests do not certify those hosts.

Next: automatic selected-root push/fetch and remote tracking, with explicit
publication of dependencies; project-scoped device sessions; Projects/native
branch/review adoption. Private/unreachable local inventory must stay private,
and the server-reported authority key must not silently become a trusted pin.
Server key rotation/distribution, witnesses, audited erased-branch reinitialization,
reachability GC/packing, complete same-file offline convergence and realistic
large-document/service acceptance remain open. These APIs do not advance centralized
per-file events or replace web drafts. The full roadmap is not complete.

## Source: private project history transfers (12 October 2026)

Schema 35 adds account-private authenticated object/commit exchange, described in
[Private project transfers](PRIVATE-PROJECT-TRANSFERS.md). Changed typed objects
are shared physically per document; immutable per-account upload acknowledgements
prevent guessed hashes from exposing other drafts. New signed multi-file commits
verify active enrolled keys, dependencies, recovery semantics and complete typed
graphs. Accepted ancestry remains readable after key revocation, subject to current
project/file access. No project branch, team document or review is published.

Persistent account staging budgets cover logical acknowledged bytes and signed
commit/scope metadata: 4 GiB, 1,000,000 acknowledgements and 100,000 commits. Batches
are atomic, retries free, quota failures return 507. Current API limits include
256 historical files per transitive scope and 64 MiB/65,536 graph reads per commit.
Audited document erasure removes affected envelopes and all descendant envelopes,
even after they remove the file; account erasure removes its private staging.
The CLI can inventory/export/import the same bounded base64 object packets.

Final tested code is pushed to main: application `be538a6`, standalone engine
`554afec` (initial implementation `2d5f613` / `1a4e6fb`).
[Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38212242810),
[standalone CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38212247413)
and [frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38212242806)
all passed, including both frontend build/test and authenticated browser workflows.
The complete Linux engine suite passed all 344 tests, including the unchanged
100 ms SSE assertion. Five new restricted-runtime tests cover transfer into a
second SQLite repository, privacy, revoked-key history, recovery, current scopes,
quota rollback/concurrent spending, permission changes during a lock wait and
both erasure paths.

Local verification: standalone task lint and task docs pass; documentation retains
its existing warnings. Generated OpenAPI exactly matches the committed Git blob,
including standalone MIT/contact metadata; the initial raw working-file hash check
was sensitive to Windows line endings. Web API types are regenerated, and local
frontend lint/Svelte checks pass with zero diagnostics. The actual CLI packet smoke
reconstructs the same state in another database, inserts zero objects on retry,
rolls back invalid packets and leaves the local branch unchanged.

The full Windows task test run and an isolated recheck both failed the existing
100 ms SSE deadline. A separate full run explicitly excluding only that assertion
passed all 343 remaining tests. Its earlier attempt exposed a test-cleanup race:
SQLite releases its final Windows handle shortly after Pool::close. The test now
uses the existing bounded retry for sharing errors 32/33; other cleanup failures
remain fatal, and the final run passed. This is not a clean local full-suite result
or a certified Windows latency guarantee. No timing threshold or CI coverage was
relaxed. Linux full-suite acceptance is recorded above.

The first frontend CI attempt failed during package-manager setup: floating pnpm
10 resolved to an unavailable tarball. Application `fdb7624` pins pnpm 10.34.6,
whose npm archive was verified with HTTP 200; both subsequent frontend/browser CI
runs passed. Task-owned PostgreSQL and synthetic SQLite scratch files are removed.
No task test servers remain. Ignored logs are retained under application target/.
No installed Office/Google host, real invitation or production data was exercised.

Last recorded VPS release: application `858801c`/schema 29, shared writers off.
No VPS deployment or production migration occurred in this pass. Source schema 35 needs compatible runtime grants
and rollback binaries plus a production-copy rehearsal before rollout. Installed
Office/Google acceptance remains the owner's local worksheet, not a completed test.

Next: shared candidate publication with explicit reader reachability, protected
project refs/reviews and atomic server receipts; selected-root push/fetch and remote
tracking; then Projects/native adoption. Private acknowledgements must never become
shared discovery. Whole-document erasure must cover future accepted history too.
Complete same-file convergence, changed-node publication/packing, trusted witnesses,
large native/service measurements and marketplace acceptance remain open. The full
roadmap is not complete.

## Source: persistent local repository capacity (12 October 2026)

Implementation: application `962c9e1`, standalone engine `5822e57`, both pushed
to main. [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38207702226)
and [standalone CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38207706865)
passed; the full Linux engine run passed all 339 tests including the unchanged
SSE timing assertion. [Frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38207702234)
also passed, including both the build/test job and authenticated browser workflows.

The [two reproducible storage experiments](LOCAL-STORAGE-BUDGETS.md#measured-small-history-growth)
retained 33 revisions of 128/512-paragraph synthetic graphs. Encoded objects plus
commits used 273,586/736,893 bytes versus 1,896,632/7,599,032 bytes for uncompressed
full JSON states. Actual SQLite files used 589,824/1,695,744 bytes. Median debug CLI
state writes were 280/1168 ms; unchanged retries stored zero bytes but took
277/1523 ms. This exposes the remaining full-state scan cost; it does not certify
large native files, peak memory, HTTP transfer or service throughput. Source
revisions, binary digest and every timing are in the reports. Synthetic benchmark
databases and keys were removed after recording the reports.

SQLite schema 2 now enforces [persistent local capacity](LOCAL-STORAGE-BUDGETS.md)
for retained objects, signed commits and branches: 4 GiB of encoded object/commit
payloads, 1,000,000 objects, 100,000 commits and 4096 branches by default. Staged
and unreachable objects count. Exact retries are free, failed batches roll back,
and existing refs remain usable at capacity. Explicit CLI/library limit changes
never prune history and cannot reduce capacity below current usage. Schema 1
upgrades transactionally without changing roots/signatures/refs; above-default
usage is retained. Older schema-1 clients reject schema 2. Physical disk/WAL,
trust-journal budgets, packing and explicit retention/GC remain separate work.

Validation: all 32 application repository tests pass, including five new migration,
quota, rollback, concurrency and missing-accounting regressions. Both Rust Clippy
checks and formatting pass. Rust documentation builds with the existing warnings.
Two complete local Windows runs failed the existing 100 ms SSE assertion; its
isolated run passed. A separate full run explicitly excluding that one timing test
passed all 338 remaining tests. This is not a clean full-suite local result or a
certified latency guarantee. The check includes the write request, database work
and per-frame authorization, not just broadcast delivery. Its threshold and CI
coverage were left unchanged. Full Linux CI passed, as recorded above.
[Actual CLI acceptance](benchmarks/local-storage-cli-20261012.json)
checks persisted capacity across processes, rejected-write rollback, explicit
increase, zero-cost reuse at capacity and reading the retained root. Logs are
under ignored target/. No native Office/Google host was exercised in this pass.

The preceding device/shared-budget changes are application `a72333a` and engine
`932f48a`: application Rust CI 38204054464, frontend CI 38204054384 (including
authenticated workflows), and standalone CI 38204058559 all passed. Source hashes
and CI for local capacity are recorded above. The prior frontend verification
also passed 289 unit tests, lint, build and three local Projects browser tests.

Application sources are mirrored to D:/dynodoc-engine; MIT metadata is preserved.
Task-owned PostgreSQL and synthetic CLI scratch data are removed; API/web test
servers remain stopped. Only ignored validation logs and the synthetic report remain.
PostgreSQL source stays schema 34. Production remains `858801c`/schema 29, shared
writers off; no deployment or production migration occurred. Before rollout,
prepare a schema-34-compatible rollback image and rehearse with a production copy.
Installed-editor acceptance remains the owner's prepared local worksheet.

Next: authenticated project commit/object discovery and exchange with explicit
private-stage/reachability/whole-document-erasure rules; protected project refs,
review and atomic server receipts; then project-wide UI and native connector
adoption. Local inventory must not become a public remote listing. Recheck current
account/key/project/document rights in the publication transaction, distinguish
already-accepted signed ancestry from new actions, and retain access checks after
revocation/erasure. Complete same-file convergence, incremental native capture,
packing/changed-node publication, trusted witnesses, large-document/service
acceptance and marketplace publication remain open in [the ledger](ROADMAP-ACCEPTANCE.md).
This checkpoint does not complete the product roadmap.

## Source: device identity and retained shared storage (12 October 2026)

Schema 33 adds [account-bound signing keys](DEVICE-SIGNING-KEYS.md): one-use
five-minute account/installation challenges, Ed25519 possession proofs, immutable
bindings, permanent revocation, current-key commit verification, offline CLI proof
signing and own-profile enrollment/revocation. Account removal revokes keys;
restoration cannot revive them. Transactional login rechecks cover the logout race.
Historical signatures remain verifiable after revocation. Signature verification
does not store/publish a commit or confer any project permission.

Schema 34 adds [durable shared-object budgets](SHARED-STORAGE-BUDGETS.md):
1 GiB/250,000 objects per document and 4 GiB/1,000,000 per creator by default.
Transition-table accounting charges only inserted objects, preserves reuse at
capacity, rolls back failed graphs and releases quota through audited erasure.
Counters/limits are read-only to the runtime. Own-profile usage and HTTP 507 errors
make capacity failures visible. This is encoded shared-history accounting, not a
physical disk, event/legacy snapshot, upload or local SQLite quota. The pilot
native-file cap remains 3,000,000 bytes. No shared writer was enabled in production.

Validation: all 334 standalone Rust tests pass, including the schema-33→34
backfill regression, restricted-runtime byte/count quota tests, concurrent account
spending, key enrollment/retry/revocation and logout races. The final targeted
quota run also tests byte and object limits independently. Both Rust Clippy checks
pass. Frontend lint/Svelte checks, 289 unit tests, production build and all three
authenticated Projects browser tests pass with shared draft/named writers enabled
on an isolated schema-34 stack. The browser test caught and verified a fix for the
Svelte-interpolated key-length pattern. The original full engine attempt missed
the existing 100 ms SSE deadline under concurrent linking; its isolated and full
reruns passed unchanged. No performance threshold was relaxed. The CLI smoke
matches the independent Node/OpenSSL vector, rejects wrong scope and preserves
existing output files. Synthetic private key files were removed. OpenAPI and web
types are regenerated; standalone OpenAPI retains its MIT metadata and matches
its generator. Rust documentation builds with 20 existing warnings. CI is pending
the source push.

Application source is mirrored into D:/dynodoc-engine with MIT metadata preserved.
The task-owned API/web acceptance servers are stopped and the labeled disposable
PostgreSQL test container has been removed. Logs live under ignored target/. Production remains 858801c/schema 29. No production migration or deployment
occurred; a schema-34-compatible rollback build and production-copy rehearsal are
still required before rollout. Never start a schema-29 image on schema 34.

Next implementation order: authenticated remote commit/object discovery and
exchange with explicit private-stage/reachability/whole-document-erasure rules;
protected project refs/reviews and atomic server receipts; portable project UI and
connector adoption. Do not expose the local inventory as an HTTP endpoint or treat
a prior verify-commit response as a grant. Recheck the live account/key/permissions
inside publication. Retained ancestry stays readable after signer revocation under
the reader's current access. Physical packing/incremental publication, local-store
retention, complete same-file convergence, background capture, RTF/native fidelity,
large-document/service evidence and marketplace acceptance remain open in
[the ledger](ROADMAP-ACCEPTANCE.md). Installed Office/Google acceptance is prepared
for the owner to run locally. This is a source checkpoint, not roadmap completion.

## Source: portable project revert and cherry-pick (12 October 2026)

Application `7d889ac` and engine `bdde0f3` add core/CLI
[commit-addressed project recovery](PROJECT-COMMIT-RECOVERY.md).
Preview applies a source delta (or its inverse) to an exact current commit; new
signed v2 commits retain source/mainline as provenance and current commit as their
only ancestry parent. Existing v1 hashes/signatures remain unchanged. Root sources
use an empty baseline; merge sources require a chosen parent. Same-file competition
and shared identity/path constraints require explicit resolutions. Import verifies
the sources, recomputes the plan and rejects unrelated settled-path changes.
Publication remains a separate generation-aware branch CAS.

All 25 repository regressions pass, including recovery fifty commits back,
preservation of later independent work, restart/retry, missing/forged dependencies,
mainline selection, conflict validation and independent Python/OpenSSL v2 vectors.
Application Clippy passes. The [actual CLI smoke](benchmarks/project-recovery-cli-20261012.json)
passes preview, revert, exact retry, historical reconstruction, later-file retention,
branch publication, stale-CAS rejection and cherry-pick. Synthetic key files were
removed. All 325 standalone Rust tests pass, along with task lint and task docs
(the same 20 existing rustdoc warnings). Application focused tests, formatting and
Clippy pass. The disposable PostgreSQL container and its volume were removed after
validation; no local API/frontend server was started. Synthetic reports/databases
and logs remain under ignored target. SQLite schema 1 and PostgreSQL schema 32 are
unchanged. Production remains application 858801c/schema 29, with no new writer
enabled. [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38171109983)
and [standalone CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38171113730)
pass all 325 tests plus their lint/build/OpenAPI checks. The application source
packaging regression and standalone documentation check also pass.
[Frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38171109919)
passes both jobs, including frontend checks, unit tests, the shared Google connector
drift check, public browser tests, production-proxy regression and authenticated
browser workflows. These CI browser sessions are not installed Office/Google
acceptance. No deployment occurred.

This completes local commit-addressed recovery, not hosted project branches or
native recovery adoption. Next remains authenticated project transport and device
identity, permission/private-stage/erasure rules, protected reviews and atomic
server receipts. Packing/incremental publication, complete same-file convergence,
native/background capture, representative scale and installed-host/marketplace
acceptance remain open in the [ledger](ROADMAP-ACCEPTANCE.md). Installed-editor
acceptance is prepared for the owner to run locally. Continue across this checkpoint.

## Source: signed ref receipts and durable client trust (12 October 2026)

Application `3218315` and standalone engine `20caed4` add [signed ref receipts](REF-RECEIPTS.md) and a separate SQLite client
trust journal. Scope, generation, commit, predecessor, declared time and authority
key are signed. Contiguous proofs reject rollback, conflicting heads, missing
updates and scope/key substitution. Verified-prefix retries are idempotent.
Authority rotation requires both keys at the exact pin; local revocation persists
across restart and cannot be undone by retries or a replacement bootstrap.
Journal publication is transactional and concurrent conflicting updates have one
winner. The CLI exposes verification, first trust, advance, rotation and revocation.

Validation: workspace check/Clippy passes; all 18 repository tests pass,
including eight new receipt/journal regressions. Independent Python/OpenSSL vectors
pin canonical hashes and receipt/rotation signatures. The local demonstration
creates real synthetic project commits, exercises a separate client journal,
reopens it and verifies fork/rollback/retry/rotation/revocation behavior. See
[its record](benchmarks/receipt-journal-20261012.json). The built CLI successfully
reads the reopened, revoked journal without PostgreSQL or network access.
A follow-up fixes both signing commands to accept standard unencrypted DER
PKCS#8 v1 keys as well as v2, retaining v2 public/private component checks.
The new CLI regression passes against independent OpenSSL signatures; an actual
CLI smoke signs/verifies a v1 key, trusts its receipt, signs/accepts a dual-key
rotation, retries and revokes the journal. Synthetic key files were removed.
Source follow-up: application `07db4bf`, standalone engine `8386adb`.
All 318 Rust tests pass in the standalone local suite and in both
[application CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38169393679)
and [engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38169397678).
Formatting/Clippy pass; documentation builds with the same 20 pre-existing warnings.
[Frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38169393675)
passes, including 19 public browser tests and 72 authenticated workflows (the
public job intentionally skips its 53 authenticated cases). The disposable test
PostgreSQL container and its volume have been removed; unrelated containers were
preserved. No local application servers were started. Source is schema 32;
production remains schema 29/application `858801c`. No new production migration,
shared-writer enablement, installed-editor or marketplace acceptance occurred.

These are core/CLI integration primitives. Production does not yet issue signed
ref receipts or distribute authority keys. Account/device registration and remote
revocation, atomic protected-ref publication with receipts, proof endpoints,
witnesses/gossip and connector adoption remain open. A declared time is not
trusted timestamping. Initial trust cannot prove unseen history, and an attacker
who can roll back the client's own trusted journal can destroy its pin.

Next: authenticated remote project history with explicit staged-object visibility,
current project/file permissions, protected refs/reviews, account/device key
binding and audited erasure of multi-file history. Do not expose object inventory
through private drafts or treat a local commit signature as an account grant.
Physical packing/incremental publication, same-file offline convergence, native
format/background capture, realistic large/concurrent workloads and installed
Office/Google acceptance also remain in the [ledger](ROADMAP-ACCEPTANCE.md).
The owner will run installed-editor acceptance locally. Continue across this
checkpoint; the complete roadmap is not certified.

## Source: shared named/deployed snapshots (12 October 2026)

Application `f8f0adb` and standalone engine `107e45e` add [shared named snapshots](SHARED-NAMED-SNAPSHOTS.md),
schema 32. Named versions, public-copy pins and deployed snapshots can refer to
the same typed objects as drafts and periodic checkpoints. The API hydrates the
existing response shape after verifying the stored receipt and referenced graph.
New receipts use a separate v2 hash domain; the legacy vector and stored rows are
unchanged. The new explicit setting is DYNODOC_NAMED_SNAPSHOT_STORAGE=shared-v2;
the default remains legacy and production is still schema 29/application 858801c.

Targeted checks pass: 19 snapshot integration tests and all 45 API integration
tests, including mixed histories/concurrent writers, object reuse, failed-write
rollback, root/object tampering, independent replay/backfill, historical restore,
public-copy privacy and audited erasure using restricted runtime credentials.
Formatting/Clippy and all 309 Rust tests pass in both repositories. Standalone
task docs passes with the same 20 pre-existing warnings. [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38167894976)
and [engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38167901880)
pass. [Browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38167894975)
passes both its build/unit/public job and all authenticated workflows.

No production migration, optional-writer enablement or hosted package update has
occurred. The task-owned disposable PostgreSQL container
dynodoc-snapshots-test-20261012 and its volume were removed after the remaining
suites completed. Logs and synthetic demo artifacts are under ignored target.
Both repositories carry matching source; preserve standalone metadata/fixtures.

Remaining storage work is physical packing, changed-node publication and realistic
growth/resource acceptance. Remote project history must bind signing keys to
accounts/devices, enforce current file/project membership, preserve private drafts,
serialize protected refs and review, and make whole-document erasure explicit for
multi-file ancestry. Local signed commits alone do not supply these boundaries.
The complete [acceptance ledger](ROADMAP-ACCEPTANCE.md) remains open; installed
Office/Google acceptance is assigned to the owner using the local worksheet.
Continue the roadmap across this checkpoint without requiring another prompt.

## Source: typed state, shared drafts and local project history (12 October 2026)

This pass continues across storage, repository and algorithm boundaries. The
[acceptance ledger](ROADMAP-ACCEPTANCE.md) tracks the whole roadmap without marking
unfinished integration as complete. Application implementation: `8dd816b`;
standalone engine implementation: `f1cdf8c`. Newer entries above record shared
named/deployed snapshots and client audit tools; the evidence below covers these commits.

Implemented:

- [Typed v2 checkpoints](TYPED-CHECKPOINTS.md), chunked text/value/asset references,
  global encoded-object and semantic reconstruction budgets, exact replay comparison,
  mixed historical readers and explicit codec selection. Schema 30 adds capability;
  existing v1 roots/events remain unchanged.
- [Shared draft bases](SHARED-DRAFT-BASES.md) at schema 31: private drafts, pushed
  requests, recovery and rebase bases can store typed roots instead of repeated
  full JSON. Transactions, document-scoped FKs, immutable initial bases and verified
  reconstruction preserve access/review/stale checks and existing API responses.
  DYNODOC_SHARED_DRAFT_BASES=shared-v2 opts in; default remains legacy.
- [Portable local project history](PORTABLE-PROJECT-HISTORY.md): SQLite WAL/FULL
  durability, signed multi-file commits/parents, generation-aware branch CAS,
  bounded/idempotent missing-object transfer, all best merge bases, conservative
  file comparison, and explicit CLI commands. Different paths still conflict when
  they share a document identity or namespace constraint. Signatures do not supply
  account binding, device revocation or independent witnesses.
- Cached iterative parent-depth preparation replaces repeated walks during diff
  sorting. Its ordering step is O(N log N) with O(N) auxiliary metadata. Other
  diff/state costs remain; the regression covers cyclic/missing-parent graphs and
  a 20,000-level chain.

Verification: formatting/Clippy and **303 Rust tests pass in each repository**
against isolated PostgreSQL 16. Standalone task docs passes with the same 20
pre-existing rustdoc warnings. Fixed content/commit hashes and previous v1 event
vectors remain green. [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38166114708)
and [engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38166122646)
pass, including OpenAPI compatibility and source packaging where applicable.
[Frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38166114711)
passes 289 units, 19 public browser checks (53 authenticated checks intentionally
skipped in that public-only job), and all 72 authenticated-stack checks. The local
repository CLI reads the demo's common head at generation 2. Standalone package
metadata, licensing, toolchain and independent fixtures remain preserved.

The synthetic two-replica example reopens both databases, exchanges only missing
objects, publishes the same reviewed project merge and recovers the initial
report. Repeating transfer sends zero objects. Initial transfer: 18 objects/2534
encoded bytes; each changed side sends eight objects (~1.2 KiB). See the
[record](benchmarks/local-repository-20261012.json). This is not native-editor,
realistic large-file, HTTP, physical SQLite storage or service-load acceptance.

Production remains application 858801c, schema 29. No live migration, writer
enablement, hosted package/archive update or marketplace submission occurred.
Schemas 30/31 require matching readers and a compatible rollback build; old
schema-29 binaries must not be reused after migration. Shared periodic writes
remain off. The task-owned disposable database dynodoc-roadmap-test-20261012
and its anonymous volume were removed after both suites. Unrelated containers
were preserved. Synthetic demo databases/logs remain under ignored target.
The owner still runs installed
Office/Google acceptance locally using the existing worksheet.

Next integration priorities: shared named/deployed versions, physical packing and
incremental publication; authenticated project transport/protected refs/device
identity and connector/UI adoption of local history. Then complete same-file
offline merge/constraint handling, native graph/fidelity/background capture,
independent receipts/witnesses and realistic large/concurrent service acceptance.
Current hosted drafts/branches remain per file. Do not call the total roadmap
complete or treat the new local library as an installed Office/Google integration.

## Source: streaming shared content and local acceptance kit (12 October 2026)

Application implementation `3462520` adds evolution part 2f. Standalone engine
implementation: `746d373`. Read [the binary contract, resource
bounds and measurements](CHUNKED-CONTENT.md). A separate content hash domain stores
64 KiB pieces under bounded 128-way trees; streaming I/O and object/byte budgets
avoid a full input/output buffer. The document-scoped memory/Pg stores retain
byte validation, reuse, transactional rollback, immutability and audited erasure.
V1 state/event hashes and migrations are unchanged. No root is silently published
as a checkpoint/commit, and no default writer or upload limit changes.

Verification: application formatting, workspace Clippy and all 284 Rust tests pass
against an isolated local PostgreSQL 16. Eight new tests cover independent fixed
hashes, chunk/tree boundaries, fragmented I/O, reuse, scope, repeated-reference
budgets, malformed graphs, I/O errors and Pg rollback/durability. Existing v1
compatibility, history and erasure checks pass. Standalone task lint, all
284 Rust tests and task docs also pass;
rustdoc retains the same 20 pre-existing link warnings. The
[application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38162089809)
and [engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38162187293)
pass, including OpenAPI compatibility and source packaging where applicable.
The [frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38162089764)
passes lint/type/build/package checks, 289 frontend units, 19 public browser
checks, and all 72 tests against the authenticated stack, including the
production-proxy regression. Source inputs are the commits above; later
documentation-only commits record evidence without changing those inputs.
All changed engine sources/fixtures are mirrored; application/engine packaging
and existing independent fixture/toolchain differences are preserved.

The 100/250/500 MiB release probe streams generated repeated bytes to a sink. Peak
working set was 5.95/5.54/5.53 MB and each trace retained five unique objects. This
checks streaming behavior only; it is not native-file, realistic-entropy database,
concurrent-service or user-capacity acceptance. Fixed boundaries can rewrite later
chunks after insertions/deletions; equal-length edits reuse unaffected pieces.

The owner chose to run installed-editor tests locally. The application now includes
the [acceptance worksheet](https://github.com/ArefinAlter/dynodoc/blob/main/extensions/ACCEPTANCE.md)
in each Google test ZIP and links it from Office installation instructions. It
covers all six hosts, exact versions, synthetic inputs, pairing/capture/review/pull,
conflicts, stale application, retries, restart, revocation and historical recovery.
The stale schema-26 installation prerequisite is corrected to 29. Package rebuild,
drift verification and script formatting pass. No native session was connected;
real host results remain not run, and PowerPoint 2021 still fails its API baseline.
No publisher registration, Marketplace submission or public listing is claimed.

VPS remains application `858801c`, schema 29. This increment is source-only; hosted
packages/archive still belong to that deployed release. Shared periodic writes
remain off. The task-owned local PostgreSQL container and its anonymous data
volume were auto-removed after both suites. Docker Desktop was left running
because unrelated containers were active. No local app server was started;
ignored target files retain only synthetic probe/review evidence.

Next concrete dependency: part 2g typed node/value/asset manifests referencing
content roots, with bounded semantic decode (not encoded bytes alone), exact
state/replay equivalence, and explicit versioned publication/read selection. Keep
v1 roots readable and add any schema change as a new migration. Ordinary v1 values
over 1 MiB still reject/fall back; full DocumentState memory is not solved here.
Then packing/incremental publication and shared draft/named-version references.
Portable project-wide commits/parents/atomic refs, durable local history and
resumable exchange follow those contracts; offline convergence, independent signed/
witnessed commitments and native fidelity/large-service acceptance remain open.
Current drafts/branches are per file. This does not complete parts 2-8.

## Source: v1 event hash regression synchronized (11 October 2026)

Engine PR [#7](https://github.com/ArefinAlter/dynodoc-engine/pull/7), source
`e31f093`, merged as `d6be03a`. Application mirror `648aa27` copies the test and
fixture byte-for-byte with contributor attribution. The 22 synthetic events cover
all 15 v1 event types, fixed content/chain hashes, replay state and node Merkle root.
No runtime, event encoding, migration or API behavior changed; no deployment is
needed. The live application remains `858801c`, schema 29, as recorded below.

Local application verification: all six log unit tests, workspace Clippy with
warnings denied, formatting and diff checks pass. Prior independent review of
`e31f093` verified hashes/root and the chain-input-order mutation; its engine CI
passed all 276 Rust tests. Application mirror CI is tracked in
[Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38160929510) and
[frontend](https://github.com/ArefinAlter/dynodoc/actions/runs/38160929481).
Both application mirror runs completed successfully.

Optional follow-up: pin typed serialization round-trips and per-event validate_op
inside the test; both passed in the separate review probe. This regression does
not add independent audit commitments. Next product dependency remains bounded
shared content and historical references, before portable project commits.

## Live: security and runtime safeguards (11 October 2026)

VPS workspace/API/admin run application `858801c`, schema 29. Engine source is
`80cd148` (workload implementation `f718c11`); application security implementation
is `812f97a`, hydration fixture correction `e22a103`, Office command-frame fix
`858801c`. Read [the release, live evidence and compatible rollback](operations/2026-10-11-security-runtime-release.md).
Documentation follow-ups do not change those deployed images/archive.

Deployed protections include fail-closed service auth, server-side logout,
checked immutable snapshots, separate operator migrations/restricted API login,
project policy floors, audited metadata, bounded REST/SSE row reads, expression
limits, committed-tree source packaging, nonce/hash CSP and HTTPS HSTS. Process-local
response admission, account/project expensive-read limits, database deadlines and
container/proxy resource limits are live. See [the exact bounds and gaps](BROWSER-AND-WORKLOAD-SECURITY.md).
The static Office manifest command frame also has a scoped policy and external
initializer; ordinary pages keep their restrictive script/embedding policy.

[App Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38116813490) and
[engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38116978805)
pass all 275 tests each. Deployed Rust inputs are identical to that app run.
[Final frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38118877490)
passes lint/type/build/drift checks, 289 units, 19 public and 72 signed-in browser
workflows, including Nginx regression. Local command-frame/CSP checks pass.
Windows full-run SSE/accessibility timing misses are recorded in the release;
Linux acceptance passes unchanged. Rustdoc passes with 20 existing warnings.

The verified backup/isolated restore, schema 26-to-29 upgrade, actual runtime
privilege denials and compatible API rollback rehearsal pass. Production retains
two documents, 36 events, zero snapshots and identical history fingerprints.
Only API/web/admin restarted; PostgreSQL/converter and unrelated containers were
preserved. Live health/headers, 24 connector delivery/auth checks, committed archive,
large signed-shell headers and anonymous Chromium checks pass. No real user sign-in,
email or document mutation was used. A reload/header-case issue in the deployment
checker was corrected before traffic reopened; no database rollback occurred.

All task-owned local test processes/database and VPS rehearsal containers/volume/
network are removed. Backups and private evidence remain under
`/opt/dynodoc-releases/20261011-security/`. Shared periodic checkpoint writes remain
off. Never use the old schema-26 API as rollback; use the documented schema-29
compatible API with the new web/proxy and preserve current data/credentials.

Next, in dependency order:

1. Part 2: versioned bounded text/value/asset chunks, decode/reference budgets,
   compatibility/reuse fixtures, packing/incremental publication and shared
   draft/version references. Current full-state memory and legacy fallback are
   still unbounded by the new workload admission limits. Keep the writer off
   until its storage/concurrency/peak-memory acceptance passes.
2. Part 3: durable local repositories, portable project-wide commits/parents,
   atomic branch refs and resumable missing-object exchange. Current drafts and
   main selection still belong to individual files.
3. Parts 4–7: native format fidelity, constraint-aware offline convergence,
   independently signed/witnessed audit commitments and measured large-file/
   multi-tenant performance. V1 hashes still omit envelope metadata and cannot
   detect an owner-level complete rewrite without an external commitment.
4. In parallel where contracts are stable: real installed Word/Docs, Excel/Sheets
   and PowerPoint/Slides acceptance and marketplace/publisher setup. Current
   desktop packages need host tests; pane capture needs the pane open, and hosted
   loading/pairing/sync need connectivity. No background/offline completeness or
   hundreds-of-thousands-of-users capacity claim is made.

## Verified source: project policy and audited metadata (11 October 2026)

Project rules now form a minimum for file rules, including old weak overrides.
File ownership/nested-folder management no longer grants project review bypasses.
Rule setters, direct edits, merges/approval overrides, metadata and moves use the
project authority. Source/destination project locks serialize project-root policy
and membership changes against affected writes. Metadata accepts bounded supported
settings patches and records actual changes atomically; settings/archive need governing management and rename follows team-edit
protection. Web import initializes files before attachment to a protected project.
See [the behavior and compatibility contract](PROJECT-POLICY-AND-METADATA.md).

Implementation is pushed to both main branches: application `6a1926f`, browser
regression correction `4dbe3f4`, standalone engine `2100213`. Implementation CI:
[application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38112653973) and
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38113414584)
pass all 269 Rust tests each. The corrected
[frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38113581840) passes
286 units, 16 public browser tests and all 69 signed-in workflows. Lint, builds,
OpenAPI consistency, source packaging, connector drift and proxy regressions pass.
These are implementation references; later documentation commits record evidence
without changing the verified runtime sources.

Local checks pass: 269 Rust tests in each repo, 286 frontend units, five affected
signed-in workflows, lint, builds, OpenAPI/client generation and standalone rustdoc.
Rustdoc retains 20 existing warnings (16 core, four API). The new project-editor
import/protection test also verifies the 80 KiB metadata request cap. Its initial CI
run attached a file while the upload control was disabled during loading; the trace
shows no import request. An enabled-control wait corrects the test, with three
consecutive local repeats and the full CI run passing, without relaxing assertions
or timeouts.

The first local app Rust run hit the existing 100 ms SSE deadline while frontend
lint ran concurrently; the final isolated run passes without changing the deadline.
The standalone parallel build exhausted Windows paging memory; `CARGO_BUILD_JOBS=1`
completed all remaining checks. Run heavy build/lint tasks separately from browser
and timing-sensitive Rust checks. An erasure assertion now includes the metadata
receipt and verifies that it retains no readable title.

Task-owned API/web servers are stopped, and the labelled disposable PostgreSQL
container and its volume are removed. Unrelated services are unchanged. Ignored
logs are under `target/policy-*`; process records are historical and PIDs may be
reused. No production data or real invitation emails were used.

Schema stays 29; no migration, hash or canonical event changes. The VPS workspace/API
remain recorded at `ecbc643`, schema 26, shared periodic writes off. Rollout still
needs session/snapshot backfill/runtime-role upgrade and compatible rollback rehearsal;
old policy-vulnerable API images are not a safe rollback.

Next security source work: CSP/HSTS compatible with Office panes, then expensive
search/deadline/rate/concurrency budgets. Afterwards resume bounded shared chunks,
shared draft/version references, portable project commits/branches, durable offline
exchange/convergence, independent commitments and installed-editor acceptance.

### Next increment acceptance: browser security and resource budgets

In the application repo, inspect `frontend/researcher/svelte.config.js`, its
`src/hooks.server.ts`, connector routes and `infra/demo/nginx-app.conf` together.
Current connector frame ancestors
do not provide a script CSP. Add nonce/hash-based script restrictions and HTTPS-only
HSTS, preserving scoped Office embedding, sign-in/pairing and document exports.
Exercise ordinary pages and all three hosted Office panes in browser/proxy checks;
keep actual installed Office/Google host acceptance explicitly separate. Verify
redirect/error/static responses as well as successful pages, and avoid duplicate
conflicting policies between the app and proxy. Google Apps Script sidebars run in
Google's host; web checks alone cannot establish acceptance there.

Then bound search and expensive operation admission by request size, time and
concurrency, including per-user/project budgets and database statement deadlines.
REST/SSE row pagination already exists; it does not bound event bytes or aggregate
memory. Preserve authorization, cancellation, retry behavior and bounded error
responses. Only after those source gates, resume evolution part 2 storage work.

## Previous increment: restricted database runtime (11 October 2026)

Migration 0029 and a separate operator CLI now remove migrations from API startup.
The API requires a non-owner login with explicit table/function grants and checks
the complete migration version/checksum list without writing. History UPDATE,
DELETE and TRUNCATE, trigger/schema changes, role elevation and migration writes
are denied. Existing audited whole-resource erasure uses tightly scoped trigger
entrypoints; ordinary application authorization remains required.

Implementation is pushed to both main branches: application `880fc29`, standalone
engine `40bd959`. All implementation CI passes:
[application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38095749693),
[frontend and restricted-login browser workflows](https://github.com/ArefinAlter/dynodoc/actions/runs/38095749692),
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38096155982).
CI ran all 266 final Rust tests in each repo, 286 frontend units, 16 public browser
tests and 68 signed-in workflows. Lint, builds, source packaging, connector drift,
proxy regression and OpenAPI consistency checks pass. Rustdoc retains 20 existing
warnings (16 core, four API). These references identify implementation; subsequent
documentation commits record the evidence without changing runtime behavior.

Local application lint/test passes (265 Rust tests plus the final disguised-owner
regression, 286 units and 68 browser workflows). Standalone lint, all 266 Rust tests,
rustdoc and OpenAPI checks pass. Fresh final-schema CLI migration/grant/check and
API startup also passed; generated API specifications are unchanged.
CI browser workflows now provision a separate runtime login. Source deployment
configuration and local tasks distinguish runtime credentials from operator-only
credentials. See [the setup, trust and rollout contract](DATABASE-PRIVILEGES.md).
Owned test servers are stopped; the labelled disposable PostgreSQL database and
volume are removed. Unrelated local services were left running.

Local browser fixtures used the initial schema-29 default-function ACL during
development. The final correction revokes global as well as schema-specific PUBLIC
execution; a future-function regression passes, and a fresh database plus final
API binary passed startup separately. No applied fixture checksum was rewritten.
Final CI exercised the final migration and restricted-login bootstrap end to end,
including all 68 signed-in workflows.
Local logs are ignored under `target/runtime-*` in each repo.

The VPS workspace/API are unchanged. Existing production is still recorded as `ecbc643`,
schema 26. Source requires schema 29, verified snapshot backfill and a provisioned
runtime login. A production-copy upgrade and compatible-image rollback rehearsal
remain required; pre-boundary API images are not safe rollback.

Next source priority: project policy authority/floors and audited metadata, then
CSP/HSTS and resource budgets. The product roadmap remains bounded shared chunks,
portable project commits/branches, durable offline repositories/exchange/convergence,
stronger audit commitments, measured scale and native-editor acceptance/distribution.

### Next increment acceptance

Start with `access.rs::policy_with_source`, `Policy::can_edit_team`/`can_merge`,
`people.rs::set_policy`, `workspace.rs::metadata` and project move/settings paths.
The current file override returns before loading project rules, and document
ownership grants the management bypass independently of the person's project role.
Enforce the project floor and authority together, including existing weak overrides,
file creators who are only project editors, direct writes and reviewed merges.
Cover moving files/folders between projects and concurrent policy changes; a
protection-flag OR alone does not close the role bypass. Keep capability/policy-source
responses and the sharing UI consistent with server enforcement.

Inventory settings consumed by current adapters before narrowing metadata writes.
Specify field-level authority and bounded schemas, preserve permitted editor flows,
and record actual before/after changes in the same transaction. Verify denied/no-op
requests and audit rollback, plus a project-editor/file-owner regression. Mirror
the engine changes and regenerate OpenAPI/types only if the public contract changes.

## Latest source work: verified legacy snapshots (11 October 2026)

Security increment implemented: migration 0028 protects legacy snapshots and
adds immutable replay-verification receipts. New snapshots use a canonical checked
writer; current/historical state, public copies, deployed downloads and restore
use checked reads. API/CLI audits compare every legacy snapshot against event
replay. An explicit per-document/all-document backfill verifies old states without
rewriting them. Normal reads retain checkpoint-plus-tail work; fingerprints stream
without another encoded state buffer. See [the contract](VERIFIED-SNAPSHOTS.md).

Application `task lint` passes (Svelte: zero errors/warnings). `task test` passes:
258 Rust tests, 286 frontend units and all 68 browser workflows. The final strict
unknown-field regression and receipt index also pass the 15-test snapshot suite
and final clippy check; there are now 259 distinct Rust tests. The schema-27 upgrade,
backfill atomicity, erasure and historical/public/restore corruption checks pass.
Standalone `task lint`, `task test` (all 259 Rust tests), `task docs` and OpenAPI
generation pass. Rustdoc reports 20 existing warnings (16 core, four API). CLI
backfill verified all 52 synthetic browser-test documents; an independent audit
also passed for a document with a stored snapshot. Owned browser-test servers are
stopped and the labelled disposable database/volume is removed. This is source
verification, not a production release.

Implementation is pushed to both main branches: application `6ccd5f3`, standalone
engine `e8fe900`. All source CI passes:
[application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38076177189),
[frontend and signed-in workflows](https://github.com/ArefinAlter/dynodoc/actions/runs/38076177211),
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38076557853).
CI ran all 259 final Rust tests in each repo, plus frontend checks, 16 public browser
tests and 68 signed-in workflows. These references track implementation; the later
documentation-only commits record this evidence without changing runtime behavior.

All 16 changed Rust/migration files are mirrored; the standalone's existing Merkle
proof style, test fixture paths and MIT/API package metadata are preserved. Local
logs are ignored under `target/snapshot-*`; CI provides the durable check record.
No VPS workspace/API deployment has occurred; the last recorded VPS is still
`ecbc643`, schema 26, with shared periodic writes off. Upgrade must include both
schema-27 login revocation and schema-28 snapshot verification/backfill; an
unmodified old API is not a compatible rollback.

Next security gate: separate migration-owner and runtime database credentials and
exercise forbidden operations under the real runtime role. Then project-policy
floors/authority, audited metadata, CSP/HSTS and resource budgets. Resume bounded
chunks/assets/shared versions, portable project commits/branches, durable offline
repositories and convergence afterwards. All six native hosts still need actual
installed acceptance; marketplace submission/background capture remain unfinished.

## Current security increment: authentication and bounded history (10 October 2026)

The security findings are being addressed before the next storage increment.
Current changes: fail-closed token issuance; server-side logout with migration 0027;
bounded REST/SSE history reads; expression admission limits and iterative cycle
traversal; committed-tree source archives; Google shared-helper drift checks;
OpenAPI CI and JSON-LD escaping. Application `task lint`, production build and
`task test` pass: 248 Rust tests, 286 frontend unit tests and 68 signed-in browser
tests. The browser regression confirms copied-cookie revocation and preservation
of a separate login. Source packaging and generated connector checks also pass.
Standalone `task lint`, `task test` (248 Rust tests), `task docs` and API generation
also pass; rustdoc retains 16 existing link/HTML warnings. All 17 changed Rust and
migration files match across repos; standalone fixture paths and package metadata
are preserved. Implementations are pushed to main: application `c9d4994`, standalone
engine `2e75859`. [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38070661483)
and [standalone CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38071036482)
passed. [Frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38070661567)
passed build/unit/public-browser checks, but its first authenticated run passed 67
of 68 tests: the existing personal-draft typing test inserted its sentence before
the second paragraph. The new copied-cookie logout test passed. The failed-job
rerun (attempt 2) passed all authenticated workflows; the overall workflow is now
green. Retain the initial failure as an unresolved intermittent editor/test issue.
See [the full findings matrix and rollout contract](SECURITY-HARDENING.md).

The retained browser trace shows cursor movement before the first autosave. Twenty
local diagnostic repetitions passed, including CPU throttling and clicking at the
paragraph start before Control+End. The exact cause is unconfirmed. No editor
change, artificial wait, weakened assertion or retry policy was introduced to mask
the failure. If it recurs, instrument DOM and ProseMirror selection around focus,
Control+End and scheduled layout; do not assume an autosave regression from the
test name alone. Ignored local evidence is under `target/security-ci-artifact/`
and `target/security-cursor-*.json`; CI retains the original failure artifact for
seven days. Preserve it before expiry if follow-up needs the browser trace.

Local verification setup initially hit Windows' lock on the running Cargo output;
running a separate copy of the API executable allowed the complete test rerun to
pass. All subsequent tests used synthetic local identities/documents. Owned API/web
processes and both disposable database instances/volumes used during verification
are removed. Existing `dynodoc-postgres` and `margin-qdrant` containers are preserved.

Highest-priority remaining security work: legacy snapshot verification/immutability,
runtime database privilege separation, project policy floor and audited metadata,
CSP/HSTS and remaining resource budgets. Then resume bounded shared storage,
portable project commits/branches, offline convergence and installed-editor acceptance.
This increment is not deployed to the VPS; the workspace/API remain ecbc643/schema
26. The new API needs schema 27. Rollback must retain login revocation enforcement
or deliberately invalidate credentials; an unmodified old API is not safe rollback.
No real production sign-in, email or document mutation was used.

## Source increment: bounded checkpoint encoding and desktop distribution (10 October 2026)

Implementation commits pushed to main: application `2f69c87`, standalone engine
`3ac2ad6`. CI evidence: [application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38066279174),
[frontend and signed-in workflows](https://github.com/ArefinAlter/dynodoc/actions/runs/38066279185),
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38066287205).
These links track source verification; a later documentation-only commit records
the references without changing implementation or deployment.

Part 2e serializes borrowed checkpoint values into a capped output buffer and
validates canonical bytes by streaming comparison. It removes full-value clones,
intermediate JSON trees and the reader's second encoded buffer without changing
v1 objects, roots, events, API schemas or migrations. The 1 MiB object limit and
remaining operation byte budget apply before output growth. Five new compatibility,
property and boundary tests cover the byte contract. See
[the contract and measured allocation trace](BOUNDED-CHECKPOINT-ENCODING.md).

The application installer and `/connectors` source now distinguish Windows/Mac
desktop from Office web, local files from hosted panes, and test distribution from
publication. Google ZIP packages include the revised instructions. Read-only local
inventory found Office 2021 version 16.0.14326.20348: PowerPoint lacks the required
PowerPointApi 1.4; Word/Excel still need actual runtime acceptance. No host was
sideloaded, account signed in, publisher registered or listing submitted. See
[publication requirements and the host acceptance matrix](ADDIN-PUBLICATION-AND-ACCEPTANCE.md).

Verification: all 238 Rust tests pass in each repository, including five new codec
tests; both lint checks and Rust formatting pass. Frontend units: 282 passed;
separate production build and all 16 public browser checks passed (51 signed-in
cases skipped locally). Connector package consistency passes. Standalone docs build
passes with existing rustdoc link warnings. Mirrored Rust files match byte-for-byte.

The initial `task test` completed Rust and frontend units but stayed in Vitest watch
mode; the test script now uses `vitest run` correctly. A browser build timed out
under competing local checks; a separate build and direct installed browser runner
passed. The global pnpm CI auto-install also stopped on existing dependency-build
policy; no policy was relaxed and its incidental workspace-file additions were
reverted. One standalone run missed the existing 100 ms SSE deadline under load;
the complete quiet 238-test rerun passed, including SSE. Owned preview/watch
processes and the labelled test database/volume are removed; existing local
containers are preserved. Source commits and CI references are above.

This source increment has not been deployed. VPS remains application `ecbc643`,
schema 26, with shared periodic writes disabled. Existing deployment/rollback
evidence in the following section remains current. The new instructions become
hosted only after a future web deployment.

Continue with:

1. Versioned, bounded text/value/asset chunks and reconstruction tests. Then
   packing/incremental publication and shared named-version/draft references.
   Current values above 1 MiB still fail shared encoding; full materialized state,
   property-key metadata and legacy fallback are not newly memory-bounded.
2. Part 3 portable project commits/parents/branches and durable local repositories,
   followed by resumable exchange, format fidelity and offline convergence. Current
   drafts still belong to individual files; a queue is not an offline commit graph.
3. Run the installed-host matrix on compatible Office and real Google add-on hosts
   using test identities. PowerPoint on this workstation needs a supporting host
   version or the existing edited-file fallback. Complete publisher identity/listing
   preparation before any marketplace submission. Capture still needs an open pane;
   loading, pairing and remote sync need connectivity.

## Source increment: Projects front page and member profiles (10 October 2026)

Application `6f1ac21` and engine `8eef6dd` implement the Projects-first interface: minimal
profiles visible only to shared Project members, repository file lists, read-only
file previews, per-file main/private-draft selection, pull requests, history and
recovery, with the existing editors under a separate Web editor tab. Migration
0026 adds private profile metadata and revision checks. Original uploaded files
are limited to 3,000,000 bytes in the browser, API and conversion/import services.
Derived provenance envelopes and expanded archive safety budgets are separate.

Both source commits are pushed to main. All 233 Rust tests pass in each repository;
standalone lint, docs and API generation pass (rustdoc retains existing link
warnings). All 282 frontend unit tests, 16 public and 67 signed-in browser tests
pass, with clean Svelte/ESLint/format/build checks. Profile tests cover absent/shared/
removed membership, disabled accounts, email omission, stale saves and preserved
names after sign-in. Upload tests cover exact 3 MB, excess bytes, request-body
bounds and denied writers without extra stored uploads. Matching Rust/migrations
and API schemas are mirrored into the standalone engine.

The production-mode browser journeys include external edits/review/revocation
and profile setup, multiple files, file moves, draft comparison, merge, historical
native download and selected recovery. Desktop/mobile layouts were inspected and
table contrast fixed. The final read-only Word preview has no editing ribbon;
its build and browser assertion pass. Account erasure clears the bio and invalidates
the profile revision, with a passing regression. Production Nginx configuration
tests pass for Projects, profile and file routes, including the large auth-header
fixture. Source CI: [application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38061246753),
[frontend and signed-in stack](https://github.com/ArefinAlter/dynodoc/actions/runs/38061246764),
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38061236800).

Local admin failures from starting the API without ADMIN_EMAILS and reusing a
rate-limited synthetic account were resolved in the isolated test environment.
The unchanged 100 ms SSE test also passes in a quiet local run and both source CI
suites. All owned API/web servers are stopped; the labelled disposable database
and its volume are removed. Existing local containers were preserved.
The source is now deployed as application `ecbc643`, schema 26, at 21:22 Asia/Dhaka
(15:22 UTC). The VPS backup was validated before activation; an isolated synthetic
upgrade/compatible-rollback/re-upgrade rehearsal passed, including retained profile
edits and erasure cleanup through the rollback API. Only API/web/admin/converter
were replaced; PostgreSQL, Nginx and all 18 unrelated containers were preserved.
All 17 live workspace and 22 connector checks pass, plus a clean browser check.
Existing document/event counts and their aggregate event fingerprint are unchanged.
No real production sign-in, email or installed-editor acceptance was used.
Project-wide portable branches/commits remain part 3; current drafts belong to one
file. Shared checkpoint writes remain disabled and part 2 storage work is pending.
The schema-25 deployment below is historical. Current release evidence, backup and
compatible rollback instructions are in the application
[deployment record](https://github.com/ArefinAlter/dynodoc/blob/main/docs/operations/2026-10-10-projects-workspace-release.md).
See also [workspace source details](PROJECTS-WORKSPACE-RELEASE.md).

Continue with:

1. Resume part 2 bounded values/assets, compression/incremental publication and
   shared draft/version references. Then implement part 3 portable commits/parents,
   project-wide branches and resumable exchange; retain the fidelity, offline,
   independent audit and measured-scale gates in the evolution plan.
2. Run installed-host acceptance for Word/Docs, Excel/Sheets and PowerPoint/Slides
   alongside independent engine work when host installations/test accounts are
   available. Development packages and browser tests are not host approval.

## Previous deployment handoff: history and editor installations (10 October 2026)

That matching API/web/admin release ran on the VPS as application `536a448`
(implementation `0478bf2`), schema 25, activated at 13:36 UTC. The standalone
engine source remains `a44b8ad`; documentation head `7bfd16c` also passed CI.
This increment changes deployment/operations documentation and adds the app's
read-only `infra/demo/check-connectors.py`; no Rust, event or codec changes.

Validated backup, all applied migration checksums and previous images/source were
retained. A disposable empty database rehearsed the actual previous API at schema
24, new API upgrade to 25 and previous-source-plus-0025 rollback API on schema 25.
The unmodified old API was confirmed to reject the newer migration. All labelled
rehearsal containers/network and their temporary credentials are removed.

Live checks: all 22 delivery/auth checks pass, including exact six-package/icon/
command bytes, pane frame headers, protected return URLs and invalid-key rejection.
A clean browser shows six downloads and approval-to-login without page errors.
Synthetic page shells pass directly and through Nginx with identical preload links.
API/web/admin are healthy. Existing counts/event fingerprint are unchanged;
PostgreSQL, converter and all 18 unrelated containers retain their IDs/images/start
times. Nginx is unchanged. Shared checkpoint writes remain disabled.

Installation page: https://app.dynodoc.online/connectors. Detailed evidence and the
rehearsed rollback command are in the application
[release record](https://github.com/ArefinAlter/dynodoc/blob/main/docs/operations/2026-10-10-history-connector-release.md).
The deployed source archive stays at `536a448`; later handoff/checker commits are
documentation/operations updates and are not rebuilt application images.

Continue with:

1. Installed Word/Docs, Excel/Sheets and PowerPoint/Slides acceptance using test
   accounts/files, including browser launch, consent, two users, push/review/pull
   and revocation. No real production sign-in/email or native-host acceptance was
   performed in this rollout. Marketplace/legal/publisher gates remain open.
2. Independently resume part 2 bounded text/assets, compression/incremental
   publication and shared named-version/draft references, retaining v1 readers
   and old histories. Current full-state memory and legacy bases remain limits.
3. Portable commits/parents/branches, local repositories and resumable exchange,
   then format fidelity, offline convergence, stronger audit and measured scale.

The source verification/feature details below remain applicable. Deployment alone
does not improve algorithmic complexity or certify native-editor fidelity.

## Latest increment: history ranges, native exports and editor sign-in (10 October 2026)

Implemented and pushed to both main branches. The application offers net revision-range
revert/cherry-pick with explicit overlap choices, historical DOCX/XLSX/PPTX downloads,
and browser sign-in/file approval in all six editor clients. Microsoft ribbon commands,
icons and three XML manifests plus three reproducible Google Editor add-on ZIP packages
are available through the application `/connectors` page and Sync & connections.

Final implementation commits:
- Application: `0478bf2fe3e9c4c6e171ec4fb38e61580c788a7a` (feature implementation `4df76ad`).
- Standalone engine: `a44b8adac296aa6414bcafa12c56f3a4bd446e73` (feature implementation `e26d51c`).

No migration, event encoding, hash, storage codec or canonical-history rewrite.
Minimum schema remains 25; shared writes remain opt-in. At source completion there was
no VPS deployment. The then-recorded
production was 8c7461d/schema 24 plus the earlier Nginx repair; it did not have these
new endpoints/assets. This is superseded by the deployment handoff above.
The standalone repository contains the Rust API and contracts; native exporters and
installation/ribbon/sidebar clients live in the application repository.

### Behavior and boundaries

- Restore takes selected blocks from one retained revision. Revert/cherry-pick uses the
  selected net content delta over `(from_seq, through_seq]`, then three-way merges it
  into the current team state. Old endpoints/anchors and explicit conflict choices
  are recorded by the server. Permission, stale-head, retry, dependency and ordinary
  review/approval checks apply. Private drafts do not change canonical content.
- Native downloads reconstruct the requested checkpoint's supported content and include
  document/revision origin metadata. Unsupported original formatting/assets and exact
  original bytes are not reconstructed. No native-host fidelity certification was run.
- Editor sign-in keeps a random 256-bit bearer secret in the requesting client; the
  approval browser sees only its hash and approves a particular file/host. The scoped
  connection cannot merge or manage access. Current membership, account/session state,
  expiry and revocation are rechecked. Links last ten minutes; server validation allows
  one minute for an ahead-running browser clock. Grants last seven days. No account-wide
  token, refresh token or service key is passed to the editor or approval URL.
- Office keys stay in pane memory; Google approved keys use existing private user/file
  properties. Capture is explicitly enabled and still limited to supported paragraph,
  cell/formula/date and shape-text observations while the pane is open. Durable local
  commits, background sync and arbitrary formatting/structural capture remain open.

Exact contracts: [history operations](HISTORY-RECOVERY.md),
[connection and installation](CONNECTOR-PAIRING-AND-INSTALLATION.md).
[Tester installation instructions](https://github.com/ArefinAlter/dynodoc/blob/4df76ad789038dc208cb0ea9a5dc2a4137b28133/extensions/INSTALL.md).

### Verification

Application fmt/Clippy and all 231 Rust tests pass locally. After the final clock fix,
all six connector API tests pass again, including ahead-clock approval, expired and
too-distant link rejection. Standalone task lint, all 231 tests, docs and regenerated
OpenAPI pass on the final source; existing rustdoc warnings remain (16 core / 4 API).
Four changed Rust source/test files and all 22 unchanged SQL migrations match byte for
byte across repositories. OpenAPI schemas match with intended MIT/contact metadata in
the standalone distribution and AGPL metadata in the application.

Frontend lint, zero-diagnostic Svelte check, build and connector artifact consistency
pass. Local unit coverage: full 277-test suite plus three added Google pairing bridge
tests (also reran shared pairing tests). Four real signed-in browser journeys pass:
auth approval after login; DOCX download/recovery/review/merge/range controls; pinned
XLSX export; pinned PPTX export. Seven existing Office/Google connector bridge browser
scenarios pass. All three Office XML manifests pass Microsoft's official validator.
Its minimum manifest version is 1.0.0.0; this does not establish marketplace approval.

Initial feature source CI passed:
- [Application Rust: 231 tests](https://github.com/ArefinAlter/dynodoc/actions/runs/38053822154).
- [Application frontend: 280 unit, 16 public and 66 signed-in browser tests](https://github.com/ArefinAlter/dynodoc/actions/runs/38053822926).
- [Standalone engine: 231 tests, lint/docs/OpenAPI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38053895976).

Final clock-only follow-up CI also passed (231 Rust tests per repository,
280 frontend unit tests, 16 public browser tests and 66 tests against the authenticated stack):
[application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38054623731),
[frontend/browser](https://github.com/ArefinAlter/dynodoc/actions/runs/38054623648),
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38054667212).
The initial range test used wrong merge request field names and was corrected; the
DOCX browser assertion was corrected to read the exporter's existing custom properties.
No exporter fix was needed. No large-file/load benchmark or installed-host acceptance
was performed for this increment; time/space complexity remains unchanged.

### Cleanup and continuation

Both owned disposable containers/volumes (dynodoc-connect-history-20261010 and
dynodoc-pair-clock-20261010) were removed after task-label/loopback-port checks.
Owned API/web processes 14912/9260 were stopped after PID/port verification. Ports
5553, 8087 and 4178 are closed; existing dynodoc-postgres and margin-qdrant are untouched.
Synthetic-only test logs remain in system temp under dynodoc-connect-history-*,
dynodoc-pair-clock-* and dynodoc-*-manifest.log. No production email or document was used.

Next work recorded at source release (deployment is now complete):
1. Deploy the matching API/web build to a pilot environment with backup/schema checks;
   run installed Word/Docs, Excel/Sheets and PowerPoint/Slides acceptance, including
   browser launch, Google consent, ribbon/menu commands, two accounts and revocation.
   Store publisher setup, legal/listing assets, OAuth review and publication remain open.
2. Resume part 2 bounded text/assets, compression/incremental publication and shared
   named-version/draft references while retaining old histories and v1 readers.
3. Add portable commit identities/parents, branch refs, durable local repositories,
   resumable object exchange and checked commit-based revert/cherry-pick. Current
   commands operate on centralized revision ranges; indexed date/author/message history
   discovery and field/run-level selection are also still needed.
4. Continue format/opaque-asset fidelity, offline replica convergence, signed/witnessed
   provenance and measured large-file/concurrent-service acceptance. These are explicit
   remaining product gates, not capabilities inferred from the current tests.

## Historical recovery increment (10 October 2026)

Complete and pushed to both main branches, with local and source CI checks passing.
Following the owner's clarification, historical content retrieval and
recovery are explicit product requirements under current permissions. The product
specification distinguishes historical download, selected restoration, commit
revert and cherry-pick. This increment implements centralized whole-block recovery;
portable commits, revert and cherry-pick remain future work.

Implementation: account-authenticated history-recovery preview/private-draft API,
server-derived source revision/hash, checked current base, idempotent UUID retries,
ordinary review/merge policies, Version history selection/download/draft controls,
Activity entrypoint and source display in review. No migration or canonical event
change. Existing shared checkpoint writers remain opt-in; no VPS deployment.

Application fmt/Clippy and the full 229 Rust tests pass. All five review integration
tests passed again after reserving the server-verified recovery source label (three
are new recovery regressions). Frontend lint, 274 unit tests, production build and
Svelte validation with zero diagnostics pass. The new signed-in browser workflow
passes locally through download, private recovery, submission and actual merge.
OpenAPI and TypeScript definitions are regenerated. Four API source/test files and
all 22 unchanged migrations match the standalone repository byte for byte.

Standalone task lint, all 229 Rust tests, docs and OpenAPI pass; documentation
retains the existing 16 core / 4 API link warnings. Source CI passes all 229 Rust
tests in each repository. Application CI passes 274 frontend unit tests, 16 public
browser tests and all 63 signed-in workflows, including the new recovery journey
and the production proxy regression.
Initial sandboxed frontend check could not spawn esbuild; rerunning with subprocess
permission passed. No source/test failure remains. No native host acceptance or
large-file/performance benchmark was run for this UI/API increment.

The owned loopback PostgreSQL container dynodoc-history-20261010 and its disposable
volume were removed after label/port verification. Existing containers remain untouched.
Owned API/web processes on 8087/4178 have been stopped after the browser check.
Logs are in system temp under dynodoc-history-*; fixtures are entirely synthetic.

Starting main heads: application de49ccf; standalone b05796f. Application source
`d83bf86782380860788a5edadf4bda342008031a` and engine source
`fdba931985cc8aa34f57f535a597842fcb8f6cb1` are pushed to main. This documentation
follow-up records completed checks without changing the tested source.

- [Application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38050861763): passed, 229 tests.
- [Application frontend/browser](https://github.com/ArefinAlter/dynodoc/actions/runs/38050861752): passed, 274 unit / 16 public / 63 signed-in tests.
- [Standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38051048267): passed, 229 tests plus lint/docs/OpenAPI.
- Final engine handoff/status: `be355d1bde86740e4babe7d8245e53b2f498c76f`
  ([checkout CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38051507307));
  source is unchanged from the verified implementation commit.

Next: resume part 2 bounded text/assets, compression/incremental construction and
shared version/draft references, followed by local commit exchange. Preserve old
revision recovery while migrating storage. Indexed history discovery, commit
revert/cherry-pick and native historical export/application remain explicit future
gates. No VPS deployment; last recorded production remains 8c7461d/schema 24 plus
the documented Nginx repair. Mixed readers require schema 25. See
[the exact recovery contract](HISTORY-RECOVERY.md).

## Part 2d: concurrent content writes during checkpoints (10 October 2026)

Part 2d is complete, synchronized and pushed to both main branches. Full local
checks and source CI passed in both repositories, including application browser
workflows. Shared writes remain opt-in. No new migration, event/codec/hash bytes,
HTTP schema or VPS deployment; schema 25 remains required for mixed readers.

### Implementation and correctness

Core append and the API begin_write entrypoint take NO KEY UPDATE on the document.
Writers still serialize before reading permissions, state, constraints and the
next event sequence; core no longer upgrades the API lock. Checkpoint KEY SHARE
guards can coexist with those writers while retaining protection against erasure.
Checkpoints select immutable history through pinned sequence N, never mutable node
rows, so later appends form the suffix rather than contaminating checkpoint N.

Stronger SHARE/UPDATE locks remain on the existing authorized export, management,
project and erasure paths. Membership changes through begin_write retain the same
exclusive writer serialization. A queued stronger operation can still delay writers.
Graph verification, object/manifest publication, rollback and erasure remain in
one transaction; no staged objects or new crash-cleanup protocol is introduced.
[The audited lock table and primary PostgreSQL reference](CHECKPOINT-CONCURRENCY.md)
document the precise paths and limitations.

### Verification and measurement

Application fmt/Clippy, all 226 Rust tests and unchanged OpenAPI pass. Tests pause
real explicit-shared, periodic-shared and legacy publishers before commit, then
require ten concurrent core writers to finish while publication stays uncommitted.
Consecutive sequences, chain integrity, old checkpoint content and replay of all
ten later edits are checked. Strong UPDATE locks still fail NOWAIT at the guard.
The real ten-collaborator API test now runs under a checkpoint guard. A new test
observes a real lock wait, commits membership revocation, and verifies that the
queued API writer is forbidden without appending any event. Existing access,
erasure, merge, snapshot, codec/corruption and stale-write suites still pass.

Standalone task lint, all 226 Rust tests, task docs and unchanged OpenAPI pass.
Rustdoc retains 16 core / 4 API pre-existing link warnings. All seven changed Rust
files and all 22 immutable migrations match byte-for-byte. MIT packaging remains
unchanged. Source CI passed all 226 Rust tests in each repository. Application
frontend CI passed 274 unit tests, 16 public browser tests and 62 signed-in workflows,
including the production proxy regression. No real native-host acceptance was run.

Controlled core benchmark: separate fresh databases, identical 8,000 fixed-ID
blocks / 6.4 MB initial state, ten concurrent actor transactions. Prior entry lock
commit times: 1,925-2,035 ms (median 1,987); compatible lock: 38-212 ms (median 129).
Zero old-mode commits versus all ten current-mode commits completed before the
publisher returned. Both roots match, all edits survive and chains verify.
Publication after the barrier release still takes about two seconds. Client peaks
were 104.38/100.59 MiB, excluding PostgreSQL. These one-run traces exclude API
full-state materialization/authorization/validation and native/network work; they
are not service latency percentiles or a large-file/scale acceptance result.
Raw artifacts and reproduction are in [CHECKPOINT-CONCURRENCY](CHECKPOINT-CONCURRENCY.md).
The old database spike example now calls its FOR UPDATE probe strong_lock_wait_ms;
historical artifacts keep their previous append_lock_wait_ms meaning.

### Next concrete work

1. Specify and implement versioned bounded text/asset chunks while retaining v1
   readers and old histories. A single encoded object over 1 MiB is still rejected;
   periodic fallback still stores full JSON. Measure expanded/native-file memory.
2. Evaluate compressed packing/backend options and incremental state construction.
   Full API/checkpoint materialization and CPU/IO scans remain; measure concurrent
   API workloads and longer/edit-dense histories before enabling shared writes.
3. Replace full named-version and draft-base copies with versioned shared references,
   then add durable local commit parents/branches and resumable missing-object
   exchange, with cross-language canonical conformance before portable exchange.
4. Offline convergence, signed/witnessed provenance, structural/formatting/background
   capture in all six native hosts, marketplace and large-service acceptance remain
   later gates. Do not resume Office web-editor parity or the civic roadmap.

### Publication and environment

Starting app c64a9ff; standalone 11399df. Both implementation commits are pushed
to main; this documentation follow-up records completed checks without changing
the tested source.
Owned container dynodoc-evolution-part2d-20261010 and its disposable volume were
removed after label/loopback-port verification. Existing dynodoc-postgres and
margin-qdrant remain running. Logs are in system temp; two synthetic JSON artifacts
are tracked. No local API/web servers started and no VPS deployment. Last recorded
production remains 8c7461d, schema 24, plus the documented Nginx repair.

Publication:

- Application source: `c19dbdab8c4e0e724e6510cf475fa97fcd9a56ac`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38048638516): passed, 226 tests.
- [Application frontend/browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38048638534): passed, 274 unit tests, 16 public browser tests and 62 signed-in workflows.
- Standalone source: `cc12b29e0197304cdfebb86ee1c16d5653b800e4`.
- [Standalone engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38048849334): passed, 226 tests.
- Final engine handoff/status: `b05796fb85003ee6f87278853d8d1d57a5bd5d2c`;
  [final checkout CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38049284279)
  passed. Source is unchanged from the verified implementation commit.

## Part 2c: bounded database batches (10 October 2026)

Part 2c is complete, synchronized and pushed to both main branches. Local checks
and both source CI runs passed, including the application browser workflows.
Part 2 remains open and shared writes remain off by default. No new migration,
event encoding or HTTP schema; schema 25 remains required by mixed readers.

- Writes buffer at most 64 objects / 1 MiB encoded bytes, plus the current encoding
  and input/driver state. PostgreSQL checks canonical input and exact stored bytes
  in a bounded VALUES join returning booleans, inserts missing objects in one
  statement, and rechecks conflicts won by concurrent inserts. Unchanged batches
  issue no INSERT; duplicate addresses count as new at most once.
- Index traversal uses breadth-first batches of up to 16 objects, as do value
  reads. Route/depth/duplicate/subtree checks and byte/object budgets remain;
  queued references count against the remaining object budget before expansion.
  Complete state/index/traversal metadata still resides in memory.
- Explicit publishers acquire the same document advisory lock as periodic jobs
  before the row lock, serializing overlapping historical graphs and preventing
  cross-batch lock-order deadlocks. Periodic jobs keep try-lock behavior. Objects
  and manifests remain in one verified transaction with rollback/erasure guarantees.
- The row KEY SHARE lock still blocks FOR UPDATE appends through publication.
  Batching shortens work but does not provide staged/unlocked or incremental
  publication. V1 bytes, roots, document scope and full reconstruction are unchanged.
  ObjectStore now requires Send, which graph operations already required.

### Verification and measured limits

Application fmt/Clippy, all 224 Rust tests and unchanged OpenAPI pass. New database
regressions cover bounded bulk writes, duplicate reuse/scope/capacity/corruption,
2,000-node index batching with byte-triggered flushes, overlapping historical
publishers, and an invalid concurrent INSERT winner checked after a real database
lock wait. Oversized-value fallback now proves rollback after an actual batch has
reached the database. Existing hash vectors and event/graph equivalence tests pass.

Standalone source is synchronized; task lint, all 224 Rust tests, task docs and
unchanged generated OpenAPI pass. The full suite passed on its first run here.
All five changed Rust files and all 22 immutable migration files match byte for
byte. Existing Rustdoc link warnings remain. MIT/OpenAPI packaging is preserved.
Remote source CI passed in both repositories (224 Rust tests each). Application
frontend CI passed 274 unit tests, 16 public browser tests, 62 signed-in workflows
and the production proxy regression. Source/CI records are linked below.

Four separate fresh PostgreSQL databases: 8,000 fixed-ID blocks, initial 6.4 MB
state, two one-block edits per trace; identical 600-byte repeated or varied payload
lengths. Individual/batched paired roots and canonical totals match, reconstructed
states match event application, and event chains verify. Publication fell from
14.6-20.9 s to 2.1-2.9 s; the competing append-lock wait remains 2.1-2.9 s. Object
INSERT statements: 9,817 each checkpoint becomes 154 initially and 4/3 for edits.
Verified graph SELECTs: 9,817 becomes 619. This compares individual SQL and batched
SQL using the current codec, not different release binaries.

Physical shared relations are about 10.9 MB. Three compressed legacy snapshots
use 0.64 MB for repeated content and 20.78 MB for varied content. Canonical bytes
alone do not predict disk savings. Client process peaks were 91.0-104.8 MiB,
excluding PostgreSQL. The four one-run, three-checkpoint traces are not percentile,
ten-collaborator, native 100-500 MB or service-scale acceptance. Exact results,
methodology and four portable raw artifacts are in [SHARED-CHECKPOINTS](SHARED-CHECKPOINTS.md).

### Next concrete work

1. Reduce the publication lock window: audit lock-mode requirements first, then
   design incremental/staged publication where necessary. Preserve append/access
   serialization, pinned sequence/chain validation, complete-root verification, erasure
   isolation and crash/retry cleanup before moving writes outside one transaction.
   Measure hot-document contention; keep the writer opt-in until this passes.
2. Specify bounded text/asset chunks and evaluate compressed packing/backend
   alternatives with edit-density and longer-history traces. Current objects still
   reject a single encoded value over 1 MiB; fallback retains a full JSON checkpoint.
3. Migrate named versions and draft bases to explicit versioned shared references,
   then add parent/root/actor commit envelopes, durable local history and resumable
   missing-object exchange. Cross-language canonical conformance precedes exchange.
4. Offline convergence, signed/witnessed provenance, all-six-host structural and
   formatting/background capture, native large-file and service-load acceptance
   remain later gates. The basic web editor is not the product priority.

### Publication and environment

Starting application: 5ce3566; standalone: 2d93630. Both implementation commits are
pushed to main. This documentation follow-up records verified results without
changing the tested source. The owned local container
dynodoc-evolution-part2c-20261010 and its disposable volume were removed after
validating its ownership label and loopback port 5553. Existing dynodoc-postgres
and margin-qdrant remain running. Logs are in system temp; synthetic JSON artifacts
are tracked. No API/web server or VPS deployment. Last recorded production remains
8c7461d, schema 24, plus the documented Nginx repair.

Publication:

- Application source: `40a80be7e11ae8b10add920c787573fab822d0f9`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38046287959): passed, 224 tests.
- [Application frontend/browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38046287986): passed, 274 unit tests, 16 public browser tests and 62 signed-in workflows.
- Standalone source: `c8ac6bcb108419cf40fb20199c6e3797353f197c`.
- [Standalone engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38046494935): passed, 224 tests.
- Final engine handoff/status: `11399dfd5525d86d65dcf57c84b1cb54bdf4c882`
  ([checkout CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38046840829)); source is unchanged.

## Part 2b: mixed readers and gated automatic writer (10 October 2026)

Part 2b is complete, synchronized and published to both main branches. Source CI
passed in both repositories, including application browser workflows. This is a
slice of part 2, not product completion.

- Current/history readers select the nearest legacy/shared checkpoint (shared wins
  ties), validate shared graphs and stream only the suffix. Pinned current seq
  remains the SSE resume boundary. API write materialization uses the same reader
  inside existing access checks and locks. Selected corrupt shared data fails.
- Automatic writes default to legacy. API background jobs opt in via
  DYNODOC_PERIODIC_CHECKPOINT_STORAGE=shared-v1; core exposes PeriodicStorage.
  Both formats count toward cadence; per-document advisory locking skips duplicate
  builders. No-op jobs check cadence before locking, then due jobs recheck it.
- Shared capacity errors roll back partial objects at a savepoint before legacy
  fallback at the same seq. Other errors propagate. Named/deployed snapshots,
  snapshot audit IDs and draft/merge bases still contain full JSON.
- PgStore prefetches at most 16 values, scoped by document; ordinary validation and
  budgets remain. Latest-batch cache has a 16 MiB encoded-byte maximum, not a whole
  request memory bound. Index entries/state still reside in memory.
- Explicit operator publication still derives from legacy/event reconstruction,
  independent of shared roots. No new schema/event/HTTP format; schema 25 required.
- Publication holds KEY SHARE on the document, which blocks the API's FOR UPDATE
  appends, and still scans/writes individual objects. Keep writer default off until
  measured publication latency and concurrent acceptance improve.

Verification: all 220 application Rust tests, fmt/Clippy and unchanged OpenAPI
passed. Targeted tests cover every revision, mixed nearest/tie/scope selection,
resume seq, duplicate builders, cadence/rollback, oversized-value partial rollback,
batched duplicates/budgets/scope and corruption. Existing API workflows now run from
shared state for stale edits and version/draft/restore compatibility. Standalone
task lint, all 220 tests and task docs passed (existing Rustdoc link warnings).
Its first full run hit the existing 100 ms SSE deadline once; the full rerun passed
without changing the test. That timing check is not a production latency guarantee.
Standalone generated OpenAPI is unchanged, including MIT metadata. Both copies of
all seven changed Rust files and all 22 immutable migrations match exactly.
Counterpart source commits and successful CI runs are linked below.

PostgreSQL spike: 8,000 blocks / six checkpoints, all reconstruction/chain checks
passed. Batching lowers graph object SELECTs 9,885 -> 2,385 and median read
6,425 -> 1,831 ms; direct legacy JSONB read is 117 ms. Shared edit publication
15.3-17.4 seconds blocks appends for much of that transaction. Shared relations
11.26 MB versus compressed legacy snapshots 2.92 MB on this repetitive fixture,
despite fewer canonical bytes and edit WAL. Observed client peak 101.1 MiB.
No production gate passes from these figures; see the raw artifact and limitations
in SHARED-CHECKPOINTS.md. Prior in-memory ratios are not database disk ratios.

Next: optimize/batch publication and shorten its append-blocking transaction;
repeat controlled traces with fixed IDs, varying text entropy/edit density/history
length and compressed backend alternatives;
bounded text/asset chunks; versioned draft/named references; cross-language codec
and parent/root/actor commit envelope; durable local history and resumable exchange.
Offline convergence, independent signatures/witnesses and native/scale acceptance
remain separate gates. The basic web editor is not the product priority.

Temporary DB dynodoc-evolution-part2b-20261010 and its disposable volume were
removed after validating its ownership label and loopback port 5553. Existing
dynodoc-postgres and margin-qdrant services remain running. Logs are in system temp;
the portable synthetic measurement artifact is committed under docs/benchmarks/.
No API/web server or VPS deployment in this increment. Production remains last
recorded 8c7461d, schema 24. Starting app 0718ee6; standalone a70647d.

Publication:
- Application source: `12be6a2d5444b7044b06749ca9d2b3fafea815ec`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38043426193): passed.
- [Application frontend/browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38043426145): passed (62 signed-in, 16 public browser tests and 274 frontend unit tests).
- Standalone source: `970d78ad49c94e63f902ac4f97dcc61cf212db27`.
- [Independent engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38043671852): passed.
- Final engine handoff/status commit: `2d9363072788de69a19df38752852a24fe230c09`
  ([checkout CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38044102949)).

## Previous completed increment

Updated 10 October 2026. **Part 2a complete, published and CI-verified. Part 2 remains in progress.** Part 1 remains published. Read [PRODUCT-SPEC](PRODUCT-SPEC.md),
[ENGINE-EVOLUTION-PLAN](ENGINE-EVOLUTION-PLAN.md) and
[SHARED-CHECKPOINTS](SHARED-CHECKPOINTS.md) before continuing.

## Part 2a: shared checkpoint foundation

- Versioned/domain-separated canonical objects cover all `DocumentState` fields,
  including tombstones, comment order, suggestions and removed choices. Logical IDs
  remain separate from addresses. Fixed hash vectors pin the Rust byte codec.
- Stable-key radix maps share unchanged objects across checkpoints and avoid fixed
  array chunk boundaries shifting on insertion. A one-node edit rewrites its value,
  map path and root; creation still scans/hashes the complete state.
- Document-scoped memory and PostgreSQL stores; no cross-document reuse or public
  hash lookup. Migration 0025 adds immutable bytes and checkpoint manifests without
  changing applied migrations or event encodings.
- Transactional opt-in publication derives an exact historical state, verifies full
  graph reconstruction before publishing, rejects mismatches, rolls back failures
  and supports same-revision concurrent retries. Reads reject missing/corrupt roots
  and check their recorded chain position. They still trust the root/history
  association; external attestations are not implemented.
- Operator `shared-snapshot` CLI; audited erasure removes manifests/objects and the
  admin preview/receipt records their counts and bytes. Ordinary snapshots and
  draft/version bases still use full JSON. No production default is switched.

## Verification and measurement

Seven codec/property tests and five isolated database tests cover round trips,
canonical bytes, all state components, reuse/insertion, malformed references/routes,
limits, legacy equivalence, rollback/retry, concurrency, scope and erasure. The
existing admin API erasure test also checks new counts and cleanup.

The first full application run failed the existing 100 ms SSE timing assertion
while a release build was running concurrently. That test was not weakened; the
full suite then passed all **216 tests** without competing work. Application fmt,
Clippy and unchanged generated OpenAPI passed. Standalone task lint and all 216
tests also passed. Standalone task docs passed with existing Rustdoc link warnings
in unchanged files. Standalone generated OpenAPI is also unchanged, including MIT
metadata. Application Rust, frontend/browser and independent engine source CI all
passed; their runs are linked below.

Synthetic release benchmark: 8,000 blocks, 21 checkpoints (20 one-block edits),
135,244,119 bytes as repeated full-state JSON versus 7,831,404 unique object bytes;
4,727-7,284 additional bytes per edit. Initial write 163.55 ms, median full scan/write
168.54 ms, final reconstruction 115.87 ms, observed process peak working set
54.5 MiB. Read the codec document for method and limitations: no PostgreSQL/WAL,
large native files, transfer, concurrency or production backend acceptance.

## Next concrete task: part 2b adoption and bounded content

1. Add a compatibility-preserving default reader/writer selection between legacy
   and shared checkpoints. Compare legacy/shared-plus-tail reconstruction at every
   revision. Migrate draft bases/named versions by explicit versioned references;
   preserve historical readers, erasure and stale-base checks. Do not silently
   delete full snapshots or switch production before rollout evidence.
2. Introduce bounded typed text/format/asset chunks so a single large logical node
   does not exceed the current 1 MiB object cap. Benchmark PostgreSQL/index/TOAST/WAL
   and durable local/remote backends, streaming, cancellation and memory. The memory
   store is a test backend; write/read budgets do not bound the prior materializer.
3. Settle cross-language numeric canonicalization and a separate commit envelope
   binding repository, parents, root, operations/capture capabilities and actor
   evidence. Then implement durable local commits, resumable missing-object exchange
   and atomic branch refs (part 3). Offline convergence/audit/native host acceptance
   remain separate explicit gates, not consequences of checkpoint deduplication.

Current limits: whole-state residency/scans, up to 1 MiB per object, 512 MiB/one
million object operations by default, Rust-specific numeric encoding, no chunked
assets, no GC, no default-path storage savings yet. No complete product or capacity
claim. The radix tree is an implementation choice; comparative prolly-tree results
remain unmeasured. Backup retention still applies after live audited erasure.

## Source, deployment and temporary resources

Working from application `1907eea` and standalone `6e7fdf9`. Both copies must match
for changed Rust sources/tests/examples and migration 0025, while preserving engine
MIT metadata and its standalone rich-text fixture. Published implementation:

- Application: `7a8e28ed1bfdf5828deb341fdf9b533fad9f5469`.
- Standalone engine: `6ccdc2277b187491eabbfa089a416bcdbcdaddac`; final handoff and
  LF-checkout guard: `a70647d553be610f41c005d87ce6de26aeb955dd`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38040472956): passed.
- [Independent engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38040495733): passed.
- [Final engine checkout-guard/handoff CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38041109485): passed at `a70647d`.
- [Frontend/authenticated browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38040472960): passed (62 signed-in and 16 public browser tests, frontend units and the production-proxy regression).

A final portability check found old Windows engine migration checkouts had CRLF
although committed/app/production bytes were LF. The engine now pins SQL checkouts
to LF and restores the committed bytes locally (no migration Git blob was edited).
All 22 migration files match the app. Applying app migrations to a fresh isolated
DB and then validating with engine migrations passed. All 216 engine tests passed
again with canonical checkout bytes. The packaging follow-up is recorded with the
finalized handoff.

No VPS deployment; last recorded production remains `8c7461d`, schema 24, with the
previous Nginx header fix. The owned disposable PostgreSQL container
`dynodoc-evolution-part2a-20261010` and its volumes were removed after verification
and ownership-label checks. No test API/web servers were started. Existing local
services were left running. Temporary logs and synthetic benchmark JSON are under
the system temp directory. The second owned checksum/test container
`dynodoc-checksum-part2a-20261010` and its volumes were also removed after its
checksum validation and 216-test rerun passed. No increment-owned servers remain.

## Earlier completed increment: part 1

The following is retained as the dated Part 1 release record. Its "next task" and
shared-storage limits predate Part 2a; use the continuation above for current work.

## Part 1 completed

- Made existing-editor document repositories the authoritative product. Replaced the
  duplicated combined volume with a current guide and marked legacy questionnaire/
  civic stages as historical. Corrected mathematical independence, complexity,
  shared-storage and audit claims, including their repeated cross-references.
- Defined stable logical identity separately from content addresses and formatting
  runs; initial graphs contain content, while the target avoids redundant full-file
  versions. Defined explicit offline/merge/trust/capacity acceptance boundaries.
- Added core `read_state_at`: nearest checkpoint at/before the exact revision,
  streamed suffix, revision/gap rejection and checkpoint/replay-count evidence.
  Historical batches/draft creation use the same reader as review/provenance.
- Streamed chain verification and made cadence checks query only sequence metadata.
- Bounded browser Myers extra work to input key count plus 1,000,000 work steps and
  trace to 1,000,000 integer cells (about 4 MB, excluding frontier/objects/input).
  Removed repeated frontier copies and quadratic duplicate occurrence lists.
  Exhaustion uses unique anchors/LIS; fewer matches can require additional review.
- Added exact historical replay/checkpoint/scope tests, duplicate-heavy matching
  regressions and uniqueness/crossed-move counterexamples to disjoint-ID independence.
- Synchronized the seven changed engine source/test files byte-for-byte to the MIT
  repo and preserved its packaging metadata. No event/schema/migration changes.

## Verification

| Check | Result |
|---|---|
| Application `cargo test --workspace`, isolated PostgreSQL | 204 passed. After final streaming/cadence changes, all 13 event/snapshot tests passed again. |
| Application Rust formatting/Clippy | Passed, including all targets after final changes. |
| Frontend full unit suite | 274 passed, including document/spreadsheet/presentation import regressions. |
| Changed frontend files: Prettier/ESLint; complete Svelte/type check | Passed; zero Svelte errors/warnings. |
| Standalone engine `task lint`, `task test` | Passed; all 204 tests, including the unchanged 100 ms SSE check. |
| Standalone `task docs` | Passed with previously documented Rustdoc link warnings. |
| Standalone `task openapi` and generated diff | Passed; HTTP schema unchanged. |
| New documentation links, source equality and whitespace checks | Passed. |
| Application remote CI | Rust passed; frontend lint/build, 274 unit tests, 62 signed-in browser tests, 16 public browser tests and proxy regression passed. |
| Engine implementation and final documentation-head CI | Both passed. |

The isolated container `dynodoc-evolution-part1-20261010` and its volumes were removed.
No test servers started; unrelated local containers retained. No VPS deployment,
real native-host acceptance or marketplace publication was performed. Last recorded
production remains app `8c7461d`, schema 24, plus the Nginx repair. Local tests do not
establish hundreds-of-MB or large-service capacity. Remote browser acceptance used an isolated CI stack; it does not certify native
Microsoft/Google installations, marketplace publication or a new VPS deployment.

## Measurements and complexity boundary

Pre-change audit (10 October) release medians: synthetic 8,000-block/6,030,116-byte
JSON parse 58.44 ms; classify 41.36 ms; format patch 0.256 ms; replay 0.099 ms; small
paragraph merge 0.032 ms; 8,000-block fingerprint 8.19 ms. These exclude native
conversion, network, database writes, rendering and concurrent load. They are a
baseline, not a fresh end-to-end benchmark of this increment.

Current reads still hydrate full state; checkpoints and draft bases still duplicate
full JSON. Missing historical checkpoints still require genesis replay. The periodic
threshold is best-effort, not a hard 500-event tail bound. Key matching budgets do
not cover all shape/cell candidate matching or key-byte comparison costs. Signed
heads, shared object storage and a complete local commit DAG remain unimplemented.

## Next concrete task: part 2 shared checkpoints

1. Specify typed/versioned canonical objects, logical identity versus hash address,
   commit manifests, ordering, chunk/reference limits and all committed state.
2. Implement a tenant-scoped object-store boundary and shared checkpoints alongside
   legacy snapshots; no rewriting event hashes or applied migrations.
3. Compare reconstruction with legacy replay at every fixture checkpoint. Verify
   unchanged objects are reused after a one-node edit, publication is recoverable,
   missing/corrupt objects fail explicitly and tenant/erasure boundaries are intact.
4. Measure unique-byte growth and peak memory. Then migrate full draft bases/named
   versions and develop local commits/object negotiation in part 3.

Bounded parsing/object contracts start now; full-load acceptance is part 7. Real-host
experiments may proceed alongside settled contracts. Do not resume Office web-editor
parity or the questionnaire/civic roadmap. Update this handoff and both project
status records at every increment and before context/token exhaustion.

## Source and publication record

Starting application `f054ad1`; starting standalone `67060cf` (service `d6666a2`).
Implementation is pushed to both main branches:

- Application: `667ecb6ecef5b64debcca6817e73b6be807bde37`.
- Standalone engine implementation: `647249e7ed692534f7ab21a1ad522912c7d58385`;
  finalized handoff/status at `6e7fdf91810677b3dc8778b7d9789882125d2bb1`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38038078571): passed.
- [Frontend/authenticated browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38038078512): passed (274 unit, 62 signed-in, 16 public browser tests; proxy regression).
- [Independent engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38038142218): passed.
- [Final engine documentation-head CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38038345736): passed.

All listed runs passed. The documentation follow-up records results and synchronization
without changing the verified implementation. There are no remaining part-1 test
failures; the larger unfinished product capabilities are explicitly listed above.
