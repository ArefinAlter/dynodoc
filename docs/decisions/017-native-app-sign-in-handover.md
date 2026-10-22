# Decision 017: signing the native apps in from the system browser

Date: 22 October 2026. Owner: Verne. Status: built and checked on this PC as listed under "What
was run"; not rehearsed against a production database, never tried on a physical phone, iOS not
started. Refines item 6 of [decision 016](016-devices-processors-desktop-and-mobile-apps.md).

## The problem

The desktop shell, the Android app and (later) the iPhone app show the hosted or a self-hosted
Dynodoc web app in the system web view. Dynodoc signs in with an emailed link or with Google. The
emailed link opens in the SYSTEM BROWSER, and so does Google's page, and the system browser shares no
cookies with the app's web view. On a computer the shell had a workaround (a menu item to paste the
emailed link); a phone has no menu, and once a server is saved the launcher is not reachable, so on a
phone the app could not be signed in at all.

## What decision 016 said, and what changes

Item 6 of decision 016 ended: "Deep-link payloads are validated and never carry tokens." That stays
true of **tokens**: no session, access token or refresh token ever travels in a link. This decision
adds one thing that does travel: a one-time **hand-over code**. It is not a credential on its own. It is
useless without a second value (the PKCE verifier) that never leaves the app, it works once, and it
expires after 60 seconds. So the line "never carry tokens" becomes "never carry a token, and carry a
hand-over code only in the one fixed link shape `dynodoc://signin?code=..&state=..`". The deep-link table
in `apps/desktop/src-tauri/src/deeplink.rs` has exactly one more row, and the exact-whitelist tests pin it.

The design is the one RFC 8252 (OAuth for native apps) describes, with PKCE (RFC 7636) so that a
program that takes the code out of the link learns nothing it can use.

## The flow

```text
 app (shell)                    system browser                    server (web tier + API)
 -----------                    --------------                    -----------------------
 1 verifier = 32 random bytes
   challenge = b64url(SHA-256(verifier))
   state = 24 random bytes
   keep them in memory only
   open  <server>/app-signin?challenge=..&state=..&platform=..  ->  page: validated; if no
                                                                      session: normal sign-in,
                                                                      back to the SAME request
                                                                   page shows who is signed in,
                                                                   the device, Continue / Cancel
 2                              Continue ->  POST /auth/app-signin  ->  API: code (256 bits), only its
                                                                      hash stored, bound to account,
                                                                      challenge, state, platform;
                                                                      60 s; once; <= 3 unused
                                <- { url: dynodoc://signin?code=..&state=.. }
                                navigate to it
 3 <- dynodoc://signin?code=..&state=..   (cold or warm start; desktop: single-instance forward)
   strict parser; attempt pending? fresh (10 min)? same server? state equal (constant time)?
   YES: the attempt is used up now; the window goes to the PLAIN address
        <server>/app-signin/finish           (no secret in any address the web view requests)
   page has loaded: the shell sets location.hash = #code=..&state=..&verifier=..   (once, only
   to that page, within 30 s; a fragment is never sent to a server)
 4 finish page (static HTML): reads the fragment, removes it from the address, POSTs
   {code, state, verifier} to its own server  ->  POST /auth/app-signin/redeem
                                                   -> API: row locked; live, unused, account still
                                                      signed in at the same generation, SHA-256(
                                                      verifier) = challenge, state equal (constant
                                                      time); used up in the same transaction;
                                                      session made as a magic-link sign-in makes it
   <- session cookie set in the app's window  ->  /workspace
```

### Bounds, all checked on both sides and again by the API

| Value | Made by | Shape |
|---|---|---|
| verifier | app, operating-system randomness | 32 bytes, 43 characters of base64url; the API accepts 43 to 128 of `A-Za-z0-9-._~` (RFC 7636) |
| challenge | app | `base64url(SHA-256(verifier))`, exactly 43 characters, canonical spelling (the last character carries four bits) |
| state | app, operating-system randomness | 24 bytes, 32 characters; accepted 16 to 64 of `A-Za-z0-9_-` |
| code | API | 32 random bytes as 64 lower-case hexadecimal digits; stored as SHA-256 only |
| platform | app | `desktop`, `android` or `ios`; shown to the person as "computer", "Android phone", "iPhone" |
| life | | code 60 s (database clock only); attempt 10 minutes (monotonic clock in the app); at most 3 unused codes per account; one attempt per running app |

### Routes

