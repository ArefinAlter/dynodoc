<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { Pause, Play } from '@lucide/svelte';
  /**
   * A looping strip for logos and format names. Pass `<li>` items as children (they are
   * rendered twice; the copy is hidden from assistive tech and focus). It pauses on hover
   * and focus, has a visible pause button (WCAG 2.2.2), and is a static wrapped list
   * under `prefers-reduced-motion`.
   */
  let {
    label,
    speed = 40,
    direction = 'left',
    pauseButton = true,
    class: className = '',
    children
  }: {
    /** Accessible name of the list, e.g. "Supported file formats". */
    label: string;
    /** Seconds for one full loop. */
    speed?: number;
    direction?: 'left' | 'right';
    pauseButton?: boolean;
    class?: string;
    children: Snippet;
  } = $props();
  let ready = $state(false);
  let paused = $state(false);
  onMount(() => {
    ready = true;
  });
</script>

<div
  class="marquee {className}"
  class:paused
  class:reverse={direction === 'right'}
  style:--marquee-duration="{speed}s"
>
  <div class="viewport">
    <div class="track">
      <ul class="lane" aria-label={label}>{@render children()}</ul>
      <ul class="lane copy" aria-hidden="true" inert>{@render children()}</ul>
    </div>
  </div>
  {#if ready && pauseButton}
    <button
      type="button"
      class="toggle"
      aria-pressed={paused}
      aria-label="Pause scrolling"
      onclick={() => (paused = !paused)}
    >
      {#if paused}<Play size={14} aria-hidden="true" />{:else}<Pause
          size={14}
          aria-hidden="true"
        />{/if}
    </button>
  {/if}
</div>

<style>
  .marquee {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .viewport {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    -webkit-mask-image: linear-gradient(90deg, transparent, #000 8%, #000 92%, transparent);
    mask-image: linear-gradient(90deg, transparent, #000 8%, #000 92%, transparent);
  }
  .track {
    display: flex;
    width: max-content;
    animation: scroll var(--marquee-duration, 40s) linear infinite;
  }
  .reverse .track {
    animation-direction: reverse;
  }
  .lane {
    display: flex;
    flex: none;
    align-items: center;
    gap: clamp(28px, 4vw, 56px);
    margin: 0;
    padding: 0 clamp(28px, 4vw, 56px) 0 0;
    list-style: none;
  }
  .marquee:hover .track,
  .marquee:focus-within .track,
  .paused .track {
    animation-play-state: paused;
  }
  .toggle {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: 32px;
    height: 32px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--site-radius-sm, 4px);
    background: transparent;
    color: var(--color-ink-soft);
    cursor: pointer;
  }
  .toggle:hover {
    border-color: var(--color-ink);
    color: var(--color-ink);
  }
  @keyframes scroll {
    to {
      transform: translateX(-50%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .track {
      width: auto;
      animation: none;
    }
    .copy,
    .toggle {
      display: none;
    }
    .lane {
      flex-wrap: wrap;
      justify-content: center;
      padding-right: 0;
    }
    .viewport {
      -webkit-mask-image: none;
      mask-image: none;
    }
  }
</style>
