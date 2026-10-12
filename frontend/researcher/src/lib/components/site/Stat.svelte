<script lang="ts">
  import { onMount } from 'svelte';
  /**
   * A big number with a label. It counts up once when it scrolls into view. The server
   * renders the final value, so no-JS and reduced-motion readers see it unchanged, and
   * screen readers always get the final value.
   */
  let {
    value,
    label,
    prefix = '',
    suffix = '',
    decimals = 0,
    duration = 1400,
    class: className = ''
  }: {
    value: number;
    label: string;
    prefix?: string;
    suffix?: string;
    decimals?: number;
    /** Count-up length in milliseconds. */
    duration?: number;
    class?: string;
  } = $props();
  const format = (n: number) =>
    n.toLocaleString('en-US', { minimumFractionDigits: decimals, maximumFractionDigits: decimals });
  let current = $state<number | null>(null);
  let root = $state<HTMLElement>();
  const text = $derived(`${prefix}${format(current ?? value)}${suffix}`);

  onMount(() => {
    if (
      !root ||
      !('IntersectionObserver' in window) ||
      window.matchMedia('(prefers-reduced-motion: reduce)').matches
    )
      return;
    let frame = 0;
    const run = () => {
      const target = value;
      const begin = performance.now();
      const tick = (now: number) => {
        const t = Math.min(1, (now - begin) / duration);
        current = target * (1 - Math.pow(1 - t, 3));
        if (t < 1) frame = requestAnimationFrame(tick);
        else current = null;
      };
      frame = requestAnimationFrame(tick);
    };
    current = 0;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        observer.disconnect();
        run();
      },
      { threshold: 0.5 }
    );
    observer.observe(root);
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  });
</script>

<div class="stat {className}" bind:this={root}>
  <p class="value num num--lg">
    <span aria-hidden="true">{text}</span><span class="sr-only"
      >{prefix}{format(value)}{suffix}</span
    >
  </p>
  <p class="label">{label}</p>
</div>

<style>
  .stat {
    display: grid;
    gap: 8px;
    align-content: start;
  }
  p {
    margin: 0;
  }
  .value {
    font-variant-numeric: tabular-nums;
  }
  .label {
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
    line-height: 1.45;
    max-width: 28ch;
  }
</style>