| Method and path | Who may call | What it does | Answers |
|---|---|---|---|
| `GET /app-signin?challenge=&state=&platform=` (web) | anyone; needs a browser session to go on | validates the three values (exactly these, each once). Not signed in: `/login?next=` with the request rebuilt in one canonical spelling, then back. Signed in: the confirmation page. Installation with no sign-in method: says so, no redirect | 200 page; invalid request: 200 "not valid" page; never a redirect to anywhere else |
| `POST /auth/app-signin` (web) | same-origin JSON from a signed-in browser | validates, calls the API with the person's own access token and the service key, returns `{url: "dynodoc://signin?code=..&state=.."}` | 200; 401 not signed in; 403 other origin; 415 not JSON; 400 bad body; 429 too many unused codes |
| `GET /app-signin/finish` (web) | anyone | static page; the script reads `#code=..&state=..&verifier=..` (there at the start, or set by the shell after the load), removes it, posts it; with none it waits six seconds, then says it did not finish | 200 |
| `POST /auth/app-signin/redeem` (web) | same-origin JSON | validates, calls the API, sets the encrypted session cookie | 200 `{ok:true}`; every failure is the same 401 and the same words |
| `POST /auth/app-handover` (API) | the web server only (service key) AND the person's bearer token | makes the code; audit `auth.app_handover.issued` | 200 `{code, expires_in}`; 400, 401, 403, 429 |
| `POST /auth/app-handover/redeem` (API) | the web server only (service key); the app's own credentials are the body | redeems; audit `.redeemed` or `.refused` | 200 tokens as `/auth/verify` returns them; 401 for every refusal; 429 over a rate limit |

The two web POST routes live under `/auth/` on purpose: the production proxy already rate-limits that
path per client address, and the no-store and no-referrer headers already apply to it.

Migration `0046_app_handover.sql` adds one table, `app_handover_code`: code hash, challenge, state,
platform, account, session generation, times, and a "used" mark that a trigger lets move from unused to
used exactly once. `engine-core/src/database.rs` lists it for the restricted runtime login (insert,
update, delete for pruning rows older than a day; the trigger refuses every other edit). Nothing else
changes, and nothing reads the table unless the routes are called.

## Refinements of the first sketch, and why

1. **The state is checked twice.** The app checks it (as the sketch said). The API stores it and the
   finish page sends it back too, so redeeming needs the code, the state and the verifier. It costs
   nothing and gives the API a binding of its own to test.
2. **Both API routes need the web server's service key**, and issuing also needs the person's own
   access token. Without the key a stolen 24-hour access token could be turned into a 30-day refresh
   session.
3. **A failed redeem burns a live code.** An interceptor gets one guess at a 256-bit value; the person
   pays one retry. An unknown code burns nothing.
4. **A link with the wrong state does not cancel the person's attempt.** A hostile link must not be able
   to cancel a sign-in (a right state uses it up at once, expiry and a change of server clear it).
5. **The attempt is bound to the server it was started for.** The verifier is only ever shown to that
   origin; if the configured server changed meanwhile, the attempt is dropped.
6. **Continue is a script request, not a form post.** A form post answered by a redirect to
   `dynodoc://` is stopped by the page's own `form-action 'self'` policy in Chrome (it checks redirects).
   The script makes the request, then navigates; a button repeats it if the browser did not open the app.
7. **The secrets are in no address the web view requests.** The sketch put code and verifier in the
   fragment of the address the window is sent to. On the Android emulator the verifier then appeared in
   the device log: Android's web view logs the page's address next to every console message (Tauri
   forwards them as `Tauri/Console`, and its own start-up scripts raise such messages on every page).
   So the shell sends the window to the plain `/app-signin/finish`, and once the page has finished
   loading sets `location.hash` from an in-memory outbox (`signin::Outbox`): once, only to that exact
   page of the configured server, within 30 seconds, and only as a script made from the three values
   (`signin::finish_script` refuses every other character). The page reads the fragment at start or on
   `hashchange`, which also covers the app opening it a second time while it still shows (after a
   failure the browser changes only the fragment and runs no page load; found by the real-shell
   probe). After this change the emulator log held neither the verifier nor the code or state in the
   app's own lines.
8. **Windows adds a slash after the query** of a custom-scheme link. The parser tolerates exactly one
   at the very end, as it already did after the path.
9. **Rate limits.** Per code prefix (first eight characters), five failures in 15 minutes; per client
   address, 20 failures, **only when the web server's view of the address tells visitors apart** (a
   proxy that hides it would make one bucket for everyone, and then one visitor could lock all others
   out). Successful redeems cost nothing. The edge proxy's own limit applies to `/auth/`.
