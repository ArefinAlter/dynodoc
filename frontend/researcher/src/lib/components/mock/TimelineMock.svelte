<script lang="ts">
  import { ShieldCheck, ArrowRight } from '@lucide/svelte';
  import MockWindow from './MockWindow.svelte';
  import { useInView } from './useInView';
  import type { MockProps } from './types';

  let { animate = true, label, class: className = '' }: MockProps = $props();

  // Newest first. Each revision's `prev` is the hash of the revision below it.
  const revs = [
    {
      seq: 44,
      who: 'Maya Chen',
      av: 'maya',
      ini: 'MC',
      text: 'Merged “Tighten the Q3 summary”',
      when: '2 h ago',
      prev: 'c04a77f2',
      hash: 'e7d31b09'
    },
    {
      seq: 43,
      who: 'Sam Okoye',
      av: 'sam',
      ini: 'SO',
      text: 'Updated the headcount table',
      when: 'yesterday',
      prev: '5be2d814',
      hash: 'c04a77f2'
    },
    {
      seq: 42,
      who: 'Priya Nair',
      av: 'priya',
      ini: 'PN',
      text: 'Reworded the risks section',
      when: '2 days ago',
      prev: 'a91f03c7',
      hash: '5be2d814'
    },
    {
      seq: 41,
      who: 'Maya Chen',
      av: 'maya',
      ini: 'MC',
      text: 'Imported from Word',
      when: 'last week',
      prev: '00000000',
      hash: 'a91f03c7'
    }
  ];
</script>

<MockWindow title="Board update.docx · History" {label} class={className}>
  <div class="tl" use:useInView={{ animate }}>
    <div class="top">
      <b>History</b>
      <span class="mk-badge"><ShieldCheck size={12} /> 4 revisions, chain intact</span>
    </div>
    <ol class="mk-list revs">
      {#each revs as r, i (r.seq)}
        {@const k = revs.length - 1 - i}
        <li class="rev" style="--k: {k}">
          <span class="node"></span>
          <div class="main">
            <div class="line">
              <span class="seq mk-mono">r{r.seq}</span>
              <span class="mk-av is-{r.av}">{r.ini}</span>
              <b>{r.who}</b>
              <span class="mk-muted when">{r.when}</span>
            </div>
            <div class="txt">{r.text}</div>
            <div class="chain">
              <span class="mk-badge is-success sig"><ShieldCheck size={11} /> Chained</span>
              <span class="mk-hash prev">{r.prev}</span>
              <span class="arrow"><ArrowRight size={12} /></span>
              <span class="mk-hash cur">{r.hash}</span>
            </div>
          </div>
        </li>
      {/each}
    </ol>
  </div>
</MockWindow>

<style>
  .tl {
    padding: 14px 16px 16px;
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 14px;
    font-size: 14px;
  }
  .revs {
    position: relative;
  }
  .rev {
    position: relative;
    padding: 0 0 14px 26px;
  }
  .rev:last-child {
    padding-bottom: 0;
  }
  .rev:not(:last-child)::before {
    content: '';
    position: absolute;
    left: 5px;
    top: 18px;
    bottom: -2px;
    width: 2px;
    background: var(--mock-border);
    transform-origin: bottom;
  }
  .node {
    position: absolute;
    left: 0;
    top: 4px;
    width: 12px;
    height: 12px;
    border: 2px solid var(--mock-accent);
    border-radius: 50%;
    background: var(--mock-canvas);
  }
  .line {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 8px;
  }
  .seq {
    color: var(--mock-accent-fg);
    font-weight: 600;
  }
  .when {
    font-size: 12px;
  }
  .txt {
    margin: 3px 0 7px;
  }
  .chain {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  .arrow {
    display: inline-flex;
    flex: none;
    color: var(--mock-muted);
  }
  .sig {
    margin-right: 4px;
  }

  @media (prefers-reduced-motion: no-preference) {
    .tl:global([data-motion]) .rev::before {
      animation: draw 0.45s ease-out both paused;
      animation-delay: calc(var(--k) * 0.45s - 0.3s);
    }
    .tl:global([data-motion]) .node,
    .tl:global([data-motion]) .main {
      animation: rise 0.45s ease-out both paused;
      animation-delay: calc(var(--k) * 0.45s + 0.15s);
    }
    .tl:global([data-motion]) .cur {
      animation: link 1.1s ease-out both paused;
      animation-delay: calc(var(--k) * 0.45s + 0.45s);
    }
    .tl:global([data-motion]) .prev {
      animation: link 1.1s ease-out both paused;
      animation-delay: calc((var(--k) - 1) * 0.45s + 0.75s);
    }
    .tl:global([data-motion='play']) .rev::before,
    .tl:global([data-motion='play']) :is(.node, .main, .cur, .prev) {
      animation-play-state: running;
    }
  }
  @keyframes draw {
    from {
      transform: scaleY(0);
    }
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  @keyframes link {
    0% {
      background: var(--mock-inset);
      color: var(--mock-muted);
    }
    20%,
    55% {
      background: var(--mock-accent-bg);
      color: var(--mock-accent-fg);
      box-shadow: 0 0 0 1px var(--mock-accent);
    }
  }
</style>
