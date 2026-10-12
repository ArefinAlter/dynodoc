<script module lang="ts">
  export type FeatureChip = { kind: 'live' | 'source' | 'test'; label: string; detail?: string };
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Check } from '@lucide/svelte';

  /**
   * One numbered feature: number and status chips, title, short body, concrete points and
   * an honest boundary on one side; a product mockup on the other (`flip` swaps them).
   */
  let {
    id,
    n,
    title,
    chips,
    body,
    points,
    boundary,
    flip = false,
    caption,
    visual
  }: {
    id: string;
    n: string;
    title: string;
    chips: FeatureChip[];
    body: string;
    points: string[];
    boundary: string;
    flip?: boolean;
    caption: string;
    visual: Snippet;
  } = $props();
</script>

<article {id} class="row" class:flip data-reveal aria-labelledby="{id}-title">
  <div class="text">
    <div class="meta">
      <span class="num">{n}</span>
      <ul class="chips" aria-label="Status">
        {#each chips as chip (chip.label)}
          <li class="chip chip--{chip.kind}">
            <b>{chip.label}</b>{#if chip.detail}<span class="sep" aria-hidden="true">·</span><span
                class="sr-only"
                >:
              </span><span>{chip.detail}</span>{/if}
          </li>
        {/each}
      </ul>
    </div>
    <h3 id="{id}-title" class="h-2">{title}</h3>
    <p class="body">{body}</p>
    <ul class="points">
      {#each points as point (point)}
        <li><Check size={16} aria-hidden="true" /><span>{point}</span></li>
      {/each}
    </ul>
    <p class="boundary"><span class="eyebrow">Boundary</span>{boundary}</p>
  </div>
  <figure class="visual">
    {@render visual()}
    <figcaption class="small">{caption}</figcaption>
  </figure>
</article>

<style>
  .row {
    display: grid;
    gap: clamp(28px, 4vw, 56px);
    padding-block: clamp(36px, 5vw, 64px);
    border-top: 1px solid var(--color-border);
  }
  .row:first-child {
    border-top: 0;
    padding-top: 0;
  }
  .row:last-child {
    padding-bottom: 0;
  }
  .text {
    display: grid;
    gap: 16px;
    align-content: start;
    justify-items: start;
    min-width: 0;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 14px;
  }
  .num {
    font-size: 0.875rem;
    color: var(--color-ink);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .chip {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0 0.6em;
    padding: 2px 9px;
    border: 1px solid var(--color-border-strong);
    border-radius: 999px;
    font-family: var(--site-font-mono);
    font-size: 0.6875rem;
    letter-spacing: 0.04em;
    line-height: 1.7;
    text-transform: uppercase;
    color: var(--color-ink-soft);
  }
  .chip b {
    font-weight: 600;
    color: var(--color-ink);
  }
  .chip--live {
    border-color: var(--color-accent-border);
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
  }
  .chip--live b {
    color: var(--color-accent-strong);
  }
  .chip--test {
    border-color: var(--color-warn-border);
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
  }
  .chip--test b {
    color: var(--color-warn-ink);
  }
  .body {
    margin: 0;
    max-width: 58ch;
    color: var(--color-ink-soft);
    font-size: 1.0625rem;
    line-height: 1.6;
  }
  .points {
    display: grid;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
    max-width: 60ch;
  }
  .points li {
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr);
    gap: 12px;
    align-items: start;
    color: var(--color-ink);
    font-size: 0.9375rem;
    line-height: 1.55;
  }
  .points :global(svg) {
    margin-top: 3px;
    color: var(--color-accent);
  }
  .boundary {
    display: grid;
    gap: 4px;
    margin: 0;
    max-width: 60ch;
    padding: 12px 14px;
    border: 1px dashed var(--color-border-strong);
    border-radius: var(--site-radius-sm);
    color: var(--color-ink-soft);
    font-size: 0.875rem;
    line-height: 1.5;
  }
  .visual {
    display: grid;
    gap: 12px;
    align-content: center;
    margin: 0;
    min-width: 0;
  }
  .visual figcaption {
    max-width: 62ch;
  }

  @media (min-width: 960px) {
    .row {
      grid-template-columns: minmax(0, 5fr) minmax(0, 6fr);
      align-items: center;
    }
    .row.flip .text {
      order: 2;
    }
  }
</style>
