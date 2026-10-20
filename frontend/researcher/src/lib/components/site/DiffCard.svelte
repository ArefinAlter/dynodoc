<script lang="ts">
  import { onMount } from 'svelte';
  import { parsePatch, type DiffLine } from './diff';
  /**
   * A file path header over `+`/`-` lines, like a pull-request hunk. Supply `lines`, or
   * `patch` (unified-diff text). `typing` types the changed lines out once when the card
   * scrolls into view; the full text is always in the page, and reduced motion or no
   * JavaScript simply shows it.
   */
  let {
    path,
    lines,
    patch,
    meta,
    typing = false,
    class: className = ''
  }: {
    path: string;
    lines?: DiffLine[];
    patch?: string;
    /** Right-aligned header text, e.g. `Maya Chen · 2 min ago`. */
    meta?: string;
    typing?: boolean;
    class?: string;
  } = $props();

  const rows = $derived(lines ?? (patch ? parsePatch(patch) : []));
  const sign = { add: '+', del: '-', ctx: ' ', hunk: '' } as const;
  let phase = $state<'off' | 'armed' | 'run'>('off');
  let root = $state<HTMLElement>();

  // Sequential typing schedule: per-character pace shrinks so the card finishes in ~3s.
  const schedule = $derived.by(() => {
    const changed = rows.filter((row) => row.kind === 'add' || row.kind === 'del');
    const total = changed.reduce((sum, row) => sum + row.text.length, 0) || 1;
    const pace = Math.min(16, 2800 / total);
    let clock = 250;
    return rows.map((row) => {
      if (row.kind !== 'add' && row.kind !== 'del') return { start: 0, chars: 1, pace };
      const entry = { start: Math.round(clock), chars: Math.max(row.text.length, 1), pace };
      clock += entry.chars * pace + 120;
      return entry;
    });
  });

  onMount(() => {
    if (
      !typing ||
      !root ||
      !('IntersectionObserver' in window) ||
      window.matchMedia('(prefers-reduced-motion: reduce)').matches
    )
      return;
    phase = 'armed';
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          phase = 'run';
          observer.disconnect();
        }
      },
      { threshold: 0.4 }
    );
    observer.observe(root);
    return () => observer.disconnect();
  });
</script>

<figure class="diffcard {className}" data-typing={phase} bind:this={root}>
  <figcaption class="bar">
    <span class="path">{path}</span>
    {#if meta}<span class="meta">{meta}</span>{/if}
  </figcaption>
  <pre class="code"><code
      >{#each rows as row, index (index)}<span
          class="ln ln--{row.kind}"
          style="--chars:{schedule[index].chars};--start:{schedule[index].start}ms;--pace:{schedule[
            index
          ].pace}ms"
          ><span class="sign">{sign[row.kind]}</span><span class="txt">{row.text}</span></span
        >{/each}</code
    ></pre>
</figure>

<style>
  .diffcard {
    margin: 0;
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius, 6px);
    background: var(--color-surface-raised, #fff);
    font-family: var(--site-font-mono, ui-monospace, monospace);
    font-size: 0.8125rem;
    line-height: 1.65;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 4px 16px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface-muted);
    color: var(--color-ink-soft);
  }
  .path {
    color: var(--color-ink);
    font-weight: 500;
    overflow-wrap: anywhere;
  }
  .meta {
    color: var(--color-muted);
    font-size: 0.75rem;
  }
  .code {
    margin: 0;
    padding: 8px 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    color: var(--color-ink);
  }
  .code code {
    display: block;
    font: inherit;
  }
  .ln {
    display: flex;
    padding: 0 14px 0 0;
  }
  .sign {
    flex: none;
    width: 2.4em;
    padding-right: 0.5em;
    text-align: right;
    color: var(--color-muted);
    -webkit-user-select: none;
    user-select: none;
  }
  .txt {
    min-width: 0;
    flex: 1;
  }
  .ln--add {
    background: var(--color-diff-add-bg);
    color: var(--color-diff-add-ink);
  }
  .ln--del {
    background: var(--color-diff-del-bg);
    color: var(--color-diff-del-ink);
  }
  .ln--add .sign,
  .ln--del .sign {
    color: inherit;
    font-weight: 600;
  }
  .ln--hunk {
    padding-left: 14px;
    color: var(--color-muted);
  }
  @media (prefers-reduced-motion: no-preference) {
    .diffcard[data-typing='armed'] .ln--add .txt,
    .diffcard[data-typing='armed'] .ln--del .txt {
      clip-path: inset(0 100% 0 0);
    }
    .diffcard[data-typing='run'] .ln--add .txt,
    .diffcard[data-typing='run'] .ln--del .txt {
      animation: type calc(var(--chars) * var(--pace)) steps(var(--chars), end) var(--start) both;
    }
    @keyframes type {
      from {
        clip-path: inset(0 100% 0 0);
      }
      to {
        clip-path: inset(0 0 0 0);
      }
    }
  }
</style>
