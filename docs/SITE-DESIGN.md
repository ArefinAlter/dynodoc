# Marketing site design: Paper & Ink

The public site (`dynodoc.online`: `/`, `/features`, `/help`, legal pages and the pages
added next to them) uses one design system. This document is the contract for people
and agents building pages. The workspace, admin, login and shared-view routes keep the
application styling in `src/app.css` and are not affected.

## Where things live

| What | Path |
| --- | --- |
| Tokens, utilities, motion CSS | `frontend/researcher/src/lib/styles/site.css` |
| Layout (header, footer, `.site` wrapper) | `frontend/researcher/src/routes/(site)/+layout.svelte` |
| Primitives | `frontend/researcher/src/lib/components/site/*` (import from `$lib/components/site`) |
| Header, footer, page shells | `src/lib/components/marketing/{SiteHeader,SiteFooter,ProductPage,LegalPage}.svelte` |
| Navigation data | `src/lib/components/site/nav.ts` |
| Fonts | `static/fonts/geist-*.woff2` (OFL texts and `README.md` next to them) |
| Art | `src/lib/art/Dino.svelte` |

Every route inside `src/routes/(site)/` is wrapped by `<div class="site">` containing the
header, the page and the footer. URLs do not include `(site)`. A page must render exactly
one `<main id="main">`; the header (skip link target `#main`) and footer sit outside it.

## Tokens

`.site` re-declares the app's `--color-*` tokens, so older components (ThemeToggle,
legacy markup) inherit the palette. Always use tokens, never literal colours. Night mode is
`:root[data-theme='dark']` (set by `static/theme-init.js`), not `prefers-color-scheme`.

| Token | Day | Night | Use |
| --- | --- | --- | --- |
| `--color-parchment` | `#f6f3ea` | `#12120f` | page background (paper) |
| `--color-surface` | `#fbf9f3` | `#191914` | cards, panels |
| `--color-surface-raised` | `#ffffff` | `#1f1f19` | inputs, menus, diff cards |
| `--color-surface-muted` | `#ece7d8` | `#1e1e18` | muted bands, table heads |
| `--color-ink-strong` / `--color-ink` | `#0f0e0c` / `#1b1a17` | `#f7f4ea` / `#ece8dc` | headings, body |
| `--color-ink-soft` | `#45423a` | `#c9c4b4` | secondary text, ledes |
| `--color-muted` (`--color-subtle`) | `#67645a` | `#a29d8d` | captions, labels (4.7:1 or better on every surface) |
| `--color-rule` / `--color-border` | `#d9d4c4` | `#2d2c25` | hairlines, card borders |
| `--color-border-strong` | `#b9b39f` | `#45443a` | outline buttons, inputs |
| `--color-accent` | `#0f6b3f` | `#6fcf97` | links, marks, emphasis (text on paper: 5.9:1) |
| `--color-accent-strong` | `#0a5530` | `#8fdfae` | accent text on `--color-accent-bg` |
| `--color-accent-bg` / `-border` | `#e3efe6` / `#b7d3bf` | `#1a2a20` / `#2d4a38` | callouts, badges |
| `--color-accent-solid` | `#0f6b3f` | `#1f7a4a` | filled controls with `--color-on-accent` (white) |
| `--color-warn*`, `--color-danger*`, `--color-success*` | | | status sets (text, `-bg`, `-border`) |
| `--color-diff-add-*`, `--color-diff-del-*` | | | diff lines |
| `--color-focus` | `#0f6b3f` | `#6fcf97` | focus ring |
| `--site-highlighter` | `#d7f25c` | `#d7f25c` | highlighter mark, selection, ink-tone button hover |
| `--site-dot` | 17% ink | 15% paper | dotted grid colour |
| `--site-font-sans` / `--site-font-mono` | Geist | Geist Mono | font stacks (Public Sans fallback) |
| `--site-max`, `--site-gutter` | 1200px, 24px (16px under 640px) | | container |
| `--site-radius`, `--site-radius-sm` | 6px, 4px | | cards, controls |
| `--site-section-y` | `clamp(64px, 8vw, 120px)` | | vertical section padding |
| `--site-header-h` | 64px | | sticky header height (scroll padding, menus) |
| `--site-ease` | `cubic-bezier(.22,.61,.36,1)` | | the one easing curve |

