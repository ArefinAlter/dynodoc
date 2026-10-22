# Decision 017: signing the native apps in from the system browser

Date: 22 October 2026 (first build and an independent adversarial review the same day; this text
includes the review's fixes). Owner: Verne. Status: built and checked on this PC as listed under "What
was run"; migration 0046 never rehearsed against a production database (it was edited in place twice
before it ever ran anywhere shared, which is allowed; see "Routes"), never tried on a physical phone,
iOS not started, and the browser sign-in is offered only on Windows and Android (see "Forced login").
A second review round (the shipped proxies, windows, frames, version grammar) is recorded at the end of
the refinements.
Refines item 6 of [decision 016](016-devices-processors-desktop-and-mobile-apps.md).

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
adds one thing that does travel: a one-time **hand-over code**. It is not a credential on its own: it
needs a second value (the PKCE verifier) that never leaves the app but in one script and one POST body,
it works once, and it expires after 60 seconds. So the line "never carry tokens" becomes "never carry a
token, and carry a hand-over code only in the one fixed link shape `dynodoc://signin?code=..&state=..`".
The deep-link table in `apps/desktop/src-tauri/src/deeplink.rs` has exactly one more row, and the
exact-whitelist tests pin it.

The design is the one RFC 8252 (OAuth for native apps) describes, with PKCE (RFC 7636) so that a
program that only takes the code out of the link learns nothing it can use. **What that does not
cover** (the first version of this text said "useless without the verifier" without qualification, which
overstated it): a program on the same device that starts a request of its own (its own verifier,
challenge and state) and also receives the link has everything the app would have. On Android two apps
can both claim a custom scheme. The pairing code (below) lets the person catch that; verified App Links
are the real fix and are not built (they need the release signing key).

## The flow

```text
 app (shell)                    system browser                    server (web tier + API)
 -----------                    --------------                    -----------------------
 1 verifier = 32 random bytes
   challenge = b64url(SHA-256(verifier))
   state = 24 random bytes
   keep them in memory only; the launcher shows
   the PAIRING CODE (4 characters of SHA-256(state))
   open  <server>/app-signin?challenge=..&state=..&platform=..  ->  page: validated; if no
                                                                      session: normal sign-in,
                                                                      back to the SAME request
                                                                   page shows who is signed in,
                                                                   the device, the pairing code,
                                                                   Continue / Cancel
 2                              Continue = a real FORM POST of that page (top-level navigation,
                                user-activated, same origin: the browser says so in Sec-Fetch-*)
                                                                ->  API: code (256 bits), only its
                                                                      hash stored, bound to account,
                                                                      challenge, state, platform,
                                                                      the login that asked; 60 s;
                                                                      once; <= 3 unused
                                <- 200 page: location.assign(dynodoc://signin?code=..&state=..)
                                   and an "Open the Dynodoc app" button with the same link
 3 <- dynodoc://signin?code=..&state=..   (cold or warm start; desktop: single-instance forward)
   strict parser; attempt pending? fresh (10 min by both clocks)? same server? state equal
   (constant time)?
   YES: the attempt is used up now; the window goes to the PLAIN address
        <server>/app-signin/finish           (no secret in any address the web view requests)
   page has loaded AND says it is ready: the shell sets location.hash = #code=..&state=..&verifier=..
   (a script that first checks the page's origin and path; once, only to that page of the server
   the attempt was made for, within 30 s; a fragment is never sent to a server)
 4 finish page: only in the app's own window (User-Agent token + the shell's bridge object);
   ignores any fragment present when it loaded; on the CHANGE the shell causes, removes the fragment
   from the address and POSTs {code, state, verifier} to its own server
                                                   ->  POST /auth/app-signin/redeem (needs the token)
                                                   -> API: row locked; live, unused, account still
                                                      signed in at the same generation, kind of app
                                                      as asked, SHA-256(verifier) = challenge, state
                                                      equal (constant time); used up and the login
                                                      made in ONE transaction, as a magic-link
                                                      sign-in makes it
   <- session cookie set in the app's window  ->  /workspace
```

### Bounds, all checked on both sides and again by the API

