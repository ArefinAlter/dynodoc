<script lang="ts">
  import type { Snippet } from 'svelte';
  import SiteHeader from './SiteHeader.svelte';
  import SiteFooter from './SiteFooter.svelte';
  let {
    title,
    description,
    eyebrow = 'VERSION CONTROL FOR DOCUMENTS',
    children
  }: { title: string; description: string; eyebrow?: string; children: Snippet } = $props();
</script>

<svelte:head
  ><title>{title} · Dynodoc</title><meta name="description" content={description} /><meta
    property="og:title"
    content={`${title} · Dynodoc`}
  /><meta property="og:description" content={description} /></svelte:head
>
<div class="product-page">
  <SiteHeader />
  <main id="main">
    <header>
      <p class="eyebrow">{eyebrow}</p>
      <h1>{title}</h1>
      <p class="intro">{description}</p>
    </header>
    <div class="page-body">{@render children()}</div>
  </main>
  <SiteFooter />
</div>

<style>
  .product-page {
    background: var(--color-parchment);
    color: var(--color-ink);
  }
  main {
    max-width: 1190px;
    padding: 65px 40px 85px;
    margin: auto;
  }
  header {
    max-width: 860px;
    margin-bottom: 55px;
  }
  .eyebrow {
    font-size: 14px;
    font-weight: 650;
    color: var(--color-accent);
    letter-spacing: 0.035em;
  }
  h1 {
    font-size: clamp(38px, 5.4vw, 65px);
    line-height: 1.09;
    letter-spacing: -2px;
    margin: 20px 0 28px;
  }
  .intro {
    font-size: 21px;
    line-height: 1.7;
    color: var(--color-ink-soft);
  }
  .page-body :global(h2) {
    font-size: 30px;
    line-height: 1.25;
    letter-spacing: -0.7px;
    margin: 0 0 18px;
  }
  .page-body :global(h3) {
    font-size: 20px;
    line-height: 1.4;
  }
  .page-body :global(p),
  .page-body :global(li) {
    font-size: 17px;
    line-height: 1.85;
    color: var(--color-ink-soft);
  }
  .page-body :global(section) {
    margin: 35px 0 55px;
  }
  .page-body :global(a) {
    text-underline-offset: 4px;
  }
  .page-body :global(.cards) {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 22px;
  }
  .page-body :global(.cards article) {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 27px;
  }
  .page-body :global(.cards article p) {
    margin-bottom: 0;
  }
  .page-body :global(.callout) {
    padding: 30px;
    border-radius: 12px;
    border: 1px solid var(--color-accent-border);
    background: var(--color-accent-bg);
  }
  .page-body :global(.table-wrap) {
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    background: var(--color-surface-raised);
  }
  .page-body :global(table) {
    width: 100%;
    border-collapse: collapse;
    text-align: left;
    font-size: 15px;
    line-height: 1.7;
  }
  .page-body :global(td),
  .page-body :global(th) {
    padding: 17px;
    border-bottom: 1px solid var(--color-border);
    vertical-align: top;
  }
  .page-body :global(th) {
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
  }
  .page-body :global(.badge) {
    display: inline-block;
    padding: 3px 9px;
    border: 1px solid var(--color-accent-border);
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
    border-radius: 4px;
    font-size: 12px;
    font-weight: 650;
  }
  .page-body :global(.cta) {
    display: inline-flex;
    padding: 12px 19px;
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
    text-decoration: none;
    border-radius: 6px;
    font-size: 16px;
  }
  .page-body :global(details) {
    padding: 21px 0;
    border-bottom: 1px solid var(--color-border);
  }
  .page-body :global(summary) {
    font-size: 19px;
    font-weight: 600;
    cursor: pointer;
    color: var(--color-ink);
  }
  .page-body :global(code) {
    font-size: 0.88em;
    word-break: break-word;
  }
  @media (max-width: 650px) {
    main {
      padding: 40px 23px 55px;
    }
    .intro {
      font-size: 18px;
    }
    .page-body :global(.cards) {
      grid-template-columns: 1fr;
    }
    .page-body :global(h2) {
      font-size: 27px;
    }
    h1 {
      letter-spacing: -1.2px;
    }
  }
</style>
