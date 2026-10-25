<script lang="ts">
  import { page } from '$app/stores';
  import { env } from '$env/dynamic/public';
  import { ArrowUpRight } from '@lucide/svelte';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import Dino from '$lib/art/Dino.svelte';
  import Tracks from '$lib/art/Tracks.svelte';
  import { FOOTER_COLUMNS, isAppHost, siteHref, workspaceHref } from '$lib/components/site/nav';
  import { OPERATOR } from '$lib/site/operator';

  const relative = $derived(!isAppHost($page.url.hostname));
  const href = (path: string) => siteHref(path, relative);
  const workspace = $derived(workspaceHref(env.PUBLIC_WORKSPACE_URL));
  const year = new Date().getFullYear();
  const letters = 'dynodoc'.split('');
</script>

<footer class="site-footer">
  <div class="container top">
    <div class="about">
      <a class="brand" href={href('/')} aria-label="Dynodoc home"
        ><img src="/brand/mark.svg?v=2" width="28" height="28" alt="" /><span>dynodoc</span></a
      >
      <p>Provenance, review and synchronization for documents, spreadsheets and presentations.</p>
      <a class="open" href={workspace}
        >Open workspace<ArrowUpRight size={15} aria-hidden="true" /></a
      >
    </div>
    <nav class="columns" aria-label="Footer navigation">
      {#each FOOTER_COLUMNS as column (column.title)}
        <div class="column">
          <h2>{column.title}</h2>
          <ul>
            {#each column.links as link (link.label)}
              <li>
                <a
                  href={link.external ? link.href : href(link.href)}
                  target={link.external ? '_blank' : undefined}
                  rel={link.external ? 'noopener noreferrer' : undefined}
                  >{link.label}{#if link.external}<ArrowUpRight size={13} aria-hidden="true" /><span
                      class="sr-only"
                    >
                      (opens in a new tab)</span
                    >{/if}</a
                >
              </li>
            {/each}
          </ul>
        </div>
      {/each}
    </nav>
  </div>

  <div class="strip" aria-hidden="true">
    <div class="container strip-inner">
      <span class="line"></span>
      <div class="tracks"><Dino name="tracks" art={Tracks} /></div>
      <span class="line"></span>
    </div>
  </div>

  <div class="container bottom">
    <p class="legal">
      <span>&copy; {year} {OPERATOR.copyrightHolder}</span>
      <span class="sep" aria-hidden="true">/</span>
      <span>Dynodoc is a product of {OPERATOR.copyrightHolder}</span>
      <span class="sep" aria-hidden="true">/</span>
      <a href={href('/licenses')}>Open-source engine (MIT)</a>
    </p>
    <ThemeToggle variant="label" />
  </div>

  <div class="ghost" aria-hidden="true">
    {#each letters as letter, index (index)}<span style="--i:{index}">{letter}</span>{/each}
  </div>
</footer>

<style>
  .site-footer {
    position: relative;
    margin-top: var(--site-section-y, 96px);
    overflow: hidden;
    border-top: 1px solid var(--color-border);
    background: var(--color-parchment);
    color: var(--color-ink-soft);
    font-family: var(--site-font-sans, var(--font-ui));
  }
  .container {
    width: 100%;
    max-width: calc(var(--site-max, 1200px) + 2 * var(--site-gutter, 24px));
    margin-inline: auto;
    padding-inline: var(--site-gutter, 24px);
  }
  .top {
    display: grid;
    gap: 40px clamp(32px, 6vw, 96px);
    padding-block: clamp(40px, 6vw, 72px);
  }
  .about {
    display: grid;
    gap: 16px;
    align-content: start;
    justify-items: start;
    max-width: 34ch;
  }
  .brand {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    color: var(--color-ink-strong);
    font-size: 1.25rem;
    font-weight: 620;
    letter-spacing: -0.04em;
    text-decoration: none;
  }
  .about p {
    margin: 0;
    font-size: 0.9375rem;
    line-height: 1.55;
  }
  .open {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 32px;
    color: var(--color-ink-strong);
    font-size: 0.9375rem;
    font-weight: 540;
    text-decoration: underline;
    text-decoration-thickness: 1px;
    text-underline-offset: 4px;
  }
  .open:hover {
    color: var(--color-accent);
  }
  .columns {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 32px 24px;
  }
  .column h2 {
    margin: 0 0 12px;
    color: var(--color-muted);
    font-family: var(--site-font-mono, ui-monospace, monospace);
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: 0.08em;
    line-height: 1.4;
    text-transform: uppercase;
  }
  .column ul {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .column a {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-height: 28px;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
    text-decoration: none;
    background-image: linear-gradient(currentColor, currentColor);
    background-position: 0 calc(100% - 3px);
    background-repeat: no-repeat;
    background-size: 0 1px;
    transition:
      color 0.15s,
      background-size 0.28s var(--site-ease, ease);
  }
  .column a:hover,
  .column a:focus-visible {
    color: var(--color-ink-strong);
    background-size: 100% 1px;
  }

  .strip {
    padding-block: 8px;
  }
  .strip-inner {
    display: flex;
    align-items: center;
    gap: 20px;
  }
  .line {
    flex: 1;
    height: 0;
    border-top: 1px solid var(--color-rule);
  }
  .tracks {
    display: block;
    flex: none;
    width: clamp(120px, 18vw, 200px);
    color: var(--color-ink-soft);
    opacity: 0.85;
  }
  .tracks :global(svg) {
    display: block;
    width: 100%;
    height: auto;
  }

  .bottom {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px 24px;
    padding-block: 16px 8px;
    font-size: 0.875rem;
  }
  .legal {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    margin: 0;
    color: var(--color-muted);
  }
  .legal a {
    color: inherit;
    text-decoration: underline;
    text-decoration-thickness: 1px;
    text-underline-offset: 3px;
  }
  .legal a:hover {
    color: var(--color-ink-strong);
  }
  .sep {
    color: var(--color-border-strong);
  }
  @media (max-width: 639px) {
    .legal {
      flex-direction: column;
      gap: 2px;
    }
    .sep {
      display: none;
    }
  }

  /* Oversized outlined wordmark. Each letter lifts and fills as the pointer passes. */
  .ghost {
    display: flex;
    justify-content: center;
    margin-top: clamp(8px, 3vw, 32px);
    padding-inline: var(--site-gutter, 24px);
    font-size: clamp(56px, 20vw, 280px);
    font-weight: 620;
    letter-spacing: 0;
    line-height: 0.86;
    white-space: nowrap;
    -webkit-user-select: none;
    user-select: none;
  }
  .ghost span {
    color: transparent;
    -webkit-text-stroke: 1px var(--color-border-strong);
    transition:
      transform 0.35s var(--site-ease, ease),
      color 0.35s,
      -webkit-text-stroke-color 0.35s;
    transition-delay: calc(var(--i) * 12ms);
  }
  .ghost span:hover {
    color: var(--color-accent-bg);
    -webkit-text-stroke-color: var(--color-accent);
    transform: translateY(-0.04em);
    transition-delay: 0ms;
  }

  @media (min-width: 640px) {
    .columns {
      grid-template-columns: repeat(4, minmax(0, 1fr));
    }
  }
  @media (min-width: 960px) {
    .top {
      grid-template-columns: minmax(220px, 3fr) minmax(0, 8fr);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .ghost span,
    .column a {
      transition: none;
    }
  }
  @media print {
    .strip,
    .ghost {
      display: none;
    }
  }
</style>