| Value | Made by | Shape |
|---|---|---|
| verifier | app, operating-system randomness | 32 bytes, 43 characters of base64url; the API accepts 43 to 128 of `A-Za-z0-9-._~` (RFC 7636) |
| challenge | app | `base64url(SHA-256(verifier))`, exactly 43 characters, canonical spelling (the last character carries four bits) |
| state | app, operating-system randomness | 24 bytes, 32 characters; accepted 16 to 64 of `A-Za-z0-9_-` |
| code | API | 32 random bytes as 64 lower-case hexadecimal digits; stored as SHA-256 only |
| platform | app | `desktop`, `android` or `ios`; shown to the person as "computer", "Android phone", "iPhone"; the app's User-Agent token names the same kind and the API compares the two at redeem |
| pairing code | app and web page, same function | 4 characters of `alphabet[SHA-256(state)[i] & 31]` over `23456789ABCDEFGHJKLMNPQRSTUVWXYZ` (no 0, 1, I, O); not a secret, proves nothing to a server |
| app token | app | `DynodocApp/<major.minor.patch> (desktop\|android\|ios)` at the end of the web view's own User-Agent |
| life | | code 60 s (database clock only); attempt and the 30-second delivery of the fragment: the larger of the monotonic and the wall clock (Android's monotonic clock stops while the phone sleeps); at most 3 unused codes per account; one attempt per running app |

### Routes

| Method and path | Who may call | What it does | Answers |
|---|---|---|---|
| `GET /app-signin?challenge=&state=&platform=` (web) | anyone; needs a browser session to go on | validates the three values (exactly these, each once). Not signed in: `/login?next=` with the request rebuilt in one canonical spelling, then back. Signed in: the confirmation page. Installation with no sign-in method: says so, no redirect | 200 page; invalid request: 200 "not valid" page; never a redirect to anywhere else |
| `POST /app-signin?challenge=&state=&platform=` (web form action) | a signed-in browser, by a real click | Continue. Accepted only when the browser reports a top-level, user-activated navigation of this site (`Sec-Fetch-Site: same-origin`, `-Mode: navigate`, `-User: ?1`, `-Dest: document`), `Origin` equals the site, a form content type; SvelteKit's own CSRF check runs first. Calls the API with the person's own access token and the service key | 200 page with the app link; a script, another client or an older browser gets a refusal that says why ("use an up-to-date browser" when the headers are absent: it fails closed); 429 too many unused codes. There is no JSON route any more |
| `GET /app-signin/finish` (web) | the app's own window | works only when the request carries the app's User-Agent token (checked on the server) and the shell's bridge object exists in the page (checked in the page); anywhere else it says "open this from the Dynodoc app" and never reads a fragment. A fragment present at load is ignored and removed; only a change after the page says it is ready (`data-finish-ready`) is acted on; with none it waits six seconds, then says it did not finish | 200 |
| `POST /auth/app-signin/redeem` (web) | same-origin JSON, with the app's User-Agent token | validates, passes the kind of app to the API, calls it, sets the encrypted session cookie | 200 `{ok:true}`; every failure, a missing token included, is the same 401 and the same words |
| `POST /auth/app-handover` (API) | the web server only (service key) AND an account login's bearer token (device, agent and share or view tokens are refused) | makes the code, remembers the login that asked; audit `auth.app_handover.issued` | 200 `{code, expires_in}`; 400, 401, 403, 429 |
| `POST /auth/app-handover/redeem` (API) | the web server only (service key); the app's own credentials are the body (unknown fields refused) | redeems, with the kind of app it was asked for; audit `.redeemed` (with the new login's id) or `.refused` | 200 tokens as `/auth/verify` returns them; 401 for every refusal; 429 over a rate limit |

The two web POST routes that stay under `/auth/` do so on purpose: the production proxy already
rate-limits that path per client address, and the no-store and no-referrer headers already apply to it.
The confirmation page itself shares its address with its own site only (`Referrer-Policy: same-origin`):
a POST under `no-referrer` carries `Origin: null`, which the form check would refuse.

