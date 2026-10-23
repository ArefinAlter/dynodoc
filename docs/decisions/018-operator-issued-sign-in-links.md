# Decision 018: operator-issued sign-in links and the first administrator

Date: 23 October 2026. Owner: Verne. Status: built and checked on this PC as listed under "What was run"
(real PostgreSQL 16, the real router, the real `engine-cli` process, Chromium against an isolated stack, and
`dynodoctl` against a scripted fake of Docker). **Not** run against the real Docker bundle (a cold image build
is about 25 minutes; the wrapper was exercised only through its fake runner), not reviewed independently yet,
migration 0047 not rehearsed against a production-like database, and GitHub Actions could not run (billing).
Completes the last open item of task P0-3 ("operator-issued sign-in links; ability to disable email sign-in")
and the first-administrator step of Phase 0 acceptance criterion 1 of the
[self-hosted edition plan](../SELF-HOSTED-EDITION.md). Refines section 9.4 of the
[identity and access design](../SELF-HOSTED-IDENTITY-AND-ACCESS.md) and reuses the patterns of
[decision 017](017-native-app-sign-in-handover.md) (secrets stored as hashes, use once in one transaction, audit
without secrets, runtime-role grants).

## The problem

A self-hosted installation could let a person in only by an emailed link (which needs a working mail server) or
with `DEV_AUTH=true` (anyone signs in as anyone: not for production). With `AUTH_EMAIL_LINK=off`, which
`dynodoctl install` chooses when no mail server is given, nobody could sign in at all, the first administrator
included. Phase 0 criterion 1 says "`dynodoctl install` produces a running installation; the first admin signs in
with an operator-issued link".

## What the design said, and what changed

Section 9.4 of the identity document proposed `dynodoctl admin sign-in-link EMAIL`, which "calls `/auth/magic-link`
with the service key and prints the one-time link", and a `break_glass` list of addresses that may use operator links
while email sign-in is off. Two things were wrong with that:

1. **It is a network path.** `/auth/magic-link` accepts the web server's service key from any caller that holds it. A
   compromised web tier (or a leaked key) could mint a sign-in link for any address, including an administrator's,
   and the route also creates accounts. Issuing a credential that signs people in must not be possible with the
   credentials the running services hold. The operator link is therefore written by the **command line tool with the
   database owner credential**, which the API and the web server never receive, and **no HTTP route issues one**.
2. **A `break_glass` list is not needed.** An operator link is a different kind of credential from an emailed one, stored
   in a table of its own. It works whatever the email switch says, and the person who can make one already holds
   the owner credential. A list would only restrict the operator, who can edit the database anyway.

## The flow

```text
 operator (shell on the host,                 database                  people (a browser)
  owner credential: the install directory)
 -------------------------------------        --------                  ----------------------------
 dynodoctl admin-link --email A --create
  asks: type A (terminal only)
  runs, in a one-off container with the
  owner credential:
   engine-cli auth issue-link --json ...  ->  operator_sign_in_link:
                                               token_hash = SHA-256(token)
                                               expires_at <= 24 h, once
                                              operation_audit: .issued
  <- {"link": "https://ORIGIN/login?token=.."}
  shows it ONCE on the terminal; saves nothing
  hands it to the person, privately
                                                                        opens the link: page "Your secure
                                                                        sign-in link is ready", Continue
                                              <- POST /auth/verify {token}  (web tier -> API, as for an emailed link)
                                              API: emailed (magic) link? no -> operator link:
                                               update ... set used where hash, unused, unexpired,
                                               account active and same session generation;
                                               make the login; audit .used     -- ONE transaction
                                              -> tokens -> encrypted session cookie -> /projects
```

### Bounds

