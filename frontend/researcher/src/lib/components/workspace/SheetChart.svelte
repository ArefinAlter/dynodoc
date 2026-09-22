<script lang="ts">
  import { GripHorizontal, Settings2, X, Expand } from '@lucide/svelte';
  import { chartData, type SheetChart } from '$lib/workspace/sheet-analysis';
  let {
    chart,
    values,
    editable,
    onChange,
    onEdit,
    onRemove
  }: {
    chart: SheetChart;
    values: unknown[][];
    editable: boolean;
    onChange: (chart: SheetChart) => void;
    onEdit: () => void;
    onRemove: () => void;
  } = $props();
  const data = $derived(chartData(chart, values));
  const colors = [
    '#4472c4',
    '#ed7d31',
    '#a5a5a5',
    '#ffc000',
    '#5b9bd5',
    '#70ad47',
    '#7030a0',
    '#264478'
  ];
  const all = $derived(data.series.flatMap((s) => s.values));
  const min = $derived(Math.min(0, ...all)),
    max = $derived(Math.max(1, ...all));
  const y = (v: number) => 222 - ((v - min) / (max - min)) * 180;
  const x = (i: number) => 58 + ((i + 0.5) * 470) / Math.max(1, data.labels.length);
  const total = $derived((data.series[0]?.values || []).reduce((a, v) => a + Math.max(0, v), 0));
  let preview = $state<SheetChart | null>(null),
    drag = $state<{ x: number; y: number; mode: 'move' | 'resize'; scale: number } | null>(null);
  const position = $derived(preview || chart);
  function begin(event: PointerEvent, mode: 'move' | 'resize') {
    if (!editable) return;
    event.preventDefault();
    if (event.currentTarget instanceof HTMLElement)
      event.currentTarget.setPointerCapture(event.pointerId);
    const figure = (event.currentTarget as HTMLElement).closest('figure');
    const scale = figure ? figure.getBoundingClientRect().width / chart.width : 1;
    drag = { x: event.clientX, y: event.clientY, mode, scale };
    preview = { ...chart };
  }
  function move(event: PointerEvent) {
    if (!drag) return;
    const dx = (event.clientX - drag.x) / drag.scale,
      dy = (event.clientY - drag.y) / drag.scale;
    preview =
      drag.mode === 'move'
        ? { ...chart, x: Math.max(0, chart.x + dx), y: Math.max(0, chart.y + dy) }
        : {
            ...chart,
            width: Math.max(360, Math.min(1000, chart.width + dx)),
            height: Math.max(240, Math.min(700, chart.height + dy))
          };
  }
  function end() {
    if (drag && preview) onChange(preview);
    drag = null;
    preview = null;
  }
  function slice(index: number) {
    const items = data.series[0]?.values || [];
    const start =
      (items.slice(0, index).reduce((a, v) => a + Math.max(0, v), 0) / total) * Math.PI * 2 -
      Math.PI / 2;
    const angle = Math.min(
      Math.PI * 2 - 0.00001,
      (Math.max(0, items[index]) / total) * Math.PI * 2
    );
    const end = start + angle;
    return `M 220 125 L ${220 + 95 * Math.cos(start)} ${125 + 95 * Math.sin(start)} A 95 95 0 ${angle > Math.PI ? 1 : 0} 1 ${220 + 95 * Math.cos(end)} ${125 + 95 * Math.sin(end)} Z`;
  }
</script>

<figure
  class="chart"
  style:left={`${position.x}px`}
  style:top={`${position.y}px`}
  style:width={`${position.width}px`}
  style:height={`${position.height}px`}
  aria-label={chart.title}
