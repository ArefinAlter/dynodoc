<script lang="ts">
  import { Plus } from '@lucide/svelte';
  import type { FaqEntry } from '$lib/seo/faq';
  /**
   * Accordion built on native <details>, so it works without JavaScript and in find-in-
   * page. The panel animates its height where the browser supports it (otherwise it
   * snaps). `items` has the shape of `FAQ` in `$lib/seo/faq`. `exclusive` makes opening
   * one entry close the others. For FAQPage JSON-LD, render from the same array.
   */
  let {
    items,
    exclusive = false,
    openFirst = false,
    class: className = ''
  }: {
    items: FaqEntry[];
    exclusive?: boolean;
    openFirst?: boolean;
    class?: string;
  } = $props();
  const uid = $props.id();
</script>

<div class="faq {className}">
  {#each items as item, index (item.question)}
    <details name={exclusive ? `faq-${uid}` : undefined} open={openFirst && index === 0}>
      <summary
        ><span class="q">{item.question}</span><Plus
          class="icon"
          size={18}
          aria-hidden="true"
        /></summary
      >
      <div class="a"><p>{item.answer}</p></div>
    </details>
  {/each}
</div>

<style>
  .faq {
    border-top: 1px solid var(--color-border-strong);
    interpolate-size: allow-keywords;
  }
  details {
    border-bottom: 1px solid var(--color-border);
  }
  summary {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 20px;
    min-height: 44px;
    padding: 20px 0;
    color: var(--color-ink-strong);
    font-size: 1.0625rem;
    font-weight: 520;
    line-height: 1.4;
    cursor: pointer;
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  summary:hover .q {
    text-decoration: underline;
    text-decoration-thickness: 1px;
    text-underline-offset: 4px;
  }
  summary :global(.icon) {
    flex: none;
    margin-top: 0.2em;
    color: var(--color-muted);
    transition: transform 0.25s var(--site-ease, ease);
  }
  details[open] summary :global(.icon) {
    transform: rotate(45deg);
    color: var(--color-ink);
  }
  .a {
    padding: 0 40px 22px 0;
  }
  .a p {
    margin: 0;
    max-width: 68ch;
    color: var(--color-ink-soft);
    line-height: 1.7;
  }
  @media (prefers-reduced-motion: no-preference) {
    details::details-content {
      block-size: 0;
      overflow: clip;
      transition:
        block-size 0.32s var(--site-ease, ease),
        content-visibility 0.32s allow-discrete;
    }
    details[open]::details-content {
      block-size: auto;
    }
  }
  @media print {
    details::details-content {
      content-visibility: visible;
      block-size: auto;
    }
  }
</style>