| Value | Where it is made | Shape |
|---|---|---|
| token | `engine-cli`, operating-system randomness (the generator emailed links use) | 32 bytes as 64 lower-case hexadecimal digits; stored as SHA-256 only; printed once |
| life | `--ttl` | `90s`, `15m`, `2h`; 60 seconds to 24 hours, default 15 minutes; the table refuses more than 24 hours |
| link | `engine-cli` | `<ORIGIN>/login?token=<token>`; `ORIGIN` is `https://host[:port]` (or `http://` for localhost), no path, user, query or fragment |
| live links | per account | one: a new link ends the earlier unused ones in the same transaction |
| operator label | `--operator`, default the operating-system user name | 1 to 64 bytes, no control characters and no invisible or direction-changing format characters; a label, not an authentication |
| reason | `--reason` | 0 to 200 bytes, same character rule; kept in the audit trail for good, so no personal data |
| hash prefix | `list-links`, `revoke-link` | the first 8 hexadecimal digits of the token's SHA-256; never the token |

## Decisions

| Question | Decision | Why |
|---|---|---|
| Where is the token stored | New table `operator_sign_in_link` (migration 0047), not a `kind` column on `magic_link` | The API issues emailed links itself, so its login needs INSERT on `magic_link`; a kind column there could not stop it writing operator links. A table of its own lets the runtime login have **no INSERT at all** (`CONSUME_PRUNE` in `engine_core::database`: select, update, delete), which the start-up check (`check-runtime`) enforces. |
| How a token's kind is told apart | By the table it is in. `POST /auth/verify` tries the emailed links first (unchanged queries, so hosted behaviour does not change) and then this table | One credential cannot be mistaken for the other: a token is a 256-bit random value known by its hash in exactly one table. |
| The switch `AUTH_EMAIL_LINK=off` | Unchanged. It stops the public request for a new emailed link (a web-tier route), as it always has; a link already issued can still be confirmed, and an operator link is never affected | That is the point of the switch on an installation with no mail. There is no flag in the API, so there is nothing for the public to bypass. A test pins the refusal in the web tier and a mutation that ignores it fails it. |
| Roles | Untouched. Administrators are the addresses in `ADMIN_EMAILS` (`admin::is_admin`), checked per request; there is no role column. `--role admin\|user` is an assertion: refuse when the installation disagrees | A link signs the person in as who they already are. An operator link for an existing non-administrator cannot make them one, and `--create` cannot make an administrator out of a new address (add it to `ADMIN_EMAILS` first). |
| `--create` | An account is made only for the **first administrator**: the address is in `ADMIN_EMAILS` and none of those addresses has an active account. Any other new account needs `--confirm-create <the address again>` (in `dynodoctl`, a second typed phrase `CREATE ACCOUNT <address>`). A refusal changes nothing (exit 3) | Making accounts is what an open sign-in page also does, but the operator should never make one by a slip. "An administrator exists" is defined the only way this installation can define it: an active account whose address is in `ADMIN_EMAILS`. A disabled administrator does not count, so if every administrator account was disabled, `--create` works again for ANOTHER address in `ADMIN_EMAILS`. **A limit, stated plainly:** for the SAME address the answer is "the account is disabled" (a disabled account gets no link, with or without `--create`), and the tool has no `--restore`. Configured administrators cannot be disabled in `/admin` (the existing rule), so this needs a manual database change or a removal by another route; the way out is then to add a second address to `ADMIN_EMAILS` and make its first link, or for the owner to re-enable the row in SQL. A `--restore` (loudly confirmed, audited) was weighed and left out as the larger change. The "one at a time" of the rule is real: every account creation takes one fixed database lock before the check, because the check looks at other addresses (found by the review: two first administrators made at once both succeeded in 23 of 25 rounds with only a lock per address; a test now makes two at once for 25 rounds). |
| Disabled and erased accounts | A disabled account gets no link (restore it in `/admin`). A link whose account is disabled, or signed out everywhere (session generation moved), after it was issued is refused. Erasing an account deletes its links. An erased address is free again: `--create` makes a new person, as an emailed sign-in would | Same checks as an emailed link (`consume_magic_link`). The `reason` text could name the person, so erasure removes it. |
| Another browser or address | Allowed. The link is a bearer token | A person is often given the link on another device. The life is short, the first use ends it, `revoke-link` ends it sooner, and it is shown only on the operator's terminal. |
| Rate limits | **None per client address, none shared with the public sign-in.** The public email-link request window (5 per 15 minutes per address) is not read or written by operator links; failed uses of unknown tokens are not counted | A lockout the public can trip is a way to lock the operator out of break-glass. A 256-bit token needs no guessing limit (the edge proxy's per-address limit on `/auth/` still applies). The operator is bounded instead: one live link per account, a 24-hour ceiling, the audit trail. |
| What the page shows | Unchanged: "Your secure sign-in link is ready" and one **Continue** button; the use happens on the click, so a mail scanner or a link preview cannot use the link up. A used, expired or ended link says "Ask your administrator for a new one" where email sign-in is off (the old wording offered a request that would be refused) | |
| The native-app hand-over (decision 017) | Unchanged. An operator link signs the browser in; `/app-signin` then works as after any sign-in. With email off and Google off, `/app-signin` before signing in still says no method is turned on | One session semantics, no new path. |
| Audit | `operation_audit`: `auth.operator_link.issued` (operator label, reason, lifetime, whether the account was made, whether the address is an administrator, links ended, hash prefix), `.used` (the login's id), `.refused` (a KNOWN link that was used, expired, revoked or whose account changed: at most one row a minute per link), `.revoked`, `.expired` (swept by the retention job). The actor is the account the link signs in as | No token, no address in any row. The operator label is **not authenticated**: whoever holds the owner credential can write any value, or the table. |
| `dynodoctl admin-link` | A thin wrapper: the address must be typed at a terminal (`--confirm` and piped input do not count, nor does a redirected output); the link is shown once on that terminal and saved nowhere; there is **no `--json`** mode; it takes the run lock | A credential in machine-readable output ends up in a log. A script must not be able to say yes for the person. |
| Logs of `engine-cli auth` | To standard error, so standard output is the link alone (`... \| clip` copies just the link) | Found by a test: at `RUST_LOG=debug` the first version wrote SQL log lines to standard output beside the link. |
| The miss path of `/auth/verify` | A well-shaped token that is not an emailed link first takes ONE indexed read of the operator table; only a token that names a row opens the transaction. A miss is the same 401 and body and writes nothing | Found by the review: every failed verify (a used emailed link, a probe) would otherwise run a transaction, an update and an insert-select on the new table, on the hosted service as well. A test holds a write-blocking lock on the table and shows an unknown token is still answered at once, while a real link waits. |
| Invisible and direction-changing characters | Refused in the operator label, the reason and the address, by the tool (`is_invisible_or_bidi`: the Unicode format characters such as U+202E, U+2066, U+200B and U+FEFF, tags, variation selectors, U+2028 and U+2029) and by the table's CHECK constraints (migration 0047, amended in place), and by `dynodoctl` (a copy of the list, as it depends on no engine crate) | `char::is_control` and `[[:cntrl:]]` do not cover category Cf, so a label or reason could have read as something else in a terminal or an audit viewer. A test tries both ends of every range against the tool and the table and the characters next to the ranges against both. |
| The token in the address bar | The sign-in page reads the token once, keeps it in memory, and replaces the address (`replaceState`, other parameters kept) when the request is made, success or failure; on failure the message shows and Continue can be pressed again from memory, but a reload shows the ordinary sign-in page and sends nothing | Keeps the link out of the history, a bookmark and a screen share after the click. Before the click it is still in the address (a mail scanner or preview does not run the page's script). Tested in the browser and by a unit test of the function. |

## Routes, migration, commands

* **Routes:** none added. `POST /auth/verify` (existing, unauthenticated, no OpenAPI entry) gained a second lookup.
  `openapi.yaml` is byte-identical to the generated one. A test posts to eleven plausible issuing paths with and without
  the service key and a bearer token (all 404 or 405), and a source check fails if any file of the server calls
  `operator::issue`.
* **Migration `0047_operator_sign_in_link.sql`:** one table, two trigger functions, three indexes. Additive; nothing existing
  changes; the hosted table stays empty unless the CLI is used. Like 0043 to 0046 it needs `engine-cli grant-runtime` after
  it, and rolling back means restoring the dump. **Amended in place on 23 October 2026** (the review of the first version: the `operator` and `reason` CHECKs now also refuse invisible and direction-changing format characters, below); the file had run in no shared environment, only in disposable test databases made fresh each run, which is the condition `AGENTS.md` sets. Row rules: `token_hash` 32 bytes and unique; `expires_at` within 24 hours
  of `created_at`; `consumed_at` and `outcome` (`used`, `revoked`, `expired`) set together; a row is **born live** (an insert
  trigger) and **ended exactly once** (an update trigger refuses every other edit, a second end, `used` after expiry and
  `expired` before it). Privileges: nothing for `PUBLIC`; the runtime login gets select, update, delete.
* **Commands (`engine-cli`, owner credential in `MIGRATION_DATABASE_URL`):** `auth issue-link --email A [--ttl 15m] [--create]
  [--confirm-create A] [--role admin|user] [--reason T] [--operator L] [--origin O] [--admin-emails E] [--json]`
  (exit 3, nothing changed, when the second confirmation is needed); `auth list-links [--all]`; `auth revoke-link PREFIX
  [--operator L]`. `dynodoctl admin-link --email A [--ttl] [--create] [--role] [--reason]`.
* **Other changes:** `ci-frontend.yml` gives the owner URL and the CLI path to the end-to-end step only (the API and web servers
  never see them); the retention job gained a spec and a sweep; account erasure deletes the links;
  `dynodoctl install` prints the command for the first administrator; the web verify route words the refusal.

## Threat table

| Threat | What stops it | What is left |
|---|---|---|
| **Remote issuing** (a compromised web tier, a leaked service key, an SQL injection in the API, a bug in a route) | No HTTP route issues links; the runtime login has no INSERT on the table (checked at every API start and by `check-runtime`, by table and by column); the issuing function is called from nowhere in the server (a source test); the container that runs it is the only one with the owner credential besides the migration jobs | Whoever holds the owner credential. They already read and change the whole database, including `identity`, `refresh_token` and `operation_audit`, so a link adds no power. **What this feature does not close:** `POST /auth/exchange` (service key only) already mints a session for any address, and the runtime login can insert `refresh_token` rows, so a service-key leak or an SQL injection in the API already reaches a session without any operator link; this feature adds no such path and removes none. |
| **Forged or guessed token** | 256 random bits; only the hash is stored; the hash index is unique; a hash, a prefix, upper case, whitespace, any variant is refused | none known |
| **Replay, and two uses at once** | The use-up and the login are one transaction on a row lock: of 8 simultaneous uses exactly one wins (5 rounds); the table's trigger refuses a second end | none known |
| **Expired link used** | Checked by the database clock in the statement that uses it up, and again by the trigger (a mutation of each, alone, is caught) | clock of the database host |
| **Link given to the wrong person, or read over a shoulder** | 15 minutes by default, 24 hours at most; once; one click on the page; shown only on the operator's terminal and saved nowhere; `revoke-link`; a new link ends the old | It is a bearer token: whoever uses it first signs in as the person, from any browser or address. Say who you are giving it to and give it privately. |
| **The token in a log, a file, an argument or the audit** | Not an argument (the container prints it); one `println!` to standard output; `Debug` and the wrapper's report never show it; the bundled proxy's access log never writes a query string (`dynodoc_safe_uri`); the audit holds the hash prefix only. Tests run `engine-cli` at `RUST_LOG=trace` and scan standard output (once), standard error, the table, the audit rows, the API's log, every file of the installation directory and every command line of the wrapper | The terminal's scroll-back. The person's browser history holds the link until they press Continue. The operator's own copy of the link in a chat or a mail is theirs to delete. |
| **A script says yes for the operator** | The address must be typed at a terminal; `--confirm`, piped input and a redirected output are refused | A person who pastes the whole command and the address into a script is a person who decided to. |
| **Escalation to administrator** | Roles are `ADMIN_EMAILS` only, which a link cannot change; `--role` asserts and refuses; `--create` cannot create an administrator that is not already configured; the first-administrator rule needs no active administrator account | An address in `ADMIN_EMAILS` that someone else will register first: set it before the first sign-in. |
| **Public lock-out of the operator** | Nothing the public does is counted against operator links; 60 garbage uses and an exhausted email-link window did not change the next issue or use | none known |
| **Operator link on a removed account** | Refused at issue (disabled), at use (disabled, or signed out everywhere since), and by erasure | none known |
| **Abuse of the CLI by the runtime login** | `engine-cli auth issue-link` with the runtime login as `MIGRATION_DATABASE_URL` fails with "permission denied" and prints nothing (tested through the real process) | none known |

## Residual risks and limits

* The owner credential is the trust root. This feature is **not** a separation of duties: the operator who makes a link can
  already do anything the link could do. The audit trail is a record for honest operation, not tamper-proof against that
  credential.
* The operator label is the operating-system user name as the tool saw it. In the bundle that is the host user of
  `dynodoctl` (passed in), not the container user. It can be set to anything.
* The link stays in the person's browser history and address bar until they press Continue (then the page removes it); it is single use.
* The `database-admin` service has `logging: { driver: none }` (`infra/selfhosted/compose.yml`), so the link is not kept in a
  container log: without it `docker logs` would hold the link of a stopped container if `dynodoctl` were killed before
  `run --rm` removed it (the review checked, on Compose v5.1.4, that `run --rm -T` still returns the output to the caller
  with this setting). **The cost:** the other maintenance commands (`verify`, `verify-snapshots`, `check-runtime`, ...) leave
  no log to read afterwards; read their output from the terminal when they run. **Not run against the real bundle by me**
  (no `dynodoc-local` image on this machine; a cold build is about 25 minutes).
* The email switch does not stop confirming an emailed link issued before it was turned off. That is the existing behaviour
  and was left alone.
* Nothing limits how many accounts `--create` makes, other than the second confirmation.
* There is no `dynodoctl list-links` or `revoke-link`; they are `engine-cli` commands run through the `database-admin`
  service (documented in the operations guide).
* OIDC, SAML and SCIM (Phase 1) will replace much of this for installations that have an identity provider; the operator
  link stays as the break-glass.

## What was run

Everything on one Windows 11 PC, 23 October 2026. Nothing ran in GitHub Actions (the private repository's Actions cannot
start: billing).

* **API, real PostgreSQL 16:** 27 tests in `crates/engine-api/tests/operator_link_test.rs` (each a fresh migrated database):
  issue then use once and the replay's identical 401; hash, upper case, whitespace and truncated variants refused; expiry by the
  query and by the trigger; lifetime bounds in the tool and the table; normalised address and one account per link; the
  `--create` rules (no administrator yet, administrator exists, ADMIN_EMAILS empty, a disabled administrator, wrong or missing
  confirmation, refusals change nothing); roles never escalated (an ordinary account stays ordinary, `--role` mismatches make
  nothing, an administrator is one only because of `ADMIN_EMAILS`); disabled accounts, signing out everywhere, erasure through
  the real administration routes; the public's exhausted email window and 60 junk uses do not block the operator; one audit row a
  minute for a replayed link, none for an unknown token; list, revoke and supersede; 8 simultaneous uses, 5 rounds, exactly one
  winner each, and simultaneous issuing and `--create`; no token in the table, the audit rows, `Debug` output or the API's trace
  log; the restricted runtime login can use a link but gets 42501 on every insert, trigger errors on every other edit, and
  `check-runtime` fails if INSERT is granted by table or by column; rows born used are refused; no route (eleven guessed paths,
  `openapi.yaml`, and a source scan for the issuing function); emailed links unchanged; the app hand-over works after an
  operator sign-in; the use, the login and the audit row succeed or fail together (a trigger refusing the login, another refusing
  the audit row).
* **`engine-cli` as a real process, real database:** 8 tests in `crates/engine-cli/tests/operator_link_cli_test.rs`, all at
  `RUST_LOG=trace`: the link is on standard output alone and once, absent from standard error, the table and the audit rows;
  `--json`; exit 3 with nothing changed for the second confirmation; bad lifetimes and origins refused before any connection;
  no owner credential means nothing (the API's `DATABASE_URL` is not a substitute); the runtime login's credential gets
  "permission denied" and prints nothing; list and revoke by prefix.
* **Retention:** the existing `retention_test.rs` (8) and the unit test now include the table: expired links swept to `expired`
  with one audit row each, old ones pruned, the rest kept, history otherwise untouched.
* **`dynodoctl`, against the scripted fake runner:** 9 tests in `admin_link.rs` (the link shown once and in no file, command
  line or seeded-secret scan; nothing made without a typed address, `--confirm` and redirected output refused; no `--json`;
  every refusal before anything runs; the second phrase; failure and strange answers show no link; the options reach the
  container exactly; the install instructions) and 1 in `ctx.rs`; 383 `dynodoctl` tests pass in all. **Not run against the
  real Docker bundle.**
* **Web:** vitest 976 passed and 4 skipped over 86 files, 1 failed (see below); 3 new in `options.test.ts` (a link is confirmed with
  email sign-in off; the refusal's wording; the old wording where it is on) next to the existing ones that pin the 403 of
  `POST /auth/email`. Chromium against an isolated stack (disposable database, this tree's API with the restricted login, three
  web processes): `selfhosted-noemail.spec.ts` 6 passed (3 new: the first administrator signs in with email off, the public
  request is refused in the same test, `/admin` works because of `ADMIN_EMAILS`, the native-app page then works, the replay in
  another browser is refused with the new words; an ordinary account is not an administrator; an expired 60-second link is
  refused after 62 seconds), `selfhosted.spec.ts` 13, and the hosted `security`, `session-revocation`, `app-signin` (17) and
  `launch` specs. eslint, prettier, svelte-check (0 errors) and the build pass. The one vitest failure,
  `schema.test.ts` "know every DYNODOC_ name the Rust crates read", names `DYNODOC_TLS_MODE`, which `dynodoctl`'s sources
  already contained at the base commit (`ca7332c`) and the web configuration's list does not; it is not caused by this work, and
  was not run against the base commit.
* **Gates:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and
  `RUSTDOCFLAGS=-D warnings cargo doc` clean; `openapi.yaml` byte-identical to the generated one; `cargo test --workspace --locked
  --no-fail-fast`: 1,766 passed, 2 failed (the known Windows-only `sse_delivers_a_new_event_within_100ms`, and the retention unit
  test whose expected windows lacked the new table, fixed and re-run: the 101 library tests pass; the whole workspace was not run
  again after that line).
* **Mutations, 19, each fails at least one test** (the first batch of results was thrown away: the build had silently failed
  because the end-to-end API held `engine-api.exe`, which the harness mistook for failing tests; the batch was redone with the
  failing test names read from the output): token stored without the hash (7 tests); single use removed from the query alone (7)
  and from the query and the trigger (7); expiry ignored in the query alone (2, a 500 from the trigger) and in the query and the
  trigger (2); the runtime login granted INSERT, grant and check both (1); `--role admin` not enforced (1); the first-
  administrator rule always true (2); the public email request ignoring `AUTH_EMAIL_LINK=off` (the vitest cases); a disabled
  account accepted by the link's query and the session generation ignored by it (each **survived** the first test set, because
  the login's own insert refuses such an account in the same transaction, so nothing outside changed; an assertion on the
  audited refusal now kills both); the token put into the audit row (1); a route calling the issuing function (the source scan);
  the CLI printing the link on standard error too (1); the CLI's logs on standard output (several); `dynodoctl` accepting
  `--confirm` (at least 1); `dynodoctl admin-link --json` allowed (at least 1); a new link not ending the earlier ones (at least
  2); the end-once trigger not installed (2); the login made outside the transaction that uses the link up (at least 2).

## Not done, on purpose

* A real-bundle run of `dynodoctl admin-link` (the API image build is about 25 minutes cold; the wrapper is tested against the
  scripted fake runner, and `engine-cli` against a real database).
* `dynodoctl list-links` and `dynodoctl revoke-link` wrappers.
* A list of break-glass addresses (not needed, see above); a per-use notification to the person; a mode that emails the link.
* A rehearsal of migration 0047 against a production-like database.
