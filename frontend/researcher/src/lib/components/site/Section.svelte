<script lang="ts">
  import type { Snippet } from 'svelte';
  import Eyebrow from './Eyebrow.svelte';
  import { reveal } from './reveal';
  /**
   * A full-width page section: optional hairline rule with registration marks on top,
   * then a container holding the eyebrow, title and lede, then the content.
   * `tone="ink"` renders an inverted panel (tokens switch to the night palette).
   * Blocks inside marked `data-reveal` rise and fade in as they scroll into view.
   */
  let {
    id,
    eyebrow,
    title,
    lede,
    tone = 'plain',
    rule = true,
    layout = 'stack',
    align = 'start',
    level = 2,
    narrow = false,
    class: className = '',
    heading,
    aside,
    children
  }: {
    id?: string;
    eyebrow?: string;
    title?: string;
    lede?: string;
    tone?: 'plain' | 'muted' | 'ink';
    /** Hairline with crosshair marks above the section. */
    rule?: boolean;
    /** `split` puts the title left and the lede/aside right on wide screens. */
    layout?: 'stack' | 'split';
    align?: 'start' | 'center';
    level?: 2 | 3;
    narrow?: boolean;
    class?: string;
    /** Rich title content (use instead of `title` for <span class="hl"> and line breaks). */
    heading?: Snippet;
    /** Extra block beside the title in `split` layout (below the lede). */
    aside?: Snippet;
    children?: Snippet;
  } = $props();
  const uid = $props.id();
  const titleId = $derived(`${id ?? `section-${uid}`}-title`);
  const hasHead = $derived(!!(eyebrow || title || heading || lede));
  const hasTitle = $derived(!!(title || heading));
</script>

<section
  {id}
  class="sec sec--{tone} {tone === 'muted'
    ? 'tone-muted'
    : tone === 'ink'
      ? 'tone-ink'
      : ''} {className}"
  aria-labelledby={hasTitle ? titleId : undefined}
  use:reveal
>
  {#if rule}<div class="container {narrow ? 'container--narrow' : ''}">
      <hr class="rule" />
    </div>{/if}
  <div class="container {narrow ? 'container--narrow' : ''} body">
    {#if hasHead}
      <header class="head head--{layout} {align === 'center' ? 'head--center' : ''}" data-reveal>
        <div class="titles">
          {#if eyebrow}<Eyebrow label={eyebrow} />{/if}
          {#if hasTitle}
            <svelte:element this={`h${level}`} id={titleId} class="h-1 title">
              {#if heading}{@render heading()}{:else}{title}{/if}
            </svelte:element>
          {/if}
        </div>
        {#if lede || aside}
          <div class="side">
            {#if lede}<p class="lede">{lede}</p>{/if}
            {#if aside}{@render aside()}{/if}
          </div>
        {/if}
      </header>
    {/if}
    {#if children}<div class="content">{@render children()}</div>{/if}
  </div>
</section>

<style>
  .sec {
    position: relative;
  }
  .sec--muted,
  .sec--ink {
    border-block: 1px solid var(--color-border);
  }
  .body {
    padding-block: var(--site-section-y, 96px);
  }
  .head {
    display: grid;
    gap: 20px;
    margin-bottom: clamp(32px, 4vw, 56px);
  }
  .head:last-child {
    margin-bottom: 0;
  }
  .titles {
    display: grid;
    gap: 16px;
    justify-items: start;
    align-content: start;
  }
  .title {
    max-width: 20ch;
  }
  .side {
    display: grid;
    gap: 20px;
    justify-items: start;
    align-content: start;
  }
  .head--split .title {
    max-width: 18ch;
  }
  .head--center {
    justify-items: center;
    text-align: center;
  }
  .head--center .titles,
  .head--center .side {
    justify-items: center;
  }
  .head--center .title {
    max-width: 24ch;
  }
  .head--center .lede {
    margin-inline: auto;
  }
  @media (min-width: 900px) {
    .head--split {
      grid-template-columns: minmax(0, 7fr) minmax(0, 5fr);
      gap: clamp(32px, 6vw, 96px);
      align-items: end;
    }
  }
</style>
