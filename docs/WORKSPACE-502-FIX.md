# Workspace 502 repair — 6 October 2026

## Cause and production repair

Opening the signed-in workspace failed at Nginx with
`upstream sent too big header while reading response header from upstream`.
The web/API containers were running; signed-out workspace visits still returned
their normal sign-in redirect.

A synthetic, encrypted web-session fixture reproduced the exact failure without
using a production account. Its access/refresh tokens are invalid engine tokens;
only the server-rendered page was requested, with no browser/API data fetches.
The web container returned **200 with 4,262 header bytes**, including a
**3,955-byte asset preload `Link` header**. The same request through public HTTPS
returned **502**. This VPS has 4,096-byte memory pages and no proxy buffer
overrides, so Nginx's default header buffer was too small.

Both current HTTPS templates, `infra/demo/nginx-app.conf` and `nginx-admin.conf`,
now set server-level allowances inherited by every proxied location:

```nginx
proxy_buffer_size 16k;
proxy_buffers 8 16k;
proxy_busy_buffers_size 32k;
```

This leaves room for preload headers and session/OAuth cookie rotation, including
the buffered `/auth/` location. Streaming workspace responses retain
`proxy_buffering off`. See the [Nginx directive reference](https://nginx.org/en/docs/http/ngx_http_proxy_module.html#proxy_buffer_size).
The admin template receives the same allowance; the reported failure was on the
workspace host.

Installed the two templates in `/etc/nginx/sites-available/dynodoc-app` and
`dynodoc-admin` and their corresponding `/opt/dynodoc-demo/infra/demo/` files.
`nginx -t` passed and Nginx was gracefully reloaded. **The same workspace request
then returned 200 through HTTPS**, preserving its preload links. All container
IDs/images and all other Nginx site hashes matched before/after. No account,
document, event, email, credential, database or API change was made.

The application containers still run `8c7461d`, schema 24, as recorded in the
[date-only release](SPREADSHEET-DATE-SYNC-RELEASE.md); this repair is a Nginx
configuration overlay and does not require rebuilding them or the engine.

## Verification and rollback

Production checks passed for app/admin health and login, Google sign-in start,
email/Google option flags with development authentication disabled, anonymous
workspace/Projects redirects and engine API rejection. Office panes retained
their scoped frame policy, while login/workspace responses retained `SAMEORIGIN`.
No new workspace header-overflow error was present after the reload at the final
check. Completing a real Google/email sign-in in the owner's browser remains an
acceptance check; this repair did not sign in as a production user.

Backups and comparison evidence are under
`/opt/dynodoc-demo/releases/workspace-header-fix-20261006T092350Z/`:
the before/after site configs, container inventory, unrelated-site hashes,
successful syntax check and reload time. To undo this overlay, restore the two
`nginx-*.before.conf` files to their corresponding site paths, restore the
`source-*.before.conf` files to the source templates, run `nginx -t`, then
`systemctl reload nginx`. The previous buffer limit will reproduce the failure;
no container or database rollback is needed.

## Regression coverage

`frontend/researcher/scripts/check-proxy-headers.mjs` runs only with
`APP_ENV=test`, a local isolated web server and its test `SESSION_SECRET`, on
Linux with Nginx installed. Run `pnpm check:proxy` from that frontend directory.
It uses the actual app/admin templates, changing only TLS/listening ports, local
upstreams and access-log output for the isolated fixture.

The check first proves that removing the buffer allowances reproduces 502 for a
large preload/cookie fixture on both auth proxies. With the real templates it
checks actual rendered workspace/Projects pages, preserves preload links, passes
the large cookie fixture, retains the anonymous sign-in redirect and verifies all
three Office pane frame policies. Fixtures cannot authenticate to the engine and
do not create or read any account/document. Processes and temporary files are
removed at completion.

Frontend CI now watches both Nginx templates and runs this regression before the
signed-in browser workflows. Earlier CI exercised the built application directly,
so its browser results did not cover Nginx's header-buffer limit.