10. **Phones: no launcher deep link, no native control.** See the next section.

## Phone sign-in without a menu

The shell's rule stays: a page of the server gets **no** native access, not even an allowlisted one.
So the page cannot ask the app to start the sign-in. Instead:

* No server saved: the launcher is the first screen (as before).
* Server saved, not signed in: the server's `/workspace` sends the window to its `/login` page. When
  the page has finished loading and its address is the configured server's `/login` (and not
  `/login?token=`, the pasted-link route), the **shell** shows its own launcher with a primary button
  "Sign in with your browser". The test is on the address alone: no cookie, header or page text is read.
  What a page can cause is "the launcher appears"; that is a page of the app itself with no power over
  the app, and a person whose server misbehaves can use it to choose another server.
* Signed in, then signs out in the web app: the web app goes to `/login`, so the launcher appears again,
  and the server can be changed there. That is the reachable "change server" and "sign out" path.
* The tap on the button: a command callable only from the launcher (the existing second lock), taking no
  argument. It starts the attempt and opens the system browser.

Weighed and **not** built: a `dynodoc://open/launcher` deep link (with an Android shortcut to it). Any
program or web page could then bring up the launcher at any time; the parser table, the manifest filters
and the checker would each grow a row; and on desktop it would have to be refused separately because the
desktop launcher holds the key operations. The sign-out route reaches the same screen without it. Left
open: a saved server that cannot be reached (a mistyped self-hosted address) still shows the offline page
with "Try again" and no way out except clearing the app's data. If the owner wants a way out, the
launcher deep link plus an Android long-press shortcut is the smallest one.

## Threat table

| Threat | What stops it | What is left |
|---|---|---|
| **A program that registered `dynodoc://` takes the code** (Android and desktop allow it; App Links and universal links do not exist yet) | PKCE: the code is bound to `SHA-256(verifier)`; the verifier exists only in the app's memory and in the fragment of one navigation to the configured server. 60 s, once, a failed redeem burns it | the interceptor can burn the code (the person taps again), and on Android a chooser may show if two apps claim the scheme. App Links later replace the custom scheme (needs the release signing key; not done) |
| **Replay** of the code or of the link | row lock and used-once trigger; the app's attempt is used up at the first accepted link, and a second copy finds nothing pending | none known |
| **Login CSRF through `/app-signin`** (a link makes the victim's browser hand a session to the attacker's app) | the page needs the visitor's own session, shows who and which device, and makes nothing until Continue, a same-origin JSON request (Origin must equal the site, JSON only); framing is refused (`frame-ancestors 'self'`) | a victim who reads the page and presses Continue anyway |
| **Forced login through a crafted `/app-signin/finish` link** (the attacker mints a code and verifier for his own account and gets the victim to open the finish page with them) | the pair is single-use and short-lived; the attacker needs an account and a started flow of his own | **Not closed.** The victim's browser or app window becomes signed in as the attacker, and the web app shows the account in use. A later fix would need the server to tell the shell's window from a browser, which no unforgeable signal exists for today |
| **Session fixation** | a new login is made at redeem (new session id and refresh lineage); nothing the visitor supplied becomes a session | none |
| **Open redirect** | the return address after sign-in is accepted only as the canonical form of a valid request; the app link is built from validated parts with a fixed scheme and host | none |
| **Code and verifier in logs, Referer or history** | the code exists only in a POST body, an API answer, the app link and the one script that sets the fragment (never in a URL a server sees); the verifier never leaves the app but in that script and the redeem POST; the window is sent to a plain address, so the web view's own log and history never hold either; `no-referrer`, `no-store`; the fragment is removed from the address at once; the shell's debug log strips queries and fragments; tests assert none of it appears in the API log, the audit rows, the ledger, any request, the shell's debug log or (Android emulator) the device log | Android's activity manager may log the address of the app link's activity start (code and state, never the verifier); only adb and developer tools read that log, and the code is useless without the verifier |
| **Brute force of the code** | 256 bits; rate limits; burning on failure | none |
| **Verifier leakage** | memory only, never written; no `Debug`/`Display`/`Clone` on it; sent only to the configured origin over https (http only for localhost); in a fragment | malware that reads the app's memory |
| **A malicious page triggers the flow** | remote pages have no commands; the only way to start is the launcher button or the desktop menu item | a page can make the launcher appear on a phone (above) |
| **Concurrent redeem** | `select ... for update` then update in one transaction; 8 simultaneous redeems of one code had exactly one winner, 5 rounds | none |
| **Clock skew** | expiry judged only by the database clock; the app uses a monotonic clock | none |
| **The person cancels** | Cancel makes no request; the app's attempt dies after 10 minutes or when replaced; an issued code dies in 60 s | none |
| **Several devices** | each attempt has its own challenge and state; each redeem is its own login; at most 3 unused codes per account | the limit is per account, so a fourth tap within a minute is refused with a plain message |
| **The app is ended while the browser is open** | | the attempt is memory only, so the person must start again (a phone can end a background app). Keeping the verifier in the Android Keystore would remove this |
| **Hostile deep links** | the table and the exact-whitelist tests (over 400,000 generated links each) | none known |
| **The configured server is hostile** | it already receives everything the flow gives it, and can already mint sessions for itself | none beyond choosing a server |

