<script lang="ts">
  /** Document canvas for the add-in mockup: paragraph text, a grid, or a slide. */
  let { kind }: { kind: 'text' | 'grid' | 'deck' } = $props();
  const grid = [
    ['Line item', 'Q2', 'Q3', 'Change'],
    ['Software', '12,400', '13,100', '+5.6%'],
    ['Contractors', '8,900', '7,600', '−14.6%'],
    ['Travel', '4,200', '4,650', '+10.7%'],
    ['Events', '6,000', '6,000', '0%'],
    ['Total', '31,500', '31,350', '−0.5%']
  ];
</script>

{#if kind === 'text'}
  <div class="page">
    <div class="pt">Q3 summary</div>
    <div class="pp">
      Revenue rose 12% in Q3, led by enterprise renewals. Operating costs held steady against the
      revised budget.
    </div>
    <div class="pp has">
      Headcount is up by four against plan, with hiring <span class="chg">resuming in Q4</span>
      pending board sign-off.
    </div>
    <i class="ln"></i><i class="ln short"></i>
  </div>
{:else if kind === 'grid'}
  <div class="sheet">
    <div class="gr gh">
      <span></span><span>A</span><span>B</span><span>C</span><span>D</span>
    </div>
    {#each grid as row, r (r)}
      <div class="gr">
        <span class="rn">{r + 1}</span>
        {#each row as cell, c (c)}
          <span class="cell" class:hd={r === 0} class:num={c > 0} class:chg={r === 3 && c === 2}
            >{cell}</span
          >
        {/each}
      </div>
    {/each}
  </div>
{:else}
  <div class="deck">
    <div class="thumbs">
      <span class="th">1</span><span class="th on">2</span><span class="th">3</span>
    </div>
    <div class="slide">
      <div class="st">Launch plan</div>
      <ul class="mk-list sb">
        <li>Beta opens in October</li>
        <li>Pricing review with finance</li>
        <li><span class="chg">Press embargo lifts Nov 4</span></li>
      </ul>
    </div>
  </div>
{/if}

<style>
  .page,
  .slide,
  .sheet {
    background: var(--page);
    color: var(--page-ink);
    border: 1px solid var(--page-rule);
  }
  .page {
    max-width: 520px;
    margin-inline: auto;
    padding: 14px 18px;
    font-size: 12px;
    line-height: 1.55;
  }
  .pt {
    margin-bottom: 6px;
    font-size: 15px;
    font-weight: 700;
  }
  .pp {
    position: relative;
    margin-bottom: 8px;
  }
  .ln {
    display: block;
    height: 5px;
    margin-top: 7px;
    border-radius: 3px;
    background: var(--page-rule);
  }
  .ln.short {
    width: 55%;
  }
  .chg {
    background: var(--page-chg);
    border-radius: 2px;
  }
  .has::before {
    content: '';
    position: absolute;
    left: -10px;
    top: 0;
    bottom: 0;
    width: 3px;
    border-radius: 2px;
    background: var(--mock-accent);
    transform-origin: top;
  }
  .sheet {
    font-size: 12px;
  }
  .gr {
    display: grid;
    grid-template-columns: 22px 1.3fr 1fr 1fr 1fr;
  }
  .gr > span {
    padding: 3px 6px;
    border-right: 1px solid var(--page-rule);
    border-bottom: 1px solid var(--page-rule);
    overflow: hidden;
    white-space: nowrap;
  }
  .gh,
  .rn {
    background: #f6f8fa;
    color: #59636e;
    text-align: center;
  }
  .hd {
    font-weight: 700;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .cell.chg {
    background: var(--page-chg);
    outline: 2px solid var(--mock-accent);
    outline-offset: -2px;
    font-weight: 600;
  }
  .deck {
    display: grid;
    grid-template-columns: 26px minmax(0, 420px);
    justify-content: center;
    gap: 10px;
  }
  .thumbs {
    display: grid;
    align-content: start;
    gap: 6px;
  }
  .th {
    display: block;
    aspect-ratio: 16 / 10;
    border: 1px solid var(--mock-border);
    background: var(--page);
    color: #59636e;
    font-size: 9px;
    text-align: center;
  }
  .th.on {
    border: 2px solid var(--mock-host);
  }
  .slide {
    aspect-ratio: 16 / 9;
    padding: 6% 7%;
  }
  .st {
    margin-bottom: 8px;
    font-size: 16px;
    font-weight: 700;
  }
  .sb {
    display: grid;
    gap: 5px;
    font-size: 12px;
  }
  .sb li::before {
    content: '•';
    margin-right: 6px;
  }

  @media (prefers-reduced-motion: no-preference) {
    :global([data-motion]) .chg {
      animation: mark 0.5s ease-out 0.5s both paused;
    }
    :global([data-motion]) .has::before {
      animation: bar 0.4s ease-out 0.5s both paused;
    }
    :global([data-motion='play']) .has::before,
    :global([data-motion='play']) .chg {
      animation-play-state: running;
    }
  }
  @keyframes mark {
    from {
      background: transparent;
    }
  }
  @keyframes bar {
    from {
      transform: scaleY(0);
    }
  }
</style>
