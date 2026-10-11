# Browser policy and workload admission

11 October 2026. Verification and deployment evidence belong in the status/handoff.

## Browser contract

SvelteKit generates a per-response script nonce (or prerendered hash). Scripts are
restricted to the site; inline script and eval are not allowed. Document formatting
and Svelte transitions require inline styles. Images can use HTTPS/data/blob;
fonts and workers are local/data/blob as appropriate. Print frames are same-origin.
The prepaint theme initializer is an external local script.

Only `/connectors/word`, `/connectors/excel` and `/connectors/powerpoint` allow the
Microsoft Office CDN scripts and Office/SharePoint frame ancestors. Crossing that
boundary forces a document navigation, because a SPA cannot replace the active CSP.
Other pages use `frame-ancestors 'self'` and SAMEORIGIN. Operator-configured
`CONNECTOR_ORIGINS` adds comma-separated HTTPS origins to pane connections only;
credentials, non-HTTPS URLs and malformed entries are discarded. The default is the
current installation plus the hosted workspace. A custom remote must also allow CORS.

The existing Office SDK endpoint remains in use. Both the existing and newly
[documented Microsoft CDN](https://learn.microsoft.com/en-us/office/dev/add-ins/develop/referencing-the-javascript-api-for-office-library-from-its-cdn)
are allowed on panes. This is not installed-Office acceptance or a marketplace
submission. Google Apps Script sidebars execute under Google's host policy; their
synthetic bridge tests therefore use a separate fixture origin.

The application adds HTTPS-only HSTS (one year, without subdomain/preload claims),
nosniff and referrer protections, including early redirects. Nginx preserves the
application's nonce policy and emits exactly one CSP/HSTS/XFO policy, covering
static files and proxy errors as well. HTTP is redirected to HTTPS. Framework
errors retain baseline CSP; proxy headers cover failures outside the application hook.
Existing large-header buffers remain necessary for signed-in pages.

## API contract

Admission is process-local: at most 64 ordinary response bodies and 256 stream
responses are retained concurrently. A slow reader holds its slot until the body
finishes or is dropped. SSE has a separate pool so idle subscriptions do not consume
ordinary edit slots; the existing per-document 100-subscriber cap still applies.
Health probes bypass admission. Rejected requests return 429 with Retry-After.
A 16 MiB outer body cap coexists with smaller endpoint/file limits (pilot uploads
remain 3 MiB). GET/HEAD response setup has a cooperative 30-second deadline; write
handlers are not cancelled by an HTTP deadline after a possibly committed write.

Search, audit/snapshot/proof and checkpoint handlers share an eight-operation
expensive-read pool. After authentication and membership checks, each identity gets
120 admissions per fixed 60-second window and at most two active expensive reads.
Document reads also share 600 admissions/four active reads per governing project
(or the document for unprojected files). Search is account-scoped, not attributed to
an arbitrary project. Rotating a token does not rotate the identity budget. Registry
entries are capped at 16,384; old idle entries are reclaimed and active guards remain
valid. Exhaustion denies admission rather than growing memory without limit.

The supplied API binary uses eight database connections, a five-second acquisition
limit, 15-second PostgreSQL statement timeout, five-second lock timeout and a
30-second idle-transaction timeout. Search additionally sets a transaction-local
two-second statement timeout. PostgreSQL cancels the expensive query itself;
LIMIT 50 bounds returned search rows, not scanned nodes. Direct database timeout
errors return a generic 504; pool exhaustion returns 503, without SQL details.
Operator migrations/backfill CLI commands do not inherit API deadlines. Library
embedders must configure their own connection deadlines and runtime privilege check.

## Limits and next work

These bounds reduce amplification for the single-instance pilot. They do not prove
bounded per-document/event memory, preempt synchronous CPU work, limit every writer
by account/project, provide fleet-wide fairness, or make search indexed. Fixed-window
limits allow a boundary burst. Reverse-proxy connection/body timeouts and container
limits are additional deployment layers, not substitutes for chunked algorithms.
Shared chunk storage, byte-bounded history reads, cancellation-aware CPU work,
indexed authorized search and distributed tenant budgets remain staged work. No
migration, historical hash or semantic content operation changes in this increment.