Printing always uses the day palette.

## Type

Geist for text and headings, Geist Mono for labels, numbers and code (variable fonts,
`font-display: swap`, latin and latin-ext subsets, no third-party requests).

| Class | Size | Use |
| --- | --- | --- |
| `.h-display` | 40px to 84px | home hero only |
| `.h-1` | 34px to 60px | page titles, section titles |
| `.h-2` | 26px to 40px | sub-section titles |
| `.h-3` | 19px to 24px | card titles |
| `.lede` | 17px to 20px, `ink-soft`, 62ch | the paragraph under a title |
| `.small` | 14px, `muted` | captions |
| `.mono`, `.num`, `.num--lg` | | numbering (`01`, `STEP 01`), stats |
| `.eyebrow` | 12px mono caps | labels (use `<Eyebrow>`) |
| `.hl` | | highlighter mark behind a phrase in a heading |

Headings have weight and tracking but no size by default: choose a class. Keep one `h1`
per page and do not skip levels.

## Utilities (plain CSS, `:where(.site)` so component styles always win)

`.container` (max 1200px content, gutter 24px or 16px), `.container--narrow` (880px),
`.rule` (hairline `<hr>` with crosshair end marks, `.rule--bare` without), `.dotgrid`
(dotted field; `.dotgrid--fade`, `.dotgrid--fade-b`), `.card` (flat, 1px border;
`.card--raised`, `.card--flat`, `.card--interactive` lifts on hover), `.badge`
(`.badge--accent`), `.callout`, `.prose` (68ch long-form text, tables, code),
`.btn` (+ `--primary`, `--secondary`, `--ghost`, `--link`, `--sm`, `--lg`; `.btn__arrow`
nudges on hover), `.link-sweep` (underline sweeps in), `.tone-muted`, `.tone-ink`
(inverted panel: tokens switch to the night palette), `.spotlight`.

## Components (`$lib/components/site`)

All are Svelte 5 runes components with typed props.

| Component | Props |
| --- | --- |
| `Eyebrow` | `label`, `as?: 'p'\|'span'\|'div'` (p), `accent?`, `class?`, `children?`. Renders `[ LABEL ]`; brackets are hidden from screen readers. |
| `Btn` | `variant?: 'primary'\|'secondary'\|'ghost'\|'link'` (primary), `size?: 'sm'\|'md'\|'lg'`, `href?` (renders `<a>`, else `<button>`), `arrow?: boolean\|'right'\|'up-right'`, `external?` (new tab, adds screen-reader text, arrow becomes up-right), `type?`, `disabled?`, `class?`, `children`, other attributes (`onclick`, `aria-*`) pass through. |
| `Section` | `id?`, `eyebrow?`, `title?`, `lede?`, `tone?: 'plain'\|'muted'\|'ink'`, `rule?` (true; hairline with marks above), `layout?: 'stack'\|'split'`, `align?: 'start'\|'center'`, `level?: 2\|3`, `narrow?`, `class?`, snippets `heading` (rich title), `aside` (extra block beside the title in split), `children`. Labels itself with its title. |
| `Faq` | `items: {question, answer}[]` (same shape as `FAQ` in `$lib/seo/faq`), `exclusive?` (one open at a time), `openFirst?`, `class?`. Native `<details>`; height animates where supported. Render FAQPage JSON-LD from the same array. |
| `CopyCode` | `code`, `label?` (completes the button name, "Copy command"), `prompt?` (`$`, decoration only), `class?`. Button is disabled until hydrated; confirms with "Copied" and an `aria-live="polite"` message. |
| `Kbd` | `keys?: string\|string[]` (`['Ctrl','K']` renders `Ctrl + K`) or `children` for one key. |
| `Marquee` | `label` (list name), `speed?` (seconds per loop, 40), `direction?: 'left'\|'right'`, `pauseButton?` (true), `class?`, `children` (pass `<li>` items). Pauses on hover and focus, has a pause button, and is a static wrapped list under reduced motion. |
| `Stat` | `value: number`, `label`, `prefix?`, `suffix?`, `decimals?`, `duration?` (ms, 1400), `class?`. Server renders the final number; counts up once on reveal. |
| `DiffCard` | `path`, `lines?: DiffLine[]` (`{kind: 'add'\|'del'\|'ctx'\|'hunk', text}`) or `patch?` (unified-diff text), `meta?`, `typing?` (types changed lines when scrolled into view), `class?`. |
| `spotlight` (action) | `<div class="spotlight dotgrid" use:spotlight>`: pointer-following glow; dots light up with `.dotgrid`. Fine pointers only. |
| `scrollable` (action) | `<div use:scrollable>` on rendered content: wide `.table-wrap`, `.table-scroll` and `pre` blocks that overflow get `tabindex="0"`, `role="group"` and a name, so keyboard users can scroll them. `ProductPage` applies it. |
| `reveal` (action) | `<main use:reveal>` reveals every `[data-reveal]` inside; `<div data-reveal use:reveal>` reveals itself. `data-reveal-delay="120"` adds ms to one block; blocks arriving together stagger by 70ms (`use:reveal={{stagger}}`). `Section` already applies it. |

