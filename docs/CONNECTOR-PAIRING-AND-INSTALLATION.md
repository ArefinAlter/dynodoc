# Editor sign-in and test distribution

10 October 2026. Applies to Word/Docs, Excel/Sheets and PowerPoint/Slides.

The six development connectors now have browser approval sign-in in addition to
manual keys. Microsoft manifests include a Home ribbon Dynodoc group, an Open
provenance command and 16/32/80 PNG icons rendered from the existing vector logo.
Google Editor add-ons retain their native menu and HTML sidebar. The application
`/connectors` page offers three XML manifests and three reproducible Apps Script
ZIP packages, linked from Sync & connections. These are test-installation artifacts,
not marketplace submissions. Source publication does not deploy the server.

## Authentication contract

The editor creates a random 256-bit secret with browser Web Crypto and retains it.
The approval URL contains its SHA-256 digest, a host and a ten-minute link expiry.
It contains no usable bearer, account session, refresh token or service key. In a
separate browser, the user signs in with normal Dynodoc email authentication,
compares the displayed code, chooses a remote file and explicitly approves.
The login return path explicitly allows `/connectors/authorize`; arbitrary
destinations remain excluded. Connector pages use no-store and no-referrer headers.

The existing account-authenticated `POST /documents/{id}/connectors` accepts
optional `key_challenge` and `expires_at` together. It checks expiry, host/file kind,
current contributor access and the active-connection limit under document/project
locks. Only the hash is stored in `connector_grant`. An exact approval retry returns
the existing grant without extending its life; reuse for another file/account/host
or a revoked grant fails. Manual key creation without these fields remains compatible.
No new unauthenticated request table, plaintext secret storage or migration is needed.

The editor explicitly completes with its secret via bearer-only
`GET /connector/connection` (web gateway `/api/connectors/connection`). This checks
current account/session generation, grant expiry/revocation, file membership and
host binding, returning the file and grant IDs. The public challenge cannot call
this endpoint. Existing checkpoint/bundle paths then provide file-scoped read/propose;
the key cannot merge, grant access, or become an account session. The browser approval
is a Dynodoc pairing protocol, not Microsoft/Google OAuth or account-wide SSO.

Office keeps the key in pane memory; re-opened panes need another sign-in/manual key.
Google uses its existing private per-user, per-working-file credential storage via
Apps Script UrlFetch. The sidebar's unapproved secret exists only in memory; closing
it abandons that attempt. The approved grant lasts seven days, remains visible and
revocable in Sync & connections and never silently renews. Enabling capture remains
explicit. Existing queue, binding, scope and compare-before-apply checks still apply.

## Installation and acceptance

Application `extensions/INSTALL.md` is included in every Google package. Build with
`pnpm build:connectors`; `pnpm check:connectors` compares generated protocol, pairing,
calendar/common sources and reproducible installation archives to current sources.
Office installs use the platform's sideload workflow; Google packages use Editor
add-on test deployments. All Google hosts include Pairing.gs. Staging requires a
trusted HTTPS origin in every Office URL and the Google origin/UrlFetch allowlist.

Microsoft's validator checks the three XML manifests; runtime/native acceptance,
Google OAuth consent review, publisher configuration, store assets/legal pages and
marketplace publication are separate gates. Existing capture still covers supported
paragraph text, existing cells/formula/date values and slide shape text, with current
size limits. No new background capture, arbitrary formatting/structure, offline
commit graph or 100 MB native acceptance claim follows from adding installation UI.

References checked 10 October 2026:
[Microsoft XML commands/schema](https://learn.microsoft.com/en-us/office/dev/add-ins/develop/xml-manifest-overview),
[Google Editor add-on test deployment](https://developers.google.com/workspace/add-ons/how-tos/testing-editor-addons).
Exact validation results and remaining rollout steps are in
[the continuation handoff](ENGINE-EVOLUTION-HANDOFF.md).