## What was run

All on one Windows 11 PC, 22 October 2026. Nothing ran in GitHub Actions (the private repository's
Actions cannot start). The branch includes `main` as of its SMTP slice.

* **API, real PostgreSQL 16:** 18 integration tests (`crates/engine-api/tests/app_handover_test.rs`: happy
  path, wrong verifier, wrong state, expired, replayed, 5 rounds of 8 simultaneous redeems with exactly one
  winner, 8 simultaneous issues, a code for A redeemed with B's bearer, sign-out between issue and redeem,
  the limit of three, both routes without the service key, malformed requests, per-client and per-prefix
  rate limits, no secret in the ledger, audit rows or log, the restricted runtime login) and 6 unit tests.
  `RuntimeDatabase` runs `grant_runtime` and `check_runtime` with the new table. Workspace:
  `cargo test --workspace --locked --no-fail-fast` 1,332 passed and 1 failed, the known Windows-only
  `sse_delivers_a_new_event_within_100ms`; `cargo fmt --check`, `clippy --workspace --all-targets -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc`, OpenAPI regenerated with no drift.
* **Web:** vitest 86 files, 962 passed, 4 skipped (new: shapes, routes, headers, safe return address);
  eslint, prettier, svelte-check (0 errors), build, hosted-string audit; Chromium: 11 tests in
  `e2e/app-signin.spec.ts` against an isolated stack with a recording proxy in front of the web server and
  another between it and the API (no code or verifier in any address, header or page; the code only in the
  one answer that makes it), 1 in `selfhosted-noemail.spec.ts`, and the login, session, security and smoke
  specs (22 in the run).
* **Shell:** 137 unit tests (the exact whitelist (over 400,000 generated links each for the page links and
  for the sign-in link), hostile links, state/verifier/expiry with an injectable clock, replay, the outbox and
  its script); format, clippy, Android `cargo check` and clippy; 44 node tests (manifest checker with the
  new filter, fixtures regenerated from real `aapt2` output); `signin-probe.mjs` against the real Windows
  shell (37 checks: menu and launcher button, real single-instance forwarding of links, PKCE consistency,
  no secret in any request or log, replay, wrong state, a server page cannot start it).
* **Android emulator** (x86_64, Android 16, WebView 133; the hosted cloud as a stand-in server, because the app
  forbids cleartext traffic, `http://localhost` included): the launcher appears by itself when the server's
  sign-in page has loaded; its button hands the system browser (Chrome) `/app-signin?...`; Android routes
  `dynodoc://signin?...` through the new manifest filter; a link with nothing pending or the wrong state is
  refused; the right one takes the window to the finish page; `SHA-256(verifier)` is the challenge; a replay
  does nothing; the verifier is not in the device log; a hosted page's call to the new command is refused.
* **Manual mutations (each fails at least one test):** API: verifier check removed, single use removed,
  state check removed, expiry removed, code stored in clear, table missing from the runtime allowlist,
  failed redeem no longer burns, outstanding limit removed, service key not required, sign-out does not
  kill a code, per-prefix limit removed. Shell: state check removed, expiry removed, attempt not used up,
  server binding removed, challenge not the hash, any parameter order accepted, secrets in the query,
  launcher check skipped, a wrong-state link cancels the attempt. Web: Origin check removed, any return
  address accepted, unchecked code in the link, fragment left in the address (browser test).

## Not done, on purpose

* Android App Links and iOS universal links (they need the release signing key and the final identifiers).
* The iPhone app, and a physical phone.
* Keeping the verifier across the app being ended.
* Telling the shell's window from a browser at the finish page.
* The launcher deep link and shortcut.
