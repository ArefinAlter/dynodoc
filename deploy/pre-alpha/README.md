# Dynodoc pre-alpha service

A small service that runs **beside** the hosted Dynodoc workspace (https://app.dynodoc.online) for
the 0.0.1a pre-alpha release. It is not part of the product: the engine, its API and the web
workspace are unchanged. It does two things.

1. **Sample workspaces for visitors.** Anyone can try Dynodoc without asking for an account: a
   visitor gets their own copy of a sample organisation (a team handbook, a 2027 budget, a
   quarterly review deck, a field guide and a grant application, with their history, drafts,
   change requests, reviews, issues, labels, discussions, versions and a published copy), written
   through the real engine API by six invented colleagues at example.org. Each copy belongs to one
   visitor, is signed in with the product's own one-use sign-in link, and is retired 24 hours after
   it was started.
2. **What the desktop client shows and the server has no place for**, per account: the changes
   the editor add-ins captured and nobody committed yet, commits not yet pushed, agent threads and
   a few client choices; and per file, commit messages (the engine records changes, not messages).

Everything else the desktop client shows (projects, files, history, drafts, change requests,
reviews, merges, versions, discussions, published copies, people and roles, review rules, the AI
agent policy, issues, labels, stars, watching, notifications, signing keys) is read from and
written to the real server through the web workspace's own `/api/engine` proxy, with the
person's session, so the server's permission rules apply unchanged.

## Endpoints (served at `<workspace>/svc/`)

| Method and path | What |
|---|---|
| `GET /svc/healthz` | liveness |
| `GET /svc/visitor` | a page that explains the sample workspace and offers to start one |
| `POST /svc/visitor/start` | (form, same origin) claims a sample workspace and sends the browser to the workspace's own sign-in confirmation page (`/login?token=...`) |
| `POST /svc/v1/visitor` | (JSON, same origin; used by the desktop app) claims one and returns its one-use sign-in token, which the app confirms at `/auth/verify` itself |
| `GET /svc/v1/session` | the signed-in account, and whether it is a sample workspace (with its end time) |
| `GET`/`PUT /svc/v1/state/{working,local,threads,client}` | the client's state for this account (256 KiB each) |
| `GET /svc/v1/notes?document=...` | commit messages of revisions of files the person can open (checked with the engine) |
| `POST /svc/v1/notes` | record one (summary up to 200 characters, description up to 4,000) |

Who is asking comes from the workspace's session cookie, read (never written) with the web tier's
session secret, and the engine's `/auth/me`. Every write needs the workspace's exact `Origin`.
Limits: 6 sample workspaces per address per hour and 240 per hour in all (the address is kept only
as a truncated SHA-256 in a counter), 600 requests per account per minute, 300 KiB per request,
5,000 notes per file; nginx adds its own per-address limits.

## How it runs

`compose.yml`, compose project `dynodoc-prealpha`, image `dynodoc-prealpha-svc` (Node 24, no npm
dependencies: Node's own `http`, `crypto` and `sqlite`):

- `svc`: the HTTP service. It answers on a unix socket (`./run/svc.sock`), not a port, and sits
  only on the workspace stack's internal engine network (`dynodoc-demo_engine`, `internal: true`),
  so it has no route out of the host. Holds the web tier's session secret (to read cookies).
- `provisioner`: no listener. Keeps three sample workspaces ready, refreshes their sign-in tokens
  (they last 15 minutes), and retires sample workspaces 24 hours after they were started (and ready
  ones nobody took in a week, and failed builds): the visitor is removed from every project and
  space, the files go to the trash, and the service forgets the account's client state. Holds the
  web tier's service key (it signs the sample colleagues in with `/auth/exchange` and asks for the
  visitors' sign-in links with `/auth/magic-link`, which sends no mail). Backs off for five
  minutes after a failed build.
- `data` volume: the SQLite database (sandboxes, client state, notes, rate counters).

Both containers are read-only, drop every capability, have memory and CPU limits and rotate
their logs. Neither logs a token, a cookie or a body.

nginx (`infra/demo/nginx-app.conf`, the `app.dynodoc.online` site) proxies `/svc/` to the socket,
with its own `limit_req` zones, and keeps the site's security headers; the service sets its own
Content-Security-Policy and `X-Robots-Tag: noindex, nofollow`.

## Operate

```sh
cd /opt/dynodoc-prealpha
docker compose ps                      # svc and provisioner
docker compose logs --tail 50 provisioner
docker compose up -d --build           # after changing the service
docker compose exec svc node -e "..."  # no shell tools inside; use node
```

Secrets live in `secrets/` (copied from the workspace's `.env`, mode 0400, owned by the
container user). Nothing here changes the workspace stack; stopping this project only stops
sample workspaces and the desktop client's extras.

What the retirement does not do: the engine has no hard delete outside the administrator's
erasure, so retired sample files stay in the trash and the visitor accounts
(`visitor-*@visitors.dynodoc.invalid`) and the sample colleagues' accounts (`*@example.org`)
stay in the identity table. An administrator can erase them in `/admin`.

## Tests

```sh
node --test deploy/pre-alpha/test/svc.test.mjs
```

The sandbox build itself (`service/lib/sample.mjs`) was run against the live server; it builds a
sample workspace in about two seconds.
