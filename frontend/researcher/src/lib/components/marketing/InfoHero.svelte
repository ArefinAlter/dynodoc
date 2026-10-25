<script lang="ts">
  import type { Snippet } from 'svelte';
  import { spotlight } from '$lib/components/site';

  /**
   * Hero for the editorial marketing pages (features, why, roadmap, changelog, brand):
   * the page's single h1, a lede, optional actions and a dinosaur plate that
   * stands on the hairline under the hero. Pass a decorative `<Dino>` as `art`.
   */
  let {
    title,
    heading,
    lede,
    actions,
    art
  }: {
    title?: string;
    /** Rich title (use instead of `title` for a highlighted phrase). */
    heading?: Snippet;
    lede: string;
    actions?: Snippet;
    art?: Snippet;
  } = $props();
</script>

<header class="hero dotgrid dotgrid--fade spotlight" use:spotlight>
  <div class="container grid" class:has-art={!!art}>
    <div class="copy">
      <h1 class="h-1">
        {#if heading}{@render heading()}{:else}{title}{/if}
      </h1>
      <p class="lede">{lede}</p>
      {#if actions}<div class="actions">{@render actions()}</div>{/if}
    </div>
    {#if art}<div class="art" aria-hidden="true">{@render art()}</div>{/if}
  </div>
</header>

<style>
  .hero {
    border-bottom: 1px solid var(--color-rule);
    overflow: clip;
  }
  .grid {
    display: grid;
    align-items: end;
    gap: clamp(8px, 4vw, 64px);
  }
  .copy {
    display: grid;
    gap: 22px;
    justify-items: start;
    align-content: start;
    min-width: 0;
    padding-block: clamp(48px, 8vw, 104px) clamp(36px, 6vw, 72px);
  }
  .copy h1 {
    max-width: 18ch;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin-top: 6px;
  }
  .art {
    min-width: 0;
    width: min(100%, 460px);
    justify-self: center;
    margin-bottom: -1px;
    color: var(--color-ink);
    --art-accent: var(--color-accent);
  }
  @media (min-width: 900px) {
    .grid.has-art {
      grid-template-columns: minmax(0, 7fr) minmax(0, 5fr);
    }
    .art {
      justify-self: end;
    }
  }
</style>
