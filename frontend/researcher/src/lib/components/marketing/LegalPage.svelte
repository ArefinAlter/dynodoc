<script lang="ts">
  import type { Snippet } from 'svelte';
  import { onMount } from 'svelte';
  import { ArrowUpRight, FileText } from '@lucide/svelte';
  import Eyebrow from '$lib/components/site/Eyebrow.svelte';
  import { page } from '$app/stores';
  import { legalPageStructuredData } from '$lib/seo';
  // Legal documents: left table of contents with scrollspy, 68ch reading column, print
  // styles. The site header and footer come from the (site) layout.
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

  let active = $state('');

  // Every legal and trust document, in reading order.
  const RELATED = [
    { href: '/privacy', label: 'Privacy policy' },
    { href: '/terms', label: 'Terms & conditions' },
    { href: '/cookies', label: 'Cookies & local storage' },
    { href: '/security', label: 'Security' },
    { href: '/subprocessors', label: 'Subprocessors' },
    { href: '/acceptable-use', label: 'Acceptable use' },
    { href: '/dpa', label: 'Data processing terms' },
    { href: '/accessibility', label: 'Accessibility' },
    { href: '/licenses', label: 'Licenses' }
  ];

  // Scrollspy: highlight the section nearest the top of the reading column. Without
  // JavaScript the table of contents is simply a list of anchors.
  onMount(() => {
    if (!('IntersectionObserver' in window)) return;
    const ids = sections.map((section) => section.id);
    const visible = new Set<string>();
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) visible.add(entry.target.id);
          else visible.delete(entry.target.id);
        }
        const first = ids.find((id) => visible.has(id));
        if (first) active = first;
      },
      { rootMargin: '-96px 0px -60% 0px' }
    );
    for (const id of ids) {
      const el = document.getElementById(id);
      if (el) observer.observe(el);
    }
    const hash = () => {
      const id = decodeURIComponent(location.hash.slice(1));
      if (ids.includes(id)) active = id;
    };
    hash();
    window.addEventListener('hashchange', hash);
    return () => {
      observer.disconnect();
      window.removeEventListener('hashchange', hash);
    };
  });
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
<main id="main" class="legal-page">
  <header class="legal-heading dotgrid dotgrid--fade-b">
    <div class="container">
      <Eyebrow label="LEGAL" />
      <h1>{title}</h1>
      <p class="intro">{description}</p>
      <p class="date">Draft revised 12 October 2026 · Not yet effective</p>
    </div>
  </header>
  <div class="container legal-body">
    <div class="draft-notice" role="note">
      <FileText size={20} aria-hidden="true" />
      <div>
        <strong>A transparent draft, pending operator details</strong>
        <p>
          The legal operator, public contact details, hosting jurisdiction and governing law have
          not yet been confirmed. This page describes how the service currently works and the terms
          proposed for it. It must be completed and reviewed before it is adopted for customer data
          or a public launch.
        </p>
      </div>
    </div>
    <div class="legal-grid">
      <aside>
        <nav aria-label="On this page">
          <p class="toc-title">On this page</p>
          <ol>
            {#each sections as section (section.id)}
              <li>
                <a
                  href={`#${section.id}`}
                  class:active={active === section.id}
                  aria-current={active === section.id ? 'location' : undefined}>{section.title}</a
                >
              </li>
            {/each}
          </ol>
        </nav>
      </aside>
      <article class="prose">{@render children()}</article>
    </div>
    <nav class="related" aria-label="Related documents">
      <p class="toc-title">Related documents</p>
      <ul>
        {#each RELATED as item (item.href)}
          <li>
            <a href={item.href} aria-current={$page.url.pathname === item.href ? 'page' : undefined}
              >{item.label} <ArrowUpRight size={13} aria-hidden="true" /></a
            >
          </li>
        {/each}
      </ul>
    </nav>
  </div>
</main>

<style>
  .legal-heading {
    border-bottom: 1px solid var(--color-border);
  }
  .legal-heading .container {
    display: grid;
    gap: 18px;
    justify-items: start;
    padding-block: clamp(44px, 7vw, 88px) clamp(28px, 4vw, 48px);
  }
  h1 {
    font-size: clamp(2.25rem, 1.3rem + 3.8vw, 3.75rem);
    line-height: 1.05;
    letter-spacing: -0.04em;
    font-weight: 520;
  }
  .intro {
    max-width: 62ch;
    margin: 0;
    color: var(--color-ink-soft);
    font-size: clamp(1.0625rem, 1rem + 0.3vw, 1.1875rem);
    line-height: 1.6;
  }
  .date {
    margin: 0;
    color: var(--color-muted);
    font-family: var(--site-font-mono, ui-monospace, monospace);
    font-size: 0.8125rem;
    letter-spacing: 0.02em;
  }
  .legal-body {
    padding-block: clamp(28px, 4vw, 48px) 0;
  }
  .draft-notice {
    display: flex;
    gap: 14px;
    max-width: 68ch;
    margin: 0 0 clamp(32px, 5vw, 56px);
    padding: 18px 20px;
    border: 1px solid var(--color-warn-border);
    border-left: 3px solid var(--color-warn);
    border-radius: var(--site-radius-sm, 4px);
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
  }
  .draft-notice > :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .draft-notice strong {
    font-size: 0.9375rem;
    font-weight: 600;
  }
  .draft-notice p {
    margin: 6px 0 0;
    font-size: 0.9375rem;
    line-height: 1.6;
  }
  .legal-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: 32px;
    align-items: start;
  }
  aside {
    min-width: 0;
  }
  .toc-title {
    margin: 0 0 12px;
    color: var(--color-muted);
    font-family: var(--site-font-mono, ui-monospace, monospace);
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  ol {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
    counter-reset: toc;
  }
  ol li {
    counter-increment: toc;
  }
  ol a {
    display: flex;
    gap: 10px;
    padding: 4px 10px 4px 12px;
    border-left: 1px solid var(--color-border);
    color: var(--color-ink-soft);
    font-size: 0.875rem;
    line-height: 1.45;
    text-decoration: none;
    transition:
      color 0.15s,
      border-color 0.15s;
  }
  ol a::before {
    content: counter(toc, decimal-leading-zero);
    content: counter(toc, decimal-leading-zero) / '';
    flex: none;
    color: var(--color-muted);
    font-family: var(--site-font-mono, ui-monospace, monospace);
    font-size: 0.75rem;
    line-height: 1.9;
  }
  ol a:hover {
    color: var(--color-ink-strong);
    border-left-color: var(--color-ink-soft);
  }
  ol a.active {
    border-left: 2px solid var(--color-accent);
    padding-left: 11px;
    color: var(--color-ink-strong);
    font-weight: 540;
  }
  .related {
    margin-top: clamp(40px, 6vw, 72px);
    padding: 24px 0 clamp(40px, 6vw, 72px);
    border-top: 1px solid var(--color-border);
  }
  .related ul {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 2px 24px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .related a {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 28px;
    color: var(--color-accent-strong);
    font-size: 0.875rem;
    text-decoration: none;
  }
  .related a:hover {
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .related a[aria-current='page'] {
    color: var(--color-ink-strong);
    font-weight: 560;
  }

  article {
    min-width: 0;
    counter-reset: legal;
  }
  article :global(section) {
    counter-increment: legal;
    margin: 0;
    padding-bottom: clamp(24px, 3vw, 36px);
    margin-bottom: clamp(24px, 3vw, 36px);
    border-bottom: 1px solid var(--color-border);
    scroll-margin-top: calc(var(--site-header-h, 64px) + 20px);
  }
  article :global(section:last-child) {
    margin-bottom: 0;
    border-bottom: 0;
  }
  article :global(section > h2:first-child) {
    margin-top: 0;
  }
  article :global(section > h2:first-child::before) {
    content: counter(legal, decimal-leading-zero);
    content: counter(legal, decimal-leading-zero) / '';
    display: block;
    margin-bottom: 8px;
    color: var(--color-muted);
    font-family: var(--site-font-mono, ui-monospace, monospace);
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: 0.08em;
  }

  @media (min-width: 900px) {
    .legal-grid {
      grid-template-columns: 240px minmax(0, 68ch);
      gap: clamp(40px, 6vw, 88px);
    }
  }
  @media (min-width: 900px) and (min-height: 640px) {
    aside {
      position: sticky;
      top: calc(var(--site-header-h, 64px) + 24px);
    }
  }
  @media (max-width: 899px) {
    ol {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 2px 16px;
    }
    aside {
      padding-bottom: 24px;
      border-bottom: 1px solid var(--color-border);
    }
  }
  @media (max-width: 480px) {
    ol {
      grid-template-columns: 1fr;
    }
  }
  @media print {
    :global(.site-header),
    :global(.site-footer),
    :global(.skip),
    .related,
    aside {
      display: none !important;
    }
    .legal-heading {
      border: 0;
    }
    .legal-grid {
      display: block;
    }
    .draft-notice,
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
      font-size: 2rem;
    }
  }
</style>