Example:

```svelte
<script lang="ts">
  import { Btn, DiffCard, Faq, Section, Stat } from '$lib/components/site';
  import { FAQ } from '$lib/seo/faq';
</script>

<main id="main">
  <Section id="how-it-works" eyebrow="HOW IT WORKS" title="Every change has an author." lede="…">
    <div class="grid" data-reveal>
      <DiffCard path="Board update.docx" meta="Maya Chen · 2 min ago" typing patch={'-Q3 budget draft\n+Q3 budget, approved'} />
      <Stat value={3} label="steps from edit to signed commit" />
    </div>
    <Btn href="/features" variant="secondary" arrow>See features</Btn>
  </Section>
  <Section tone="ink" eyebrow="FAQ" title="Questions"><Faq items={FAQ} /></Section>
</main>
```

Page shells: `ProductPage` (`title`, `description`, `eyebrow?`; body styles for `section`,
`.cards`, `.table-wrap`, `.callout`, `.badge`, `.cta`) and `LegalPage` (`title`,
`description`, `sections: {id, title}[]`; sticky table of contents with scrollspy, 68ch
column, numbered sections, print styles, draft notice, "LEGAL" eyebrow).

## Motion

One easing curve (`--site-ease`), 150 to 600ms. Allowed motion: reveal-on-scroll (12px rise
and fade, 400 to 600ms, staggered), link underline sweep, button arrow nudge, card
border and lift on hover, count-up numbers, typing diffs, pointer spotlight, copy
confirmation, accordion height, marquee. Rules:

- Everything is off under `prefers-reduced-motion: reduce`: CSS rules sit in
  `@media (prefers-reduced-motion: no-preference)`, scripts check the query and bail.
- Nothing is hidden without JavaScript. Reveal only marks blocks that are below the
  fold *after* hydration; counters and typing show their final state in server HTML.
- Auto-moving content that lasts longer than 5 seconds (marquees) needs a pause control.
- Animate `opacity`, `transform`, `clip-path`; never layout properties.

## Art

Pencil and halftone dinosaurs from `$lib/art/Dino.svelte`. The page imports the plate it
shows, so each route bundles only that art; `art` is required:

```svelte
<script lang="ts">
  import Dino from '$lib/art/Dino.svelte';
  import Rex from '$lib/art/Rex.svelte';
</script>

<Dino name="rex" art={Rex} class? animate? label? />
```

