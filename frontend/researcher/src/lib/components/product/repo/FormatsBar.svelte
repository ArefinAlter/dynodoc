<script lang="ts">
  import type { FormatShare } from '$lib/workspace/repo-ui';
  let { shares, variant = 'bar' }: { shares: FormatShare[]; variant?: 'bar' | 'dots' } = $props();
  const pct = (n: number) => `${Number.isInteger(n) ? n : n.toFixed(1)}%`;
</script>

{#if shares.length}
  {#if variant === 'bar'}
    <div class="bar" aria-hidden="true">
      {#each shares as s (s.format.key)}<span
          class={`seg ${s.format.key}`}
          style:width="{s.percent}%"
          title={`${s.format.label} ${pct(s.percent)}`}
        ></span>{/each}
    </div>
    <ul class="legend">
      {#each shares as s (s.format.key)}<li>
          <span class={`dot ${s.format.key}`} aria-hidden="true"></span><strong
            >{s.format.label}</strong
          ><span class="pct">{pct(s.percent)}</span>
        </li>{/each}
    </ul>
  {:else}
    <span class="dots">
      {#each shares as s (s.format.key)}<span class="item"
          ><span class={`dot ${s.format.key}`} aria-hidden="true"></span>{s.format.label}</span
        >{/each}
    </span>
  {/if}
{/if}

<style>
  .bar {
    display: flex;
    height: 8px;
    overflow: hidden;
    border-radius: 999px;
    background: var(--rp-neutral-bg);
    gap: 2px;
  }
  .seg {
    min-width: 6px;
    transition: opacity 0.12s;
  }
  .bar:hover .seg:not(:hover) {
    opacity: 0.55;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
    font-size: 12px;
  }
  .legend li,
  .dots,
  .item {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .dots {
    gap: 12px;
    flex-wrap: wrap;
  }
  .pct {
    color: var(--rp-muted);
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex: none;
  }
  .word {
    background: var(--rp-format-word);
  }
  .excel {
    background: var(--rp-format-excel);
  }
  .powerpoint {
    background: var(--rp-format-powerpoint);
  }
  .other {
    background: var(--rp-format-other);
  }
</style>
