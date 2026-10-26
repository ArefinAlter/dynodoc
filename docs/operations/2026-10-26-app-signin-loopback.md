# Web tier change for the standalone desktop's sign-in (26 October 2026)

Status: **written, tested on a throw-away self-hosted server, not deployed to the hosted installation.**
The run that wrote it had no access to the host (the operator credential was not available to it), so
nothing on the VPS was touched.

## What changes

The web tier (`frontend/researcher`) only. No engine API change, no migration, no nginx change, no new
secret, no new environment variable.

| File | Change |
|---|---|
| `src/lib/app-signin.ts` | `/app-signin` accepts one more value, `redirect_port` (an unprivileged port as plain digits, desktop requests only). New `loopbackLink`. A request without it is parsed exactly as before |
| `src/lib/server/app-signin.ts` | After Continue, the link is `http://127.0.0.1:<port>/signin?code=..&state=..` when the request carried a port, the `dynodoc://signin` link as before otherwise. The API is called with exactly the three values it always got |
| `src/routes/app-signin/+page.server.ts`, `+page.svelte` | Passes the loopback flag; one sentence of text differs for it |
| `src/lib/app-signin.test.ts`, `src/lib/server/app-signin.test.ts` | Tests for the port (accepted and refused shapes, the rebuilt address, the link, the three-value API body) |

Decision: [020](../decisions/020-desktop-web-sign-in-and-sync.md).

## What it does for people

Until it is deployed, the desktop app signs in to a server that does not have this change through its
second button, "Server older than this app?", which ends with the `dynodoc://signin` link of decision
017 (it works when the app was installed with its installer, which registers the link). With the change
the first button works against the hosted workspace too.

## What was run

On the machine that built it: `vitest` for the two test files (34 tests), `prettier --check`,
`svelte-check` (0 errors), `vite build`; then the built web tier as a self-hosted server on loopback
(`DYNODOC_MODE=selfhosted`, `DEV_AUTH=true`) against a throw-away engine whose access tokens last 30
seconds, with the desktop app and Microsoft Edge: sign-in, link, sync, sign-out, sign-in again, a
restart after the access token had expired (ACCEPTANCE-0.0.1b.md, scenario 4).

## To deploy it (not done)

1. Back up as for any release (the database dump and the current web image or build directory).
2. Build the web tier from this branch as the previous release was built, and replace the web
   container. Nothing else restarts; the API and the database are not involved.
3. Check: `GET /app-signin?challenge=<43 chars>&state=<32 chars>&platform=desktop&redirect_port=49152`
   answers the confirmation page (or the sign-in page first, when not signed in), not the "not valid"
   page; the same request without `redirect_port` behaves as before.
4. Rollback: put the previous web container back. The change has no stored state.

Under the host rules of the earlier releases: only the Dynodoc stack, `flock /tmp/verne-nginx.lock`
for any nginx work (there is none here), the owner's real accounts are in the database (nothing here
touches it), and the credential is never written down.
