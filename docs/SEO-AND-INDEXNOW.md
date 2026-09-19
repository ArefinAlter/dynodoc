# SEO, GEO and IndexNow

How the public site is made discoverable by search engines and by answer
engines, and what has to happen at deploy time to activate IndexNow.

Prepared 19 September 2026. This describes the pilot's public homepage only.

## What is indexable

Only `dynodoc.online` (and `www`) is indexable. `app.dynodoc.online` and
`admin.dynodoc.online` run the same SvelteKit app but hold research content, so
they serve a fully disallowed `robots.txt`, return 404 for `sitemap.xml` and the
`llms.txt` files, and send `x-robots-tag: noindex, nofollow` on every path.

Host separation depends on `ORIGIN` being set correctly per deployment, because
the Node adapter derives the request URL from it. The VPS sets
`ORIGIN=https://app.dynodoc.online`; the admin container sets its own host.
Vercel derives the host from the request. Any host that is not `dynodoc.online`
or `www.dynodoc.online` is treated as private.

## Single source of truth

`frontend/researcher/src/lib/seo/site.ts` holds the canonical origin, the
indexable hosts, the indexable pages and the disallowed paths. `robots.txt`,
`sitemap.xml`, `llms.txt`, `llms-full.txt`, the head tags in `+layout.svelte`
and the noindex rules in `hooks.server.ts` all derive from it.

**To publish a new public page, add it to `INDEXABLE_PAGES`.** It then appears
in the sitemap and `llms.txt`, receives a canonical URL and the indexable robots
meta, and stops receiving the noindex header. Nothing else needs editing.

`lastmod` is maintained by hand rather than set at build time, so a redeploy
that changes nothing does not claim a fresh modification date. Update the date
when a page's content actually changes.

## GEO: what answer engines read

- `https://dynodoc.online/llms.txt` — a short Markdown description: what the
  product is, what it does, **what it does not do**, and the factual basics.
- `https://dynodoc.online/llms-full.txt` — the same, plus the complete FAQ.
- JSON-LD on every public page: `Organization`, `WebSite`,
  `SoftwareApplication` and `FAQPage` on the homepage; `WebPage` on the legal
  pages.

The limitations section is deliberate. An engine asked "does Dynodoc do live
co-editing?" should find the real answer rather than infer one from marketing
copy.

The homepage FAQ and its `FAQPage` markup both render from
`src/lib/seo/faq.ts`, so the marked-up answers cannot drift from the visible
ones. Edit the FAQ there, never in the page template.

`robots.txt` explicitly allows both retrieval crawlers (OAI-SearchBot,
ChatGPT-User, PerplexityBot, Claude-SearchBot, …) and training crawlers
(GPTBot, ClaudeBot, CCBot, Google-Extended, …), while disallowing the private
paths for all of them. To stop training-data collection while staying citable,
move the training agents out of `AI_AGENTS` in
`src/routes/robots.txt/+server.ts` and give them `Disallow: /`.

## IndexNow

IndexNow tells Bing, Yandex, Seznam and Naver that a page changed instead of
waiting to be recrawled. Ownership is proved by hosting a file named after the
key, containing the key, at the site root.

- Key: `ee30dd1d086546a79b0725a154acab9c`
- Key file: `frontend/researcher/static/ee30dd1d086546a79b0725a154acab9c.txt`
  (exactly 32 bytes: the key, no BOM, no trailing newline)
- Published at: `https://dynodoc.online/ee30dd1d086546a79b0725a154acab9c.txt`

The key is public by design; hosting it at a public URL is how the protocol
works. A unit test asserts the file still matches the constant in
`src/lib/seo/indexnow.ts`, so the two cannot drift.

### Activating it

IndexNow is **not active until the site is deployed** — until then the key file
returns 404 and submissions fail with 403.

1. Deploy the homepage to Vercel.
2. Confirm the key file is live and returns the key as `text/plain`:
   ```
   curl https://dynodoc.online/ee30dd1d086546a79b0725a154acab9c.txt
   ```
3. Submit the published URLs:
   ```
   cd frontend/researcher
   pnpm seo:indexnow --dry-run   # prints what would be sent
   pnpm seo:indexnow             # submits every URL in the live sitemap
   ```
4. Check receipt in [Bing Webmaster Tools](https://www.bing.com/webmasters).

### Submitting after later deploys

```
pnpm seo:indexnow                      # everything in the live sitemap
pnpm seo:indexnow /privacy /terms      # only these paths
```

The script reads the key from the key file rather than hardcoding it, and reads
the URL list from the live `sitemap.xml`, so it submits exactly what the site
advertises. Submit only URLs whose content actually changed; repeated
submission of unchanged pages is what triggers 429.

Response codes: 200 accepted · 400 malformed · 403 key not valid or not
reachable · 422 URL not on this host · 429 throttled.

### Rotating the key

Replace the file in `static/` (named after the new key, containing the new
key), update `INDEXNOW_KEY` in `src/lib/seo/indexnow.ts`, deploy, then submit.
The test will fail if the two disagree.

## Still outstanding

- `PUBLIC_GOOGLE_SITE_VERIFICATION` is read by the layout but must be set in
  Vercel for the Search Console meta tag to appear.
- Submit `https://dynodoc.online/sitemap.xml` in Google Search Console and Bing
  Webmaster Tools; IndexNow does not cover Google.
- The privacy and terms pages are now indexable, but their text is still a draft
  marked "not yet effective". Complete the operator details before relying on
  them publicly.
