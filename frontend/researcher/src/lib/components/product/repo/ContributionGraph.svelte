<script lang="ts">
  import { buildHeatGrid, type ActivityDay } from '$lib/workspace/activity';
  let {
    year,
    years,
    days,
    loading = false,
    onYear
  }: {
    year: number;
    years: number[];
    days: ActivityDay[];
    loading?: boolean;
    onYear: (year: number) => void;
  } = $props();
  const grid = $derived(buildHeatGrid(year, days));
  let wrap = $state<HTMLElement | null>(null),
    tip = $state<{ x: number; y: number; text: string } | null>(null);
  const label = (date: string, n: number) => {
    const [y, m, d] = date.split('-').map(Number);
    const when = new Date(y, m - 1, d).toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: 'numeric'
    });
    return `${n ? `${n} recorded ${n === 1 ? 'change' : 'changes'}` : 'No recorded changes'} on ${when}`;
  };
  function over(event: PointerEvent) {
    const cell = (event.target as HTMLElement).closest<HTMLElement>('[data-tip]');
    if (!cell || !wrap) {
      tip = null;
      return;
    }
    const a = cell.getBoundingClientRect(),
      b = wrap.getBoundingClientRect();
    tip = {
      x: Math.min(Math.max(a.left - b.left + a.width / 2, 90), b.width - 90),
      y: a.top - b.top,
      text: cell.dataset.tip || ''
    };
  }
</script>

<div class="graph" class:loading bind:this={wrap}>
  <div class="main">
    <div class="scroll">
      <div class="inner" style:--weeks={grid.weeks.length}>
        <div class="months" aria-hidden="true">
          {#each grid.months as m}<span style:grid-column={m.week + 1}>{m.label}</span>{/each}
        </div>
        <div class="body">
          <div class="rows" aria-hidden="true">
            <span>Mon</span><span>Wed</span><span>Fri</span>
          </div>
          <div
            class="cells"
            role="img"
            aria-label={`${grid.total} recorded ${grid.total === 1 ? 'change' : 'changes'} in ${year}`}
            onpointerover={over}
            onpointerleave={() => (tip = null)}
          >
            {#each grid.weeks as week}{#each week as cell}<span
                  class="cell"
                  class:outside={cell.outside}
                  data-level={cell.level}
                  data-tip={cell.outside ? undefined : label(cell.date, cell.count)}
                ></span>{/each}{/each}
          </div>
        </div>
      </div>
    </div>
    <div class="legend" aria-hidden="true">
      <span>Less</span>
      {#each [0, 1, 2, 3, 4] as level}<span class="cell" data-level={level}></span>{/each}
      <span>More</span>
    </div>
  </div>
  {#if years.length > 1}
    <ul class="years" aria-label="Choose a year">
      {#each years as y}<li>
          <button type="button" aria-pressed={y === year} onclick={() => onYear(y)}>{y}</button>
        </li>{/each}
    </ul>
  {/if}
  {#if tip}<div class="tip" role="tooltip" style:left="{tip.x}px" style:top="{tip.y}px">
      {tip.text}
    </div>{/if}
</div>

<style>
  .graph {
    position: relative;
    display: flex;
    gap: 16px;
    align-items: flex-start;
    --cell: 11px;
    --gap: 3px;
  }
  .main {
    min-width: 0;
    flex: 1;
  }
  .scroll {
    overflow-x: auto;
    padding-bottom: 4px;
  }
  .inner {
    width: max-content;
  }
  .months {
    display: grid;
    grid-template-columns: repeat(var(--weeks), var(--cell));
    column-gap: var(--gap);
    margin-left: 32px;
    height: 16px;
    font-size: 12px;
    color: var(--rp-muted);
  }
  .months span {
    white-space: nowrap;
  }
  .body {
    display: flex;
    gap: 4px;
  }
  .rows {
    display: grid;
    grid-template-rows: repeat(7, var(--cell));
    row-gap: var(--gap);
    width: 28px;
    font-size: 11px;
    line-height: var(--cell);
    color: var(--rp-muted);
  }
  .rows span:nth-child(1) {
    grid-row: 2;
  }
  .rows span:nth-child(2) {
    grid-row: 4;
  }
  .rows span:nth-child(3) {
    grid-row: 6;
  }
  .cells {
    display: grid;
    grid-auto-flow: column;
    grid-template-rows: repeat(7, var(--cell));
    grid-auto-columns: var(--cell);
    gap: var(--gap);
  }
  .cell {
    width: var(--cell);
    height: var(--cell);
    border-radius: 2px;
    background: var(--rp-heat-0);
    outline: 1px solid var(--rp-heat-outline);
    outline-offset: -1px;
  }
  .cell[data-level='1'] {
    background: var(--rp-heat-1);
  }
  .cell[data-level='2'] {
    background: var(--rp-heat-2);
  }
  .cell[data-level='3'] {
    background: var(--rp-heat-3);
  }
  .cell[data-level='4'] {
    background: var(--rp-heat-4);
  }
  .cell.outside {
    background: transparent;
    outline: 0;
  }
  .cells .cell:not(.outside):hover {
    outline: 1px solid var(--rp-fg);
    outline-offset: -1px;
    transform: scale(1.25);
    transition: transform 0.08s ease-out;
  }
  .legend {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    margin-top: 6px;
    font-size: 12px;
    color: var(--rp-muted);
  }
  .legend .cell {
    margin: 0;
  }
  .legend span:first-child {
    margin-right: 4px;
  }
  .legend span:last-child {
    margin-left: 4px;
  }
  .years {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .years button {
    min-width: 64px;
    padding: 5px 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--rp-muted);
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    transition:
      background-color 0.12s,
      color 0.12s;
  }
  .years button:hover {
    background: var(--rp-btn-hover);
    color: var(--rp-fg);
  }
  .years button[aria-pressed='true'] {
    background: var(--rp-accent-solid);
    color: #fff;
  }
  .tip {
    position: absolute;
    z-index: 5;
    transform: translate(-50%, calc(-100% - 6px));
    padding: 6px 9px;
    border-radius: 6px;
    background: var(--rp-tip-bg);
    color: var(--rp-tip-fg);
    font-size: 12px;
    white-space: nowrap;
    pointer-events: none;
  }
  .loading .cells {
    opacity: 0.5;
  }
  @media (max-width: 700px) {
    .graph {
      flex-direction: column-reverse;
    }
    .years {
      flex-direction: row;
      flex-wrap: wrap;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .cells .cell:not(.outside):hover {
      transform: none;
      transition: none;
    }
  }
</style>