>
  <header>
    <button
      class="drag"
      aria-label={`Move chart ${chart.title}`}
      title="Drag to move chart"
      disabled={!editable}
      onpointerdown={(e) => begin(e, 'move')}
      onpointermove={move}
      onpointerup={end}
      onpointercancel={() => {
        drag = null;
        preview = null;
      }}
      onkeydown={(e) => {
        if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key)) {
          e.preventDefault();
          onChange({
            ...chart,
            x: Math.max(
              0,
              chart.x + (e.key === 'ArrowLeft' ? -10 : e.key === 'ArrowRight' ? 10 : 0)
            ),
            y: Math.max(0, chart.y + (e.key === 'ArrowUp' ? -10 : e.key === 'ArrowDown' ? 10 : 0))
          });
        }
      }}><GripHorizontal size={16} /></button
    ><strong>{chart.title}</strong>{#if editable}<button
        aria-label={`Edit chart ${chart.title}`}
        onclick={onEdit}><Settings2 size={15} /></button
      ><button aria-label={`Delete chart ${chart.title}`} onclick={onRemove}><X size={15} /></button
      >{/if}
  </header>
  <svg viewBox="0 0 560 270" role="img" aria-label={`${chart.type} chart: ${chart.title}`}>
    {#if chart.type === 'pie'}
      {#if total > 0}{#each data.series[0]?.values || [] as value, i}{#if value > 0}<path
              d={slice(i)}
              fill={colors[i % colors.length]}
              stroke="white"
              stroke-width="1"><title>{data.labels[i]}: {value}</title></path
            >{/if}{/each}{:else}<text x="50" y="100"
          >Select positive numeric values for a pie chart.</text
        >{/if}
      {#each data.labels.slice(0, 10) as label, i}<rect
          x="340"
          y={28 + i * 20}
          width="10"
          height="10"
          fill={colors[i % colors.length]}
        /><text x="356" y={37 + i * 20}>{label.slice(0, 23)}</text>{/each}
    {:else if chart.type === 'bar'}
      {#each data.labels as label, i}<text
          x="8"
          y={28 + (i * 200) / Math.max(1, data.labels.length)}>{label.slice(0, 12)}</text
        >{#each data.series as series, j}{@const value = series.values[i]}<rect
            x={110 + ((0 - min) / (max - min)) * 400}
            y={20 +
              (i * 200) / Math.max(1, data.labels.length) +
              (j * 160) / Math.max(1, data.labels.length) / data.series.length}
            width={(Math.abs(value) / (max - min)) * 400}
            height={140 / Math.max(1, data.labels.length) / data.series.length}
            fill={colors[j % colors.length]}
            transform={value < 0
              ? `translate(${(-Math.abs(value) / (max - min)) * 400},0)`
              : undefined}><title>{label} · {series.name}: {value}</title></rect
          >{/each}{/each}
    {:else}
      {#each [0, 1, 2, 3, 4] as tick}<line
          x1="52"
          y1={42 + tick * 45}
          x2="535"
          y2={42 + tick * 45}
          stroke="#e5e5e5"
        /><text x="46" y={46 + tick * 45} text-anchor="end"
          >{(max - ((max - min) * tick) / 4).toLocaleString(undefined, {
            maximumFractionDigits: 1
          })}</text
        >{/each}
      <line x1="52" y1={y(0)} x2="535" y2={y(0)} stroke="#888" />
      {#each data.series as series, j}
        {#if chart.type === 'line'}<polyline
            points={series.values.map((v, i) => `${x(i)},${y(v)}`).join(' ')}
            fill="none"
            stroke={colors[j % colors.length]}
            stroke-width="2.5"
          />{#each series.values as value, i}<circle
              cx={x(i)}
              cy={y(value)}
              r="2.5"
              fill={colors[j % colors.length]}
              ><title>{data.labels[i]} · {series.name}: {value}</title></circle
            >{/each}
        {:else}{#each series.values as value, i}{@const width =
              350 / Math.max(1, data.labels.length) / Math.max(1, data.series.length)}<rect
              x={x(i) - (width * data.series.length) / 2 + j * width}
              y={Math.min(y(value), y(0))}
              width={Math.max(0.5, width - 1)}
              height={Math.abs(y(value) - y(0))}
              fill={colors[j % colors.length]}
              ><title>{data.labels[i]} · {series.name}: {value}</title></rect
            >{/each}{/if}
      {/each}
      {#each data.labels as label, i}{#if i % Math.max(1, Math.ceil(data.labels.length / 10)) === 0}<text
            x={x(i)}
            y="238"
            text-anchor="middle">{label.slice(0, 11)}</text
          >{/if}{/each}
    {/if}
    {#if chart.type !== 'pie'}{#each data.series.slice(0, 4) as series, i}<rect
          x={80 + i * 120}
          y="253"
          width="9"
          height="9"
          fill={colors[i]}
        /><text x={94 + i * 120} y="261">{series.name.slice(0, 17)}</text>{/each}{/if}
  </svg>
  {#if editable}<button
      class="resize"
      title="Drag to resize chart"
      aria-label={`Resize chart ${chart.title}`}
      onpointerdown={(e) => begin(e, 'resize')}
      onpointermove={move}
      onpointerup={end}
      onpointercancel={() => {
        drag = null;
        preview = null;
      }}><Expand size={13} /></button
    >{/if}
</figure>

<style>
  .chart {
    position: absolute;
    z-index: 8;
    background: white;
    border: 1px solid #b8b8b8;
    box-shadow: 0 2px 7px #0002;
    margin: 0;
    display: flex;
    flex-direction: column;
    color: #333;
    min-width: 360px;
    min-height: 240px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 7px;
    height: 30px;
    flex-shrink: 0;
  }
  strong {
    flex: 1;
    font:
      15px 'Segoe UI',
      Arial,
      sans-serif;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  button {
    border: 0;
    background: transparent;
    padding: 4px;
    display: grid;
    place-items: center;
    color: #555;
    cursor: pointer;
  }
  button:hover {
    background: #eee;
  }
  .drag {
    cursor: move;
    touch-action: none;
  }
  .resize {
    position: absolute;
    bottom: 0;
    right: 0;
    cursor: nwse-resize;
    touch-action: none;
  }
  svg {
    flex: 1;
    width: 100%;
    min-height: 0;
  }
  text {
    font:
      10px 'Segoe UI',
      Arial,
      sans-serif;
    fill: #444;
  }
</style>