Migration `0046_app_handover.sql` adds one table, `app_handover_code`: code hash, challenge, state,
platform, account, session generation, the login (session) that asked, times, and a "used" mark that a
trigger lets move from unused to used exactly once. It was **amended in place** after the review
(the login that asked, two indexes, the guard trigger covering the new column): it had never run in any
shared environment, which is the condition `AGENTS.md` sets for editing a migration. If it has run
anywhere by the time this is read, the change must become a new numbered file instead. **Any local or
test database that applied the first version of 0046 must be reset** (sqlx records a checksum of every
applied migration and refuses a changed one: `task migrate-reset`, development databases only); this
branch's own test databases are created fresh each run.
`engine-core/src/database.rs` lists it for the restricted runtime login (insert, update, delete; the
trigger refuses every other edit). The retention job (`retention.rs`, one more spec, with its own test)
deletes rows that expired more than `RETENTION_AUTH_DAYS` ago; the audit rows stay. Nothing else
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
6. **Continue is a real form post that only a browser can make.** (The first version was a JSON
   request made by the page's script, which any script on the origin, or any client holding the
   session cookie, could repeat.) It is now a top-level form POST, accepted only with the `Sec-Fetch-*`
   headers a browser sets for a user-activated, same-origin navigation: script cannot set them (they
   are forbidden header names), and a script's own `fetch`, `XMLHttpRequest` or `form.submit()` without
   a click carries other values or none. A browser that sends none of them (an old one) is refused with
   the words "use an up-to-date browser": it fails closed. The answer is a 200 page, not a redirect (a
   redirect to `dynodoc://` is stopped by the page's own `form-action 'self'` policy in Chrome, which
   checks redirects); its script calls `location.assign(dynodoc://...)` and an "Open the Dynodoc app"
   button repeats it. Cancel makes no request at all. Tested with real clicks and with scripted
   `fetch`, XHR and a script-submitted form (the last needs the activation to have lapsed: Playwright
   counts every call into a page as a gesture).
7. **The secrets are in no address the web view requests.** The sketch put code and verifier in the
   fragment of the address the window is sent to. On the Android emulator the verifier then appeared in
   the device log: Android's web view logs the page's address next to every console message (Tauri
   forwards them as `Tauri/Console`, and its own start-up scripts raise such messages on every page).
   So the shell sends the window to the plain `/app-signin/finish`, and once the page has finished
   loading sets `location.hash` from an in-memory outbox (`signin::Outbox`): once, only to that exact
   page of the configured server, within 30 seconds, and only as a script made from the three values
   (`signin::finish_script` refuses every other character). The page reads the fragment on
   `hashchange`, which also covers the app opening it a second time while it still shows (after a
   failure the browser changes only the fragment and runs no page load; found by the real-shell
   probe). After this change the emulator log held neither the verifier nor the code or state in the
   app's own lines.
8. **Windows adds a slash after the query** of a custom-scheme link. The parser tolerates exactly one
   at the very end, as it already did after the path.
9. **Rate limits.** Per code prefix (first eight characters), five failures in 15 minutes; per client
   address, 20 failures, **only when the web server's view of the address tells visitors apart** (a
   proxy that hides it would make one bucket for everyone, and then one visitor could lock all others
   out). "Tells apart" means: not loopback, private, link-local, shared (CGNAT), unspecified, multicast
   or malformed, in ANY spelling (`0:0:0:0:0:0:0:1`, `::ffff:7f00:1`, `::ffff:127.0.0.1`,
   `fe80::1%eth0`, NAT64 and 6to4 forms carrying such an address); tested. Without
   `ADDRESS_HEADER` and `XFF_DEPTH` (the number of proxies between the visitor and the web server,
   never more) the per-client limit is therefore **off**, which the deployment guide now says. Successful
   redeems cost nothing. The edge proxy's own limit applies to `/auth/`. A person who presses Continue
   with three codes waiting writes one audit row a minute, not one per press.