Plates: `Rex`, `Stego`, `Trike`, `Brachio`, `Ptero`, `Fossil`, `Egg`, `Tracks` (names
`rex stego trike brachio ptero fossil egg tracks`). Aspect ratios: about 5:3 for rex, stego,
ptero and fossil, 4:3 trike, 1:1 egg, 8:1 tracks. The art is decorative by default (no
`label`, hidden from assistive tech) and takes its colour from `color`/`currentColor`
(accent via `--art-accent`), so it follows day and night; the plate scales to the width of
its parent. Use one per section at most, large and cropped, never behind body text. The
footer already uses `tracks` as a divider.

## Accessibility rules

Pages are axe-tested in day mode with wcag2a/aa/21a/21aa/22aa and best-practice.

- Text contrast 4.5:1 (3:1 for 24px+). `muted` is the lightest text colour allowed and
  holds 4.7:1 or better on every surface; do not set text lighter than that.
- One `h1`, no skipped heading levels, unique landmarks, one `main`.
- Interactive targets at least 24px (controls here default to 40 to 44px). Visible focus
  ring (`--color-focus`, 2px, 3px offset) on everything; do not remove outlines.
- Link names must make sense out of context; icon-only controls need `aria-label`.
  External links use `<Btn external>` or add "(opens in a new tab)" screen-reader text.
- Colour is never the only signal (diff lines carry `+`/`-`).
- Decorative SVG and art: `aria-hidden="true"`. Scrollable regions that can overflow need
  `tabindex="0"` and a name.
- No horizontal overflow at 390px. Test with the viewport, not by eye.
- CSP: no inline scripts, no external fonts, CSS or CDNs; inline `<style>`, inline SVG and
  `data:` URIs are fine.

## Header and footer

`SiteHeader` is sticky with a hairline and blur. Dropdowns (Product, Resources) are
disclosure buttons (`aria-expanded`), open on click or desktop hover intent, close on Esc,
outside click or focus leaving, and ArrowDown moves into the panel. Below 961px a
full-screen menu with accordion groups appears (buttons "Open menu" / "Close menu"),
locks page scrolling, traps Tab and closes when a link is followed. Without JavaScript the
links render as a plain list. Edit links in `nav.ts` (`NAV_GROUPS`, `NAV_DIRECT`,
`FOOTER_COLUMNS`); `siteHref(path, relative)` keeps them relative on the marketing site and
absolute (`https://dynodoc.online`) when the page is served from an app host.

`SiteFooter` has Product / Resources / Trust & legal / Company columns
(`aria-label="Footer navigation"`), the tracks divider, a copyright line, the theme
toggle and an oversized outlined wordmark.

## How to add a page

1. Create `src/routes/(site)/<name>/+page.svelte`. Use `ProductPage` or `LegalPage`, or
   write `<svelte:head>` (title, description, og/twitter tags) plus your own
   `<main id="main">` built from `Section` and the primitives. Do not add the header or
   footer; the layout does.
2. Add the page to `INDEXABLE_PAGES` in `src/lib/seo/site.ts` (path, title, description,
   `lastmod`, `changefreq`, `priority`). That adds the canonical URL, sitemap entry,
   `llms.txt` line and IndexNow submission (see `docs/SEO-AND-INDEXNOW.md`). Pages not
   listed there are served `noindex`.
3. Link it from `nav.ts` if it belongs in the header or footer.
4. Add the path to the axe list in `e2e/launch.spec.ts` ("public pages pass automated
   WCAG A and AA checks") and, for legal pages, to the legal-pages test in
   `e2e/smoke.spec.ts`.
5. Use synthetic names in mockups (Maya Chen, Sam Okoye, Priya Nair; `Q3 budget.xlsx`,
   `Board update.docx`, `Launch plan.pptx`). Claim only what exists: no marketplace
   listings, customers, user counts, compliance certifications or uptime promises.
6. Page `<head>` should not set `theme-color`; the layout does.
7. Check `/<name>` at 1440 and 390px, day and night, and with reduced motion.
