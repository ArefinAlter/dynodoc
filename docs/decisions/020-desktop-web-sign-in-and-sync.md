# Decision 020: the standalone desktop signs in to a server with the system browser, and syncs projects

Date: 26 October 2026. Owner: Verne. Status: built for 0.0.1b (pre-alpha); acceptance in
[ACCEPTANCE-0.0.1b.md](../ACCEPTANCE-0.0.1b.md). Extends decisions 017 (signing the native apps in)
and 019 (the standalone desktop with a local workspace).

## The question

With decision 019 the desktop app has a workspace of its own and needs no server. A server is an
addition: the hosted Dynodoc, or one an organisation runs (the self-hosted edition). The person must
be able to (1) sign in to a server from the app, (2) see that server's projects, (3) link a project on
this computer with one there, and (4) send and receive changes between the two, with the person's
choice wherever the two disagree.

## Signing in: the existing page, one new value, and a loopback address

Decision 017 already has everything but the last step. The app makes a PKCE pair and a state, opens
the server's own `/app-signin` page in the system browser, the person signs in there (emailed link
or Google, whatever that server offers) and presses Continue on a real form, and the server makes a
one-time code bound to the challenge. 017 hands that code to the app in a `dynodoc://signin` link,
which the app turns into a session in its own web view.

The standalone app needs the session in the app, not in a web view, and a custom-scheme link is a
poor way to reach it: it needs the operating system to have registered the scheme (a portable folder
or a development build has not), and it is the one step that differs on every system. RFC 8252
section 7.3 names the other way for native apps: a redirect to the loopback address, with the app
listening on a port it chooses.

The change to the server is additive and small. `/app-signin` accepts one more value,
`redirect_port` (an unprivileged port as plain digits, desktop requests only). When it is present,
the page that follows Continue hands the code to `http://127.0.0.1:<port>/signin?code=..&state=..`
instead of `dynodoc://`. Nothing else changes: the same form, the same checks of the browser's
`Sec-Fetch-*` headers, the same code (64 hex digits, 60 seconds, once, three unused at most), the
same API route, the same `/auth/app-signin/redeem`. A request without the value behaves exactly as
before; the API is called with exactly the three values it always got.

In the app (`src-tauri/src/accounts`):

- The listener binds 127.0.0.1 only, on a port the system picks, for one attempt, for at most ten
  minutes. It answers one fixed path and method, with exactly the two values in the shapes the server
  makes, and checks the `Host` header names it (DNS rebinding) and that the browser says it is a
  top-level navigation (not an image, script or `fetch`). A request with a state that is not the
  attempt's changes nothing and cannot cancel the person's own. The answer to a good request is a
  redirect to a fixed page that says nothing about the code, so the code is not left in the browser's
  history.
- The app then redeems at the server itself: `POST /auth/app-signin/redeem` with the code, state and
  verifier, with the app's `User-Agent` token and the site's own `Origin`, exactly what the redeem
  route requires. The verifier is in memory from the start of the attempt to that request, in a type
  with no `Debug`, `Clone` or `Display`, wiped when dropped.
- The server answers with its session cookie. The app keeps the cookie's value in the operating
  system's credential store (Windows Credential Manager, the macOS Keychain, the Secret Service), one
  entry per server, and nowhere else: not in the list of servers (`accounts.json`, which holds
  addresses and the name and email the server gave), not in a log, not in the page. The page is told
  only whether there is a session.
- The server renews its session on every request (it rotates the refresh token). The app stores the
  renewed cookie from each answer; a cleared cookie signs the person out here. Signing out calls the
  server's own logout (which revokes the login there) and then removes the entry whether or not the
  server answered.
- The page the app opens is always `<server>/app-signin` built from an address the person added
  (validated like the 0.0.1a launcher's: https, or http only to this computer). The page cannot name
  another address, a header or a port; every request the page makes to a server goes through the same
  fixed route tables as before (`remote.rs`).

What this does not do: it does not use the server's web view (0.0.1a mode, still there under "Web
workspace in this window"), it does not need an account to use the app, and it does not make a server
required. The app does not say it works with Dynodoc servers older than this one: a server without the
change shows its "not valid" page for a request carrying `redirect_port`, and the app tells the
person the server needs updating.

## Syncing: content, node by node

Both sides are Dynodoc engines and a file is nodes with fields on both, with node ids the engine
never reuses. A file pushed from here is created there from the same nodes (same ids), and from then
on the two copies are comparable node by node. A sync (`client/src/data/sync.ts`, tested against an
in-memory engine in `client/test/sync.test.mjs`):

1. lists the files of the local project and of the linked server project;
2. creates on one side every file the other has and this link has not seen (matching is by what was
   synced from what, never by title: two files with the same title and different origins are two
   files);
3. for a file both have, remembers the revision of each side when they last agreed, reads the content
   of each side at that revision and now (`documents/{id}/state?through_seq=`), and works out what
   each side changed per node and per field: new nodes, deletions, moves, field values;
4. takes both sides' changes when they do not touch the same field of the same node (equal changes
   count once), and writes to each side only what it lacks, as an ordinary batch of operations with
   the revision it read (a write that finds the file changed meanwhile is refused, and the next sync
   starts over);
5. holds a file back when the same field was changed differently on both sides, or a node was
   deleted on one side and changed on the other: nothing is written for that file until a person
   chooses, for each, "this computer" or "the server".

The history of each side stays its own hash chain and verifies on its own; what crosses is content,
not revisions, comments, change requests or discussions, and not the original uploaded file. A server
whose rules refuse a direct write to the main line (403) is not worked around: the changes can be sent
as a change request on that server (`sendForReview`), and the file waits until the request is merged.
Syncing is manual (a button), not in the background: a background process that decides on its own
which side wins is the thing the choice step exists to prevent.

The links are kept with the workspace on this computer (`state/links`), so a backup carries them; the
sessions are not (they are in the credential store, and a restored workspace asks to sign in again).

## What was run, what was not

See [ACCEPTANCE-0.0.1b.md](../ACCEPTANCE-0.0.1b.md), scenario 4: the real window, a real Edge as the
person's browser, a self-hosted server (the web tier built from this branch with the development
sign-in link, on loopback, with an engine whose access tokens last 30 seconds, so every request renews
the session). Not run: the hosted pre-alpha server before it carries the web-tier change; Google
sign-in; a server behind a corporate proxy; a second computer (the two-computer case is the same code
with two apps, but it was not run).