10. **Phones: no launcher deep link, no native control.** See the next section.
11. **Forced login is closed, in four layers** (see the threat table). The server only acts for the
    app's own window: a `DynodocApp/<version> (kind)` token appended to the web view's own
    User-Agent (WebView2 through `ICoreWebView2Settings2`; Android in Kotlin; checked by the finish
    page's load, the redeem route and, as the kind of app, the API), plus the shell's bridge object in
    the page. The finish page ignores a fragment present at load and acts only on a change the shell
    causes after the page says it is ready. The navigation rule refuses any page-initiated navigation
    to `/app-signin/finish` with a query, a fragment or another spelling, `window.open` and a redirect
    included. The shell's script checks the page's origin, path and an empty address before it sets
    anything. Where a web view's User-Agent cannot be extended and the change has not been run
    (macOS, Linux, iOS) the browser sign-in is **not offered** (the launcher hides the button and
    the command says so) rather than offered and refused.
12. **A pairing code ties the page to the app that asked.** Four characters of SHA-256(state), shown
    by the launcher while its request waits and by the confirmation page next to Continue and Cancel.
    A program that started a request of its own cannot make the person's app show the same four; the
    person who sees different ones, or none, presses Cancel. It is a human check, not a proof: it helps
    only a person who looks, and the real fix for a program that claims the link scheme is App Links.
13. **The outbox and the attempt belong to one server.** The fragment waiting for the finish page
    remembers the origin it was made for and is handed only to that server's finish page; a change of
    server clears both it and the pending attempt (the attempt also still refuses another origin on
    its own, so this is a second lock).
14. **The sign-in request has its own hand-off budget** (one per two seconds, six a minute), apart
    from the click budget a page holding the click secret could spend, and the page budget.
15. **Time is the larger of two clocks.** An attempt, and the 30 seconds the fragment may wait, expire
    by whichever of the monotonic and the wall clock says more: Android's monotonic clock does not run
    while the phone sleeps, so a monotonic-only check could keep a ten-minute attempt alive for hours.
    A wall clock set backwards shortens nothing; one set forwards expires an attempt early, which only
    costs a retry.
16. **Signing out ends what the login asked for.** An unused code remembers the login that asked for
    it; a plain `/auth/logout` of that login ends its unused codes in the same transaction (the
    sign-out-everywhere and a disabled account already did). The `redeemed` audit row names the login
    it made, so a hand-over can be followed to its session. Constant-time comparisons use `subtle`
    and no longer zero-pad (`"abc"` and `"abc\0"` were equal under the first version's HMAC trick).
17. **Secrets are overwritten when dropped** where a crate already in the tree allows it: the API's
    redeem request (code, state and verifier, `zeroize`), and in the shell every buffer that held a verifier,
    state or fragment (plain stores kept by `black_box`; the shell has no `unsafe` and adds no
    dependency for this). The copies a web view makes of a script it runs are out of reach.

### Second review round: what only a real edge and real browsers showed

18. **The Continue form was refused behind both shipped proxies, and no test could see it.**
    `infra/demo/nginx-app.conf` and the bundled proxy hide the application's `Referrer-Policy` and
    force `no-referrer`, so the `same-origin` the application sends for `/app-signin` never reached the
    browser; under `no-referrer` a same-origin form POST carries `Origin: null` (Chromium, Firefox and
    WebKit all do), and the form check refused it (SvelteKit's own CSRF check would too). The
    browser tests ran without a proxy. The confirmation page now carries
    `<meta name="referrer" content="same-origin">`, which overrules the header, so the form sends its
    real Origin. It is tested **through** both configurations: `infra/selfhosted/test/proxy-app-signin.mjs`
    runs the hosted `nginx-app.conf` (rewritten only for server name, certificate and upstream, each
    rewrite asserted) and the bundled proxy image in front of the real built web app and the real API
    and drives Chromium (12 checks), and `scripts/smoke.mjs` runs the same browser checks (h3 to h7)
    through the full bundle. Removing the meta fails them.
19. **A script on an infected page could read a minted code through a popup.** With two real inputs
    it opens a same-origin window by name, posts the Continue form into it and reads the answer from
    the window it holds (found in Chromium and Firefox; every request is `dest=document`, `user=?1`).
    `/app-signin` and `/app-signin/finish` now send `Cross-Origin-Opener-Policy: same-origin`, so the
    answer lands in a window group of its own and the opener cannot read it. Both proxies pass the
    header through unchanged (tested through them). **Residual:** the same script can still make the
    victim's browser mint codes blind (two real inputs, nothing read); such a code goes only to the
    victim's own `dynodoc://` handler and is useless without a verifier the script does not have. A
    script on the confirmation page itself shares the page's own window group and is out of scope:
    it already runs with the person's session.
20. **WebKit sends no `Sec-Fetch-User`, so Safari could never press Continue.** Not reachable today
    (the browser sign-in is not offered on macOS, and there is no iPhone app), so the check is **not
    relaxed**; the refusal page says plainly that this browser may not support the confirmation step
    and to use Chrome, Edge or Firefox, in words that give a script nothing (a script-submitted form
    without activation looks the same to the server). **Prerequisite of the iOS slice:** for the
    iPhone app the proof of a real, person-made confirmation cannot be a browser header. It must
    change, for example to the person typing the pairing code shown by the app into the confirmation
    page, which makes the pairing code (item 12) mandatory instead of advisory, and removes the
    reliance on `Sec-Fetch-User`.
21. **The sign-in pages are never framed.** A plain link whose `target` names a frame can feed a
    framed finish page a fragment with no script at all, and in the shell the frame would carry the
    app's token and bridge object. `/app-signin` and `/app-signin/finish` send `X-Frame-Options:
    DENY` and `frame-ancestors 'none'` (both proxies overwrite the application's `X-Frame-Options`,
    so their frame-option map says `DENY` for the two paths, in the hosted file and the bundle's
    `maps.conf`, which a smoke check keeps byte for byte equal), and the finish page and the shell's
    script both do nothing when they are not the top window. Tested with the headers removed.
22. **The confirmation form is rate limited at the proxy.** Moving Continue out of `/auth/` also took
    it out of the proxy's 60-a-minute-per-address limit. Both configurations now have a zone of their
    own for `POST /app-signin` (an empty key for any other method, so no GET is counted); the smoke
    test checks both configurations' text and the harness checks a 429 through each.
23. **One grammar for the version in the token.** The server accepted three digits per number, the
    Android layer six, and the Rust shell passed Cargo's raw version, so a version such as
    `2026.10.22` or `0.1.0-beta.1` gave a token the server silently refused. Now: `<major>.<minor>.<patch>`,
    each one to nine ASCII digits; a producer keeps the leading dotted triple of its version and
    **drops whatever follows** (a pre-release or build suffix, a fourth number), and writes `0.0.0`
    when there is none. One fixture file (`apps/desktop/fixtures/app-token.json`: `2026.10.22`,
    `0.1.0-beta.1`, `10.0.0+build5`, `1.2.3.4`, empty, very long, full-width digits ...) is run by
    the Rust shell, the Android JVM test and the web app's tests.
24. **What the User-Agent token proves.** Nothing about identity. It is a speed bump: a crafted link
    or a page cannot make an ordinary browser send it on a navigation, so the finish page and the
    redeem route refuse such a request. It is **not authentication**: any program can send the header,
    and the kind of app the API compares with the one the code was asked for comes from that same
    client-controlled header, relayed by the web tier. What keeps a code from being useful to another
    program is the verifier, not the token.

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
| **A program that registered `dynodoc://` only takes the code out of the link** (Android and desktop allow it; App Links and universal links do not exist yet) | PKCE: the code is bound to `SHA-256(verifier)`; the verifier exists only in the app's memory and in the one script and POST body that finish the sign-in. 60 s, once, a failed redeem burns it | the interceptor can burn the code (the person taps again), and on Android a chooser may show if two apps claim the scheme |
| **A program on the device starts a request of its own and also receives the link** (its own verifier, challenge and state, opened in the person's browser; the person, signed in there, sees the confirmation page and presses Continue; the link goes to the program, which holds the verifier) | PKCE does **not** stop this: "useless without the verifier" is true only of a program that did not start the request. What helps: the page names the account and the device and shows a **pairing code** the real app also shows in its launcher while it waits, so a person who looks sees a mismatch (or no code in the app) and presses Cancel; the code is a human check, not a proof | **Open for a person who does not compare, and for a program that claims the scheme.** The real fix is verified Android App Links and iOS universal links, so that only the app signed with our key can receive the link. Not built: it needs the release signing key and the final identifiers |
| **A script on an infected page reads a code minted into a popup** | `Cross-Origin-Opener-Policy: same-origin` on both pages and the form's answer (refinement 19), tested in the browser through both shipped proxies; the browser tests show the read fails | blind minting by two real inputs (the code goes only to the victim's own handler and needs a verifier); a script on the confirmation page itself |
| **The Continue form refused behind a proxy** (`Origin: null` under `no-referrer`) | the page's own `<meta name="referrer">` (refinement 18); tested through both shipped proxies | a proxy that also rewrites the HTML; none known |
| **Replay** of the code or of the link | row lock and used-once trigger; the app's attempt is used up at the first accepted link, and a second copy finds nothing pending | none known |
| **Login CSRF through `/app-signin`** (a link makes the victim's browser hand a session to the attacker's app) | the page needs the visitor's own session, shows who, which device and the pairing code, and makes nothing until Continue, a real user-activated top-level form post of this site (`Sec-Fetch-*` and Origin, which script cannot set; an old browser that sends none is refused); framing is refused (`frame-ancestors 'self'`); no JSON route exists to script | a victim who reads the page and presses Continue anyway |
| **Forced login through a crafted `/app-signin/finish` link** (the attacker mints a code and verifier for his own account and gets the victim's browser or app window to open the finish page with them) | **Closed, by four independent layers** (each has a test and a mutation that fails it): (1) the server acts only for a request carrying the app's User-Agent token, which a page cannot make a browser send on a navigation, and says so to anything else; (2) the page also needs the shell's bridge object, ignores a fragment present at load, and acts only on a change the shell causes after the page says it is ready; (3) the navigation rule refuses a page-initiated navigation (link, script, `window.open`, redirect) to `/app-signin/finish` with a query, a fragment or another spelling; (4) the shell's script checks origin and path before it touches the address. A normal browser with a valid triple in the fragment does not sign in, even after a later change of the fragment (browser test: the session cookie stays) | The token is not a secret: a native program can send it, but a native program already runs with the person's rights and cannot reach the browser's cookies through this. On Windows the request of a navigation the shell cancels may still reach the server (WebView2 sends it first; the window does not move). Whether Android's web view reports a server redirect or a history step to the navigation rule was not tested, so there layers (1) and (2) are what is relied on (they do not depend on it). Not offered at all where the token cannot be added: macOS, Linux and iOS. A server redirect to the finish page was **not tried on Android** (the hosted cloud has no endpoint that redirects there); a page-initiated navigation to it was (the probe's check). A framed finish page is refused by headers, by the page and by the shell's script (refinement 21) |
| **Session fixation** | a new login is made at redeem (new session id and refresh lineage); nothing the visitor supplied becomes a session | none |
| **Open redirect** | the return address after sign-in is accepted only as the canonical form of a valid request; the app link is built from validated parts with a fixed scheme and host | none |
| **Code and verifier in logs, Referer or history** | the code exists only in the page that answers the Continue form (the app link), the API's answer to the web server, a POST body, the app link and the one script that sets the fragment (never in a URL a server sees); the verifier never leaves the app but in that script and the redeem POST; the window is sent to a plain address, so the web view's own log and history never hold either; `no-referrer`, `no-store`; the fragment is removed from the address at once; the shell's debug log strips queries and fragments; tests assert none of it appears in the API log, the audit rows, the ledger, any request, the shell's debug log or (Android emulator) the device log | Android's activity manager may log the address of the app link's activity start (code and state, never the verifier); only adb and developer tools read that log, and the code is useless without the verifier |
| **Brute force of the code** | 256 bits; rate limits; burning on failure | none |
| **Verifier leakage** | memory only, never written; no `Debug`/`Display`/`Clone` on it; overwritten when dropped; sent only to the configured origin over https (http only for localhost); in a fragment, set by a script that first checks the page's origin and path | malware that reads the app's memory; the copies a web view makes of a script it runs |
| **A malicious page triggers the flow** | remote pages have no commands; the only way to start is the launcher button or the desktop menu item | a page can make the launcher appear on a phone (above) |
| **Concurrent redeem** | `select ... for update` then update in one transaction; 8 simultaneous redeems of one code had exactly one winner, 5 rounds | none |
| **Clock skew, and a sleeping phone** | a code's expiry is judged by the database clock only; the app's attempt and the fragment's delivery window use the larger of the monotonic and the wall clock | a wall clock set forward ends an attempt early (a retry) |
| **A page floods hand-offs so the sign-in button stops working** | the sign-in request has its own budget; the click budget (which a page holding the click secret could spend) and the page budget cannot touch it, and it cannot touch them | none |
| **The server is changed while a request waits** | changing the server clears the pending attempt and the fragment waiting in the outbox; each also compares the origin again when used | none |
| **The login that asked signs out while a code waits** | the code remembers that login; a plain logout ends its unused codes in the same transaction | the code is 60 seconds old at most anyway |
| **The person cancels** | Cancel makes no request; the app's attempt dies after 10 minutes or when replaced; an issued code dies in 60 s | none |
| **Several devices** | each attempt has its own challenge and state; each redeem is its own login; at most 3 unused codes per account | the limit is per account, so a fourth tap within a minute is refused with a plain message |
| **The app is ended while the browser is open** | | the attempt is memory only, so the person must start again (a phone can end a background app). Keeping the verifier in the Android Keystore would remove this |
| **Hostile deep links** | the table and the exact-whitelist tests (over 400,000 generated links each) | none known |
| **The configured server is hostile** | it already receives everything the flow gives it, and can already mint sessions for itself | none beyond choosing a server |

## What was run

All on one Windows 11 PC, 22 October 2026, in two rounds (the build, then an independent adversarial
review whose fixes this text includes). Nothing ran in GitHub Actions (the private repository's Actions
cannot start: billing). The branch includes `main` as of 0257d46 (the self-hosted bundle).

* **API, real PostgreSQL 16:** 25 integration tests in `crates/engine-api/tests/app_handover_test.rs`
  (including, added after the review: a downgrade to the plain method, the kind of app, unknown fields on both
  routes, device, agent and share or view tokens refused at the issue route, a plain logout ending its
  codes, the login named in the `redeemed` audit row, one audit row a minute for the button pressed
  with three codes waiting, redeem and login in one transaction) and the retention test; unit tests in
  `handover.rs` (exact comparison, no zero-padding). `cargo test --workspace --locked --no-fail-fast`:
  1,338 passed, 2 failed: the known Windows-only `sse_delivers_a_new_event_within_100ms`, and a
  retention unit test that had not been given the new spec's window (fixed; the 96 library tests,
  `app_handover_test` and `retention_test` were rerun green). `cargo fmt --all --check`, `clippy
  --workspace --all-targets -D warnings`, `RUSTDOCFLAGS=-D warnings cargo doc`, and the OpenAPI file
  byte-identical to the generated one. `RuntimeDatabase` runs `grant_runtime` and `check_runtime` with
  the table (the browser-test stack was set up with them).
* **Web:** vitest 86 files, 970 passed, 4 skipped; eslint, prettier, svelte-check (0 errors), build,
  hosted-string audit. Chromium against an isolated stack with a recording proxy in front of the web
  server and another between it and the API: 14 tests in `e2e/app-signin.spec.ts` (an ordinary browser
  and the token-only and bridge-only look-alikes sent to the finish page with a valid triple in the
  fragment, also changed after the load, sign nobody in and leave the session cookie alone; the
  fragment at load is ignored in the app's window; redeem without the token is the one 401; scripted
  `fetch`, XHR, a script-submitted form and another client cannot make a code, a real click can; the
  pairing code on the page equals the app's; Cancel makes no request; no code or verifier in any
  address, header or page a server saw), plus the security, session-revocation, smoke and launch specs
  (48 passed in that run) and 3 in `selfhosted-noemail.spec.ts`.
* **Shell:** 154 unit tests (the exact deep-link whitelist; hostile links; state, verifier and expiry
  with injectable clocks, including a phone that slept; the outbox bound to a server; the finish-page
  navigation rule in every spelling; the three hand-off budgets; the script's charset with a 43-character
  verifier that is valid except for `";x;"`; source checks that pin how `window.rs` wires it); fmt, clippy
  `-D warnings`; 51 Node tests (finish script 7, link script 12, blank targets 6, Android hardening 8,
  manifest checker 18); `signin-probe.mjs` against the real Windows shell, **53 checks** (menu and
  launcher button, real single-instance forwarding of links, PKCE consistency, no secret in any request
  or log, replay, wrong state, a server page cannot start it, the window's User-Agent is WebView2's own
  with `DynodocApp/<version> (desktop)` appended, the shell waits for the page's ready marker, the
  pairing code in the launcher, seven ways a page tries to send the window to the finish page with
  something in the address); `security-probe.mjs` 79 checks and `smoke.ps1` unchanged and green.
  WebView2 sends the request of a navigation the shell then cancels (the README already says so), so
  the probe's check is that the window keeps its document, not that the server sees nothing.
* **Android emulator** (x86_64, Android 16, WebView 133; the hosted cloud as a stand-in server, because
  the app forbids cleartext traffic, `http://localhost` included): rebuilt after the review (the Kotlin `AppToken`, 3 JVM tests pass), the `signin` phase of
  `android-probe.mjs` has 18 checks, all passed: the launcher appears by itself when the server's
  sign-in page has loaded; its button hands the system browser (Chrome) `/app-signin?...` and shows the
  request's pairing code (the same four characters the web page computes); Android routes
  `dynodoc://signin?...` through the manifest filter; a link with nothing pending or the wrong state is
  refused; the right one takes the window to the finish page; **the web view's User-Agent is its own
  with `DynodocApp/0.1.0 (android)` appended**; the fragment is NOT set until the page says it is ready
  (the cloud's 404 page stands in for it, and the probe sets its ready marker), then it is, so Android's
  web view does not report the same-document change to the navigation rule; `SHA-256(verifier)` is the
  challenge; a replay does nothing; the verifier is not in the device log. The `ipc`, `links`,
  `deeplink` and `recreate` phases (42 checks) also passed on the new build. **Not run on Android:** the
  whole flow against a real server (the cloud has neither page yet), a server redirect or a history step
  aimed at the finish page, a physical phone.
* **Mutations, each of which fails at least one test** (first round: API verifier check, single use,
  state check, expiry, code stored in clear, table missing from the runtime allowlist, failed redeem
  not burning, outstanding limit, service key, sign-out not killing a code, per-prefix limit; shell state
  check, expiry, attempt not used up, server binding, challenge not the hash, parameter order, secrets
  in the query, launcher check, wrong-state link cancelling; web Origin check, return address, unchecked
  code in the link, fragment left in the address). Second round, redone after the review found four
  test gaps (three of them survivors): API plain-method downgrade accepted (the review's survivor, now caught by
  `a_downgrade_to_the_plain_method_is_refused`), unknown fields accepted on redeem and on issue, the
  issue route accepting a device token, the kind of app not compared, a zero-padded comparison; shell
  navigation rule allowing the finish page with anything in the address (and with only the query and
  fragment check removed), the script's path check, origin check and readiness wait removed, the outbox
  ignoring its server, the script's charset broadened (the review's survivor), the sign-in request
  spending the click budget (survived until the test filled both budgets first), expiry by one clock only,
  a server change not clearing, a different pairing code, a secret not overwritten, no User-Agent token;
  web redeem accepting a request without the token, the form accepting any `Sec-Fetch-Mode` or no
  `Sec-Fetch-User`, the finish page open to any browser, acting on a fragment present at load, and not
  needing the bridge object (the last four fail the browser tests).

## Not done, on purpose

* Android App Links and iOS universal links (they need the release signing key and the final
  identifiers). **They are the real fix for a program that claims the link scheme**; until then the
  pairing code is a human check and nothing more.
* The iPhone app, and a physical phone.
* The browser sign-in on macOS and Linux (no User-Agent token is added there, and nothing there has
  been run; the button is hidden and the command refuses). Adding it is a few lines per web view plus a
  run on that system.
* Keeping the verifier across the app being ended.
* The launcher deep link and shortcut.
* A rehearsal of migration 0046 against a production-like database.

(Removed from this list: "Telling the shell's window from a browser at the finish page". It is done;
see refinement 11.)
