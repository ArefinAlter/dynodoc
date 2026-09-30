<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ArrowUpRight, FileText } from '@lucide/svelte';
  import SiteHeader from './SiteHeader.svelte';
  import SiteFooter from './SiteFooter.svelte';
  import { page } from '$app/stores';
  import { legalPageStructuredData } from '$lib/seo';
  let {
    title,
    description,
    sections,
    children
  }: {
    title: string;
    description: string;
    sections: { id: string; title: string }[];
    children: Snippet;
  } = $props();
</script>

<svelte:head>
  <title>{title} · Dynodoc</title>
  <meta name="description" content={description} />
  <meta property="og:title" content={`${title} · Dynodoc`} />
  <meta property="og:description" content={description} />
  <meta name="twitter:title" content={`${title} · Dynodoc`} />
  <meta name="twitter:description" content={description} />
  <!-- Static, first-party page metadata; no visitor or document value is used. -->
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html legalPageStructuredData($page.url.pathname)}
</svelte:head>
<div class="legal-page">
  <SiteHeader />
  <main id="main">
    <div class="legal-heading">
      <p class="eyebrow">DYNODOC · RESEARCH PILOT</p>
      <h1>{title}</h1>
      <p class="intro">{description}</p>
      <p class="date">Draft prepared 19 September 2026 · Not yet effective</p>
    </div>
    <div class="draft-notice">
      <FileText size={21} />
      <div>
        <strong>A transparent draft, pending operator details</strong>
        <p>
          The legal operator, public contact details, hosting jurisdiction and governing law have
          not yet been confirmed. This page describes the current pilot and proposed terms of
          operation. It must be completed and reviewed before it is adopted for participant data or
          a public launch.
        </p>
      </div>
    </div>
    <div class="legal-grid">
      <aside>
        <nav aria-label="On this page">
          <span>ON THIS PAGE</span>{#each sections as section}<a href={`#${section.id}`}
              >{section.title}</a
            >{/each}
        </nav>
        <div class="related">
          <a href="/privacy">Privacy policy <ArrowUpRight size={13} /></a><a href="/terms"
            >Terms &amp; conditions <ArrowUpRight size={13} /></a
          >
        </div>
      </aside>
      <article>{@render children()}</article>
    </div>
  </main>
  <SiteFooter />
</div>

<style>
  .legal-page {
    background: var(--color-parchment);
    font-family: var(--font-ui);
    color: var(--color-ink);
  }
  main {
    max-width: 1190px;
    padding: 55px 48px 90px;
    margin: auto;
  }
  .eyebrow {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 1.5px;
    color: var(--color-ink-soft);
    margin: 0 0 22px;
  }
  h1 {
    font-size: 52px;
    line-height: 1.12;
    font-weight: 550;
    letter-spacing: -2px;
    margin: 0 0 23px;
    color: var(--color-ink-strong);
  }
  .intro {
    font-size: 17px;
    line-height: 1.8;
    max-width: 740px;
    color: var(--color-ink-soft);
    margin: 0 0 20px;
  }
  .date {
    font-size: 13px;
    color: var(--color-ink-soft);
    margin-bottom: 36px;
  }
  .draft-notice {
    display: flex;
    gap: 17px;
    padding: 23px 27px;
    background: var(--color-surface-muted);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    margin-bottom: 48px;
    color: var(--color-warn-ink);
  }
  .draft-notice > :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }
  .draft-notice strong {
    font-size: 13px;
    font-weight: 600;
  }
  .draft-notice p {
    font-size: 13px;
    line-height: 1.85;
    margin: 7px 0 0;
    max-width: 880px;
  }
  .legal-grid {
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 70px;
    align-items: start;
  }
  aside {
    position: sticky;
    top: 30px;
  }
  nav {
    display: grid;
    gap: 12px;
  }
  nav > span {
    font-size: 13px;
    letter-spacing: 1.1px;
    font-weight: 650;
    color: var(--color-ink-soft);
    margin-bottom: 7px;
  }
  nav > a {
    font-size: 13px;
    color: var(--color-ink-soft);
    text-decoration: none;
    line-height: 1.6;
  }
  nav > a:hover {
    color: var(--color-accent);
    text-decoration: underline;
    text-underline-offset: 4px;
  }
  .related {
    display: grid;
    gap: 12px;
    border-top: 1px solid var(--color-border);
    padding-top: 23px;
    margin-top: 26px;
  }
  .related > a {
    font-size: 13px;
    color: var(--color-accent);
    display: flex;
    gap: 10px;
    align-items: center;
    text-decoration: none;
  }
  article {
    min-width: 0;
  }
  article :global(section) {
    scroll-margin-top: 25px;
    padding-bottom: 29px;
    margin-bottom: 27px;
    border-bottom: 1px solid var(--color-border);
  }
  article :global(section:last-child) {
    margin-bottom: 0;
  }
  article :global(h2) {
    font-size: 24px;
    line-height: 1.35;
    letter-spacing: -0.6px;
    font-weight: 550;
    color: var(--color-ink-strong);
    margin: 0 0 20px;
  }
  article :global(h3) {
    font-size: 15px;
    font-weight: 600;
    margin: 23px 0 10px;
    color: var(--color-ink-soft);
  }
  article :global(p),
  article :global(li) {
    font-size: 13px;
    line-height: 1.95;
    color: var(--color-ink-soft);
  }
  article :global(p) {
    margin: 12px 0;
  }
  article :global(ul) {
    padding-left: 20px;
    margin: 16px 0;
  }
  article :global(li) {
    padding-left: 3px;
    margin: 8px 0;
  }
  article :global(strong) {
    color: var(--color-ink);
    font-weight: 600;
  }
  article :global(a) {
    color: var(--color-accent);
    text-underline-offset: 3px;
  }
  article :global(.table-scroll) {
    overflow: auto;
    margin: 20px 0;
    border: 1px solid var(--color-border);
    border-radius: 4px;
  }
  article :global(table) {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
    min-width: 470px;
  }
  article :global(th) {
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
    text-align: left;
    padding: 14px;
    font-size: 13px;
    font-weight: 600;
    border-bottom: 1px solid var(--color-border);
  }
  article :global(td) {
    padding: 14px;
    border-bottom: 1px solid var(--color-border-soft);
    vertical-align: top;
    color: var(--color-ink-soft);
    line-height: 1.9;
  }
  article :global(tr:last-child td) {
    border-bottom: 0;
  }
  article :global(td:first-child) {
    color: var(--color-ink-soft);
    min-width: 130px;
  }
  article :global(.callout) {
    background: var(--color-accent-bg);
    border-left: 3px solid color-mix(in srgb, var(--color-accent) 60%, transparent);
    padding: 16px 20px;
    margin: 22px 0;
  }
  article :global(.callout p) {
    margin: 0;
  }
  article :global(.contact-details) {
    display: grid;
    grid-template-columns: 150px 1fr;
    gap: 14px;
    margin: 20px 0;
    padding: 20px;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    font-size: 13px;
  }
  article :global(dt) {
    color: var(--color-ink-soft);
    font-weight: 550;
  }
  article :global(dd) {
    margin: 0;
    color: var(--color-ink-soft);
  }
  @media (max-width: 850px) {
    main {
      padding: 40px 28px 65px;
    }
    .legal-grid {
      gap: 35px;
      grid-template-columns: 175px 1fr;
    }
    h1 {
      font-size: 43px;
    }
    .intro {
      font-size: 15px;
    }
    .draft-notice {
      padding: 20px;
    }
  }
  @media (max-width: 600px) {
    main {
      padding: 30px 22px 50px;
    }
    h1 {
      font-size: 38px;
      letter-spacing: -1.3px;
    }
    .intro {
      font-size: 14px;
      line-height: 1.8;
    }
    .date {
      font-size: 13px;
      line-height: 1.8;
    }
    .draft-notice {
      padding: 18px;
      gap: 12px;
      margin-bottom: 30px;
    }
    .draft-notice strong {
      font-size: 13px;
    }
    .draft-notice p {
      font-size: 13px;
    }
    .legal-grid {
      grid-template-columns: 1fr;
      gap: 35px;
    }
    aside {
      position: static;
      padding-bottom: 28px;
      border-bottom: 1px solid var(--color-border);
    }
    nav {
      grid-template-columns: 1fr 1fr;
      gap: 12px 18px;
    }
    nav > span {
      grid-column: 1/-1;
    }
    nav > a {
      font-size: 13px;
    }
    .related {
      display: flex;
      gap: 22px;
      padding-top: 17px;
      margin-top: 20px;
    }
    .related > a {
      font-size: 13px;
    }
    article :global(h2) {
      font-size: 22px;
    }
    article :global(p),
    article :global(li) {
      font-size: 13px;
    }
    article :global(.contact-details) {
      grid-template-columns: 1fr;
      gap: 7px;
      padding: 16px;
    }
    article :global(dd) {
      margin-bottom: 9px;
    }
  }
  @media print {
    .legal-page :global(header),
    .legal-page :global(footer),
    aside {
      display: none;
    }
    main {
      padding: 0;
      max-width: none;
    }
    .legal-grid {
      display: block;
    }
    .draft-notice {
      break-inside: avoid;
    }
    article :global(section) {
      break-inside: avoid;
    }
    article :global(.table-scroll) {
      overflow: visible;
    }
    article :global(table) {
      min-width: 0;
    }
    h1 {
      font-size: 32px;
    }
  }
</style>
