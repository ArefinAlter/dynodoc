# Search, branding and administration

## Public identity and discovery

The tentative logo uses a blue document-shaped lowercase d, a light-blue earlier
page and a green saved-change mark. The SVG sources are in
`frontend/researcher/static/brand/`; the main mark is also `favicon.svg`.
PNG icons are provided at 48, 180, 192 and 512 pixels, plus an ICO and a 1200×630
social card. These are original project assets, separate from research vendors' logos.

The public site has canonical URLs, Open Graph metadata, a social card, static
WebSite/SoftwareApplication JSON-LD, a web manifest and `/sitemap.xml`.
Only the homepage, privacy and terms pages are listed. Private screens carry
`noindex` response headers and metadata. The workspace/admin hosts disallow crawling.
Authentication and membership checks remain the actual data access controls.

After the Vercel deployment completes:

1. In Google Search Console, add the **Domain** property `dynodoc.online`.
2. Copy Google's exact TXT verification value into the registrar's DNS and verify.
   Keep all existing website, workspace, admin and mail records.
3. Submit `https://dynodoc.online/sitemap.xml` in Sitemaps.
4. Inspect `https://dynodoc.online/` and request indexing. Review indexing and
   Core Web Vitals reports as Google gathers sufficient data.
5. If using a URL-prefix property instead, the optional Vercel public variable
   `PUBLIC_GOOGLE_SITE_VERIFICATION` accepts the HTML verification token; redeploy.

Search Console ownership and indexing requests require the owner's Google account.
A sitemap makes discovery easier; it does not guarantee indexing or rankings.
The privacy/terms pages still need the operator's final legal details and review.

References: [Google sitemaps](https://developers.google.com/search/docs/crawling-indexing/sitemaps/build-sitemap),
[Google favicons](https://developers.google.com/search/docs/appearance/favicon-in-search).

## Administrator access

The dedicated administrator site is `https://admin.dynodoc.online`. Its separate
web container uses loopback port 3089, a separate Nginx host and HTTPS certificate.
`ADMIN_EMAILS` on the API contains the explicit comma-separated allowlist; the first
administrator is configured privately as the owner's supplied Google email.
An empty allowlist denies all access. Ordinary Google registration does not grant
administrative access. Each request checks the authenticated identity's email.

Google's Web application client needs both exact redirect URIs:

```
https://app.dynodoc.online/auth/google/callback
https://admin.dynodoc.online/auth/google/callback
```

Use **External → In production** in Google Auth Platform for public registration.
The application requests only `openid email profile`, has no pilot-email allowlist,
and does not request Drive permissions. Google Workspace administrators can still
restrict apps for their own organizations. The administrator allowlist is unrelated
to public workspace sign-in.

The dashboard shows service status, disk space, database size/table statistics,
upload totals, account/document/version/event counts, latest local backup/checksum,
page loads and 75th-percentile web vitals. It offers no SQL console, Docker controls,
secret viewer or unrestricted access to researchers' records. Uploads live in
PostgreSQL; this deployment has no object-storage buckets.

`infra/demo/collect-status.py` runs every minute on the host and publishes a small
non-secret `ops/status.json`. The admin container mounts that directory read-only;
it never mounts the Docker socket. A status older than three minutes is visibly
stale. The collector also expires operational samples older than 30 days.

## Anonymous measurement

The browser sends a random per-page-load identifier, a broad page category and
numeric visit/LCP/INP/CLS/FCP/TTFB measurements to the VPS. It sends no IP address,
account/document ID, full URL, query, content, referrer or persistent identifier.
The endpoint validates categories, bounds payloads, rate-limits at Nginx, and stores
one value per page-load/metric. Do Not Track and Global Privacy Control disable it.
There are no analytics cookies. See the updated privacy notice.

Counts are page loads rather than unique people. Browser-blocked requests and
unsupported metrics are absent. Client-submitted analytics can be spoofed and are
not suitable for billing or security decisions. Core Web Vitals are measured with
[Google's web-vitals library](https://github.com/GoogleChrome/web-vitals).

Off-server backups, external alert delivery and production capacity planning remain
separate operational work; the dashboard must not imply that they are configured.
