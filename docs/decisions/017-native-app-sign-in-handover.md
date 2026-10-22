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
   YES: the attempt is used up now; the window goes to
        <server>/app-signin/finish#code=..&state=..&verifier=..          (a FRAGMENT)
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
| `GET /app-signin/finish` (web) | anyone | static page; the script reads `#code=..&state=..&verifier=..`, removes it, posts it | 200 |
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
7. **The finish page also listens for `hashchange`.** When the app opens it a second time while it
   still shows (after a failure) the browser changes only the fragment; found by the real-shell probe
   and the second-try browser test.
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
| **Code in logs, Referer or history** | the code exists only in a POST body, an API answer and the app link (never in a URL a server sees); `no-referrer`, `no-store`; the fragment is removed from the address at once; the shell's debug log strips queries and fragments; tests assert none of it appears in the API log, the audit rows, the ledger or any request | the in-memory back list of the web view holds the original address of the finish page until the page is left; by then the code is used up |
| **Brute force of the code** | 256 bits; rate limits; burning on failure | none |
| **Verifier leakage** | memory only, never written; no `Debug`/`Display`/`Clone` on it; sent only to the configured origin over https (http only for localhost); in a fragment | malware that reads the app's memory |
| **A malicious page triggers the flow** | remote pages have no commands; the only way to start is the launcher button or the desktop menu item | a page can make the launcher appear on a phone (above) |
| **Concurrent redeem** | `select ... for update` then update in one transaction; 8 simultaneous redeems of one code had exactly one winner, 5 rounds | none |
| **Clock skew** | expiry judged only by the database clock; the app uses a monotonic clock | none |
| **The person cancels** | Cancel makes no request; the app's attempt dies after 10 minutes or when replaced; an issued code dies in 60 s | none |
| **Several devices** | each attempt has its own challenge and state; each redeem is its own login; at most 3 unused codes per account | the limit is per account, so a fourth tap within a minute is refused with a plain message |
| **The app is ended while the browser is open** | | the attempt is memory only, so the person must start again (a phone can end a background app). Keeping the verifier in the Android Keystore would remove this |
| **Hostile deep links** | the table and the exact-whitelist tests (over 490,000 generated links each) | none known |
| **The configured server is hostile** | it already receives everything the flow gives it, and can already mint sessions for itself | none beyond choosing a server |

## What was run

The figures are filled in by the final report of the round; the ledgers record them. Nothing here ran in
GitHub Actions (the private repository's Actions cannot start).

## Not done, on purpose

* Android App Links and iOS universal links (they need the release signing key and the final identifiers).
* The iPhone app, and a physical phone.
* Keeping the verifier across the app being ended.
* Telling the shell's window from a browser at the finish page.
* The launcher deep link and shortcut.
