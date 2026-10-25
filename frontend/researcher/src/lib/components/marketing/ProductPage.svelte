<script lang="ts">
  import type { Snippet } from 'svelte';
  import { reveal } from '$lib/components/site/reveal';
  import { scrollable } from '$lib/components/site/scrollable';
  import { spotlight } from '$lib/components/site/spotlight';
  // Page shell for simple marketing pages. The site header and footer come from the
  // (site) layout, so this renders only <main id="main">.
  let { title, description, children }: { title: string; description: string; children: Snippet } =
    $props();
</script>

<svelte:head
  ><title>{title} · Dynodoc</title><meta name="description" content={description} /><meta
    property="og:title"
    content={`${title} · Dynodoc`}
  /><meta property="og:description" content={description} /></svelte:head
>
<main id="main" class="product-page">
  <header class="hero dotgrid dotgrid--fade spotlight" use:spotlight>
    <div class="container head">
      <h1>{title}</h1>
      <p class="lede">{description}</p>
    </div>
  </header>
  <div class="container page-body" use:reveal use:scrollable>{@render children()}</div>
</main>

<style>
  .hero {
    border-bottom: 1px solid var(--color-border);
  }
  .head {
    display: grid;
    gap: 22px;
    justify-items: start;
    padding-block: clamp(48px, 8vw, 104px) clamp(36px, 6vw, 72px);
  }
  h1 {
    max-width: 20ch;
    font-size: clamp(2.25rem, 1.2rem + 4.6vw, 4.5rem);
    line-height: 1.04;
    letter-spacing: -0.04em;
    font-weight: 520;
  }
  .page-body {
    padding-block: clamp(32px, 5vw, 64px) 0;
  }

  /* Body content written by the page: sections, cards, tables and callouts. */
  .page-body :global(section) {
    margin: 0;
    padding-block: clamp(32px, 4vw, 56px);
    border-top: 1px solid var(--color-rule);
  }
  .page-body :global(section:first-child) {
    border-top: 0;
    padding-top: 0;
  }
  .page-body :global(h2) {
    max-width: 28ch;
    margin: 0 0 18px;
    font-size: clamp(1.5rem, 1.1rem + 1.6vw, 2.25rem);
    line-height: 1.12;
    letter-spacing: -0.03em;
  }
  .page-body :global(h3) {
    margin: 0 0 8px;
    font-size: 1.1875rem;
    line-height: 1.3;
    letter-spacing: -0.015em;
  }
  .page-body :global(p),
  .page-body :global(li) {
    max-width: 72ch;
    font-size: 1.0625rem;
    line-height: 1.7;
    color: var(--color-ink-soft);
  }
  .page-body :global(p) {
    margin: 0 0 1em;
  }
  .page-body :global(a) {
    color: var(--color-accent-strong);
    text-decoration-thickness: 1px;
    text-underline-offset: 4px;
  }
  .page-body :global(a:hover) {
    color: var(--color-accent);
  }
  .page-body :global(.cards) {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
  }
  .page-body :global(.cards article) {
    padding: clamp(20px, 2.6vw, 32px);
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius, 6px);
    background: var(--color-surface);
    transition:
      border-color 0.2s,
      transform 0.25s;
  }
  .page-body :global(.cards article:hover) {
    border-color: var(--color-ink-soft);
    transform: translateY(-2px);
  }
  .page-body :global(.cards article h2) {
    margin-top: 14px;
    font-size: 1.375rem;
    letter-spacing: -0.02em;
  }
  .page-body :global(.cards article p) {
    margin-bottom: 0;
    font-size: 1rem;
  }
  .page-body :global(.badge) {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    border: 1px solid var(--color-accent-border);
    border-radius: 999px;
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
    font-family: var(--site-font-mono, ui-monospace, monospace);
    font-size: 0.6875rem;
    font-weight: 500;
    letter-spacing: 0.06em;
    line-height: 1.6;
    text-transform: uppercase;
  }
  .page-body :global(.callout) {
    margin-block: clamp(24px, 4vw, 48px);
    padding: clamp(20px, 3vw, 32px);
    border: 1px solid var(--color-accent-border);
    border-left: 3px solid var(--color-accent);
    border-radius: var(--site-radius, 6px);
    background: var(--color-accent-bg);
  }
  .page-body :global(.callout > :last-child) {
    margin-bottom: 0;
  }
  .page-body :global(.table-wrap) {
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius, 6px);
    background: var(--color-surface-raised);
  }
  .page-body :global(table) {
    width: 100%;
    min-width: 560px;
    border-collapse: collapse;
    text-align: left;
    font-size: 0.9375rem;
    line-height: 1.6;
  }
  .page-body :global(th),
  .page-body :global(td) {
    padding: 14px 16px;
    border-bottom: 1px solid var(--color-border);
    vertical-align: top;
    color: var(--color-ink-soft);
  }
  .page-body :global(th) {
    background: var(--color-surface-muted);
    color: var(--color-ink);
    font-family: var(--site-font-mono, ui-monospace, monospace);
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .page-body :global(tr:last-child td) {
    border-bottom: 0;
  }
  .page-body :global(td:first-child) {
    color: var(--color-ink);
    font-weight: 500;
  }
  .page-body :global(.cta) {
    display: inline-flex;
    align-items: center;
    min-height: 48px;
    margin-top: 16px;
    padding: 0 22px;
    border: 1px solid var(--color-ink-strong);
    border-radius: var(--site-radius-sm, 4px);
    background: var(--color-ink-strong);
    color: var(--color-parchment);
    font-size: 1rem;
    font-weight: 500;
    text-decoration: none;
    transition:
      background-color 0.18s,
      border-color 0.18s;
  }
  .page-body :global(.cta:hover) {
    border-color: var(--color-accent);
    background: var(--color-accent);
    color: var(--color-parchment);
  }
  .page-body :global(details) {
    border-bottom: 1px solid var(--color-border);
  }
  .page-body :global(summary) {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 16px;
    min-height: 44px;
    padding: 18px 0;
    color: var(--color-ink-strong);
    font-size: 1.125rem;
    font-weight: 520;
    cursor: pointer;
    list-style: none;
  }
  .page-body :global(summary::-webkit-details-marker) {
    display: none;
  }
  .page-body :global(summary::after) {
    content: '+';
    flex: none;
    color: var(--color-muted);
    font-family: var(--site-font-mono, ui-monospace, monospace);
    font-size: 1.25rem;
    line-height: 1;
  }
  .page-body :global(details[open] > summary::after) {
    content: '\2212';
  }
  .page-body :global(code) {
    padding: 0.1em 0.35em;
    border: 1px solid var(--color-border);
    border-radius: 3px;
    background: var(--color-surface-muted);
    font-size: 0.88em;
    overflow-wrap: anywhere;
  }
  @media (max-width: 720px) {
    .page-body :global(.cards) {
      grid-template-columns: 1fr;
    }
  }
</style>
