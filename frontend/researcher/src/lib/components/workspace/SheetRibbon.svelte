<script lang="ts">
  import RibbonButton from './RibbonButton.svelte';
  import { officeFonts } from '$lib/workspace/office';
  import type { CellFormat, SheetLayout } from '$lib/workspace/sheet-model';
  import {
    ClipboardPaste,
    Scissors,
    Copy,
    Paintbrush,
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Grid2X2,
    PaintBucket,
    Baseline,
    AlignLeft,
    AlignCenter,
    AlignRight,
    AlignVerticalJustifyStart,
    AlignVerticalJustifyCenter,
    AlignVerticalJustifyEnd,
    WrapText,
    Merge,
    Split,
    DollarSign,
    Percent,
    ChartColumn,
    ChartLine,
    ChartPie,
    Table2,
    TableProperties,
    Rows3,
    Columns3,
    Trash2,
    Sigma,
    ArrowDownToLine,
    ArrowRightToLine,
    ArrowDownAZ,
    ArrowUpAZ,
    Filter,
    Search,
    Eraser,
    Printer,
    Undo2,
    Redo2,
    ListFilter,
    Replace,
    RefreshCw,
    MessageSquare,
    Link,
    StickyNote,
    LockKeyhole,
    UnlockKeyhole,
    FunctionSquare,
    Tags,
    ZoomIn,
    ZoomOut,
    Eye,
    EyeOff,
    FileSpreadsheet,
    Rows2,
    Columns2,
    Settings2,
    CheckCheck
  } from '@lucide/svelte';
  let {
    tab = $bindable('Home'),
    editable,
    format = {},
    layout,
    onFormat,
    onAction,
    filter = false,
    showFormulas = false,
    gridlines = true,
    protectedSheet = false,
    hasPivot = false
  }: {
    tab?: string;
    editable: boolean;
    format?: CellFormat;
    layout: SheetLayout;
    onFormat: (patch: Record<string, unknown>) => void;
    onAction: (action: string, value?: string) => void;
    filter?: boolean;
    showFormulas?: boolean;
    gridlines?: boolean;
    protectedSheet?: boolean;
    hasPivot?: boolean;
  } = $props();
  const tabs = ['Home', 'Insert', 'Page Layout', 'Formulas', 'Data', 'Review', 'View'];
  function tabKey(event: KeyboardEvent, index: number) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const i =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? tabs.length - 1
          : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
    tab = tabs[i];
    (event.currentTarget as HTMLElement).parentElement
      ?.querySelectorAll<HTMLButtonElement>('[role=tab]')
      [i].focus();
  }
  function toggle(field: keyof CellFormat) {
    onFormat({ [field]: !format[field] });
  }
</script>

<div class="ribbon-tabs">
  <div class="tab-list" role="tablist" aria-label="Spreadsheet ribbon">
    {#each tabs as name, i}<button
        role="tab"
        id={`sheet-ribbon-${i}`}
        aria-selected={tab === name}
        aria-controls="sheet-ribbon-panel"
        tabindex={tab === name ? 0 : -1}
        onclick={() => (tab = name)}
        onkeydown={(e) => tabKey(e, i)}>{name}</button
      >{/each}
  </div>
  <button class="help" onclick={() => window.dispatchEvent(new Event('dynodoc:help'))}>Help</button>
</div>
<div
  class="ribbon"
  id="sheet-ribbon-panel"
  role="tabpanel"
  aria-labelledby={`sheet-ribbon-${tabs.indexOf(tab)}`}
>
  {#if tab === 'Home'}
    <div class="group">
      <span class="group-label">Clipboard</span>
      <div class="group-body">
        <RibbonButton
          label="Paste cells"
          icon={ClipboardPaste}
          large
          disabled={!editable}
          onclick={() => onAction('paste')}
        />
        <div class="stack">
          <RibbonButton
            label="Cut cells"
            icon={Scissors}
            disabled={!editable}
            onclick={() => onAction('cut')}
          /><RibbonButton
            label="Copy cells"
            icon={Copy}
            onclick={() => onAction('copy')}
          /><RibbonButton
            label="Format Painter"
            icon={Paintbrush}
            disabled={!editable}
            onclick={() => onAction('format-painter')}
          />
        </div>
      </div>
    </div>
    <div class="group">
      <span class="group-label">Font</span>
      <div class="group-body">
        <div class="stack">
          <div class="row">
            <select
              aria-label="Cell font"
              value={format.fontFamily || 'Arial'}
              disabled={!editable}
              onchange={(e) => onFormat({ fontFamily: e.currentTarget.value })}
              >{#each officeFonts as font}<option>{font}</option>{/each}</select
            ><input
              class="size"
              aria-label="Cell font size"
              type="number"
              min="6"
              max="96"
              value={format.fontSize || 11}
              disabled={!editable}
              onchange={(e) => onFormat({ fontSize: Number(e.currentTarget.value) })}
            />
          </div>
          <div class="row">
            <RibbonButton
              label="Bold cells"
              icon={Bold}
              active={!!format.bold}
              disabled={!editable}
              onclick={() => toggle('bold')}
            /><RibbonButton
              label="Italic cells"
              icon={Italic}
              active={!!format.italic}
              disabled={!editable}
              onclick={() => toggle('italic')}
            /><RibbonButton
              label="Underline"
              icon={Underline}
              active={!!format.underline}
              disabled={!editable}
              onclick={() => toggle('underline')}
            /><RibbonButton
              label="Strikethrough"
              icon={Strikethrough}
              active={!!format.strike}
              disabled={!editable}
              onclick={() => toggle('strike')}
            /><RibbonButton
              label="Borders"
              icon={Grid2X2}
              active={!!format.border}
              disabled={!editable}
              onclick={() => toggle('border')}
            /><label class="color" title="Fill color"
              ><PaintBucket size={16} /><input
                type="color"
                aria-label="Cell fill color"
                value={format.background || '#ffffff'}
                disabled={!editable}
                onchange={(e) => onFormat({ background: e.currentTarget.value })}
              /></label
            ><label class="color" title="Font color"
              ><Baseline size={16} /><input
                type="color"
                aria-label="Cell text color"
                value={format.color || '#202124'}
                disabled={!editable}
                onchange={(e) => onFormat({ color: e.currentTarget.value })}
              /></label
            >
          </div>
        </div>
      </div>
    </div>
    <div class="group">
      <span class="group-label">Alignment</span>
      <div class="group-body">
        <div class="stack">
          <div class="row">
            <RibbonButton
              label="Align cells top"
              icon={AlignVerticalJustifyStart}
              disabled={!editable}
              onclick={() => onFormat({ vertical: 'top' })}
            /><RibbonButton
              label="Align cells middle"
              icon={AlignVerticalJustifyCenter}
              disabled={!editable}
              onclick={() => onFormat({ vertical: 'middle' })}
            /><RibbonButton
              label="Align cells bottom"
              icon={AlignVerticalJustifyEnd}
              disabled={!editable}
              onclick={() => onFormat({ vertical: 'bottom' })}
            />
          </div>
          <div class="row">
            <RibbonButton
              label="Align cells left"
              icon={AlignLeft}
              disabled={!editable}
              onclick={() => onFormat({ align: 'left' })}
            /><RibbonButton
              label="Align cells center"
              icon={AlignCenter}
              disabled={!editable}
              onclick={() => onFormat({ align: 'center' })}
            /><RibbonButton
              label="Align cells right"
              icon={AlignRight}
              disabled={!editable}
              onclick={() => onFormat({ align: 'right' })}
            />
          </div>
        </div>
        <div class="stack">
          <button disabled={!editable} class:active={format.wrap} onclick={() => toggle('wrap')}
            ><WrapText size={16} />Wrap Text</button
          ><button
            disabled={!editable}
            aria-label="Merge cells"
            title="Merge & Center"
            onclick={() => onAction('merge')}><Merge size={16} />Merge & Center</button
          >
        </div>
        <RibbonButton
          label="Unmerge"
          icon={Split}
          disabled={!editable}
          onclick={() => onAction('unmerge')}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Number</span>
      <div class="group-body">
        <div class="stack">
          <select
            aria-label="Number format"
            value={format.number || 'auto'}
            disabled={!editable}
            onchange={(e) => onFormat({ number: e.currentTarget.value })}
            >{#each [['auto', 'General'], ['decimal', 'Number'], ['currency', 'Currency'], ['percent', 'Percentage'], ['integer', 'Integer'], ['thousands', 'Comma style'], ['scientific', 'Scientific'], ['date', 'Short Date'], ['text', 'Text']] as [value, label]}<option
                {value}>{label}</option
              >{/each}</select
          >
          <div class="row">
            <RibbonButton
              label="Currency format"
              icon={DollarSign}
              disabled={!editable}
              onclick={() => onFormat({ number: 'currency' })}
            /><RibbonButton
              label="Percent format"
              icon={Percent}
              disabled={!editable}
              onclick={() => onFormat({ number: 'percent' })}
            /><button
              title="Increase decimal places"
              aria-label="Increase decimal places"
              disabled={!editable}
              onclick={() => onAction('decimals', '1')}>.0<span class="arrow">←</span></button
            ><button
              title="Decrease decimal places"
              aria-label="Decrease decimal places"
              disabled={!editable}
              onclick={() => onAction('decimals', '-1')}>.00<span class="arrow">→</span></button
            >
          </div>
        </div>
      </div>
    </div>
    <div class="group">
      <span class="group-label">Styles</span>
      <div class="group-body">
        <RibbonButton
          label="Conditional formatting"
          icon={PaintBucket}
          large
          disabled={!editable}
          onclick={() => onAction('conditional')}
        /><RibbonButton
          label="Format as Table"
          icon={TableProperties}
          large
          disabled={!editable}
          onclick={() => onAction('table-style')}
        /><RibbonButton
          label="Cell Styles"
          icon={Table2}
          large
          disabled={!editable}
          onclick={() => onAction('cell-style')}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Cells</span>
      <div class="group-body">
        <div class="stack">
          <button disabled={!editable} onclick={() => onAction('insert-row')}
            ><Rows3 size={17} />Insert row</button
          ><button disabled={!editable} onclick={() => onAction('insert-column')}
            ><Columns3 size={17} />Insert column</button
          >
        </div>
        <div class="stack">
          <button disabled={!editable} onclick={() => onAction('delete-row')}
            ><Trash2 size={16} />Delete row</button
          ><button disabled={!editable} onclick={() => onAction('dimensions')}
            ><Settings2 size={16} />Format</button
          >
        </div>
      </div>
    </div>
    <div class="group">
      <span class="group-label">Editing</span>
      <div class="group-body">
        <div class="stack">
          <button disabled={!editable} onclick={() => onAction('sum', 'SUM')}
            ><Sigma size={17} />AutoSum</button
          >
          <div class="row">
            <RibbonButton
              label="Fill down"
              icon={ArrowDownToLine}
              disabled={!editable}
              onclick={() => onAction('fill-down')}
            /><RibbonButton
              label="Fill right"
              icon={ArrowRightToLine}
              disabled={!editable}
              onclick={() => onAction('fill-right')}
            /><RibbonButton
              label="Clear contents"
              icon={Eraser}
              disabled={!editable}
              onclick={() => onAction('clear')}
            />
          </div>
        </div>
        <RibbonButton
          label="Sort ascending"
          icon={ArrowDownAZ}
          disabled={!editable}
          onclick={() => onAction('sort-asc')}
        /><RibbonButton
          label="Sort descending"
          icon={ArrowUpAZ}
          disabled={!editable}
          onclick={() => onAction('sort-desc')}
        /><RibbonButton
          label="Filter rows"
          icon={Filter}
          active={filter}
          onclick={() => onAction('filter')}
        /><RibbonButton
          label="Find and replace cells"
          icon={Search}
          large
          onclick={() => onAction('find')}
        />
      </div>
    </div>
  {:else if tab === 'Insert'}
    <div class="group">
      <span class="group-label">Tables</span>
      <div class="group-body">
        <RibbonButton
          label="PivotTable"
          icon={TableProperties}
          large
          disabled={!editable}
          onclick={() => onAction('pivot')}
        /><RibbonButton
          label="Format as Table"
          icon={Table2}
          large
          disabled={!editable}
          onclick={() => onAction('table-style')}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Charts</span>
      <div class="group-body">
        <RibbonButton
          label="Insert column chart"
          icon={ChartColumn}
          large
          disabled={!editable}
          onclick={() => onAction('chart', 'column')}
        /><RibbonButton
          label="Insert line chart"
          icon={ChartLine}
          large
          disabled={!editable}
          onclick={() => onAction('chart', 'line')}
        /><RibbonButton
          label="Insert pie chart"
          icon={ChartPie}
          large
          disabled={!editable}
          onclick={() => onAction('chart', 'pie')}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Worksheet</span>
      <div class="group-body">
        <RibbonButton
          label="New sheet"
          icon={FileSpreadsheet}
          large
          disabled={!editable}
          onclick={() => onAction('new-sheet')}
        /><RibbonButton
          label="Insert row above"
          icon={Rows3}
          large
          disabled={!editable}
          onclick={() => onAction('insert-row')}
        /><RibbonButton
          label="Insert column left"
          icon={Columns3}
          large
          disabled={!editable}
          onclick={() => onAction('insert-column')}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Links & notes</span>
      <div class="group-body">
        <RibbonButton
          label="Insert hyperlink"
          icon={Link}
          large
          disabled={!editable}
          onclick={() => onAction('hyperlink')}
        /><RibbonButton
          label="Cell note"
          icon={StickyNote}
          large
          disabled={!editable}
          onclick={() => onAction('note')}
        /><RibbonButton
          label="Comment"
          icon={MessageSquare}
          large
          onclick={() => onAction('comment')}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Functions</span>
      <div class="group-body">
        <label
          >Summary<select
            aria-label="Insert summary formula"
            disabled={!editable}
            onchange={(e) => {
              onAction('sum', e.currentTarget.value);
              e.currentTarget.value = '';
            }}
            ><option value="">Choose function</option
            >{#each ['SUM', 'AVERAGE', 'COUNT', 'MIN', 'MAX'] as fn}<option>{fn}</option
              >{/each}</select
          ></label
        >
      </div>
    </div>
  {:else if tab === 'Page Layout'}
    <div class="group">
      <span class="group-label">Page setup</span>
      <div class="group-body">
        <RibbonButton
          label="Sheet page setup"
          icon={Settings2}
          large
          disabled={!editable}
          onclick={() => onAction('page-setup')}
        /><RibbonButton
          label="Set print area"
          icon={Table2}
          large
          disabled={!editable}
          onclick={() => onAction('print-area')}
        /><RibbonButton
          label="Clear print area"
          icon={Eraser}
          large
          disabled={!editable}
          onclick={() => onAction('clear-print-area')}
        /><RibbonButton
          label="Print spreadsheet"
          icon={Printer}
          large
          onclick={() => onAction('print')}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Sheet options</span>
      <div class="group-body">
        <button aria-pressed={gridlines} onclick={() => onAction('gridlines')}
          ><Grid2X2 size={17} />Gridlines</button
        ><RibbonButton
          label="Column and row dimensions"
          text="Row & column size"
          icon={Columns3}
          large
          disabled={!editable}
          onclick={() => onAction('dimensions')}
        />
      </div>
    </div>
  {:else if tab === 'Formulas'}
    <div class="group">
      <span class="group-label">Function library</span>
      <div class="group-body">
        <RibbonButton
          label="Insert function"
          icon={FunctionSquare}
          large
          disabled={!editable}
          onclick={() => onAction('function')}
        />{#each ['SUM', 'AVERAGE', 'COUNT', 'MIN', 'MAX'] as fn}<button
            disabled={!editable}
            onclick={() => onAction('sum', fn)}><Sigma size={16} />{fn}</button
          >{/each}
      </div>
    </div>
    <div class="group">
      <span class="group-label">Defined names</span>
      <div class="group-body">
        <RibbonButton
          label="Name Manager"
          icon={Tags}
          large
          disabled={!editable}
          onclick={() => onAction('names')}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Formula auditing</span>
      <div class="group-body">
        <RibbonButton
          label="Show formulas"
          icon={Eye}
          large
          active={showFormulas}
          onclick={() => onAction('formulas')}
        /><RibbonButton
          label="Recalculate workbook"
          icon={RefreshCw}
          large
          onclick={() => onAction('calculate')}
        />
      </div>
    </div>
  {:else if tab === 'Data'}
    <div class="group">
      <span class="group-label">Sort & filter</span>
      <div class="group-body">
        <RibbonButton
          label="Sort ascending"
          icon={ArrowDownAZ}
          large
          disabled={!editable}
          onclick={() => onAction('sort-asc')}
        /><RibbonButton
          label="Sort descending"
          icon={ArrowUpAZ}
          large
          disabled={!editable}
          onclick={() => onAction('sort-desc')}
        /><RibbonButton
          label="Custom sort"
          icon={ListFilter}
          large
          disabled={!editable}
          onclick={() => onAction('sort')}
        /><RibbonButton
          label="Filter rows"
          icon={Filter}
          large
          active={filter}
          onclick={() => onAction('filter')}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Data tools</span>
      <div class="group-body">
        <RibbonButton
          label="Text to Columns"
          icon={Columns2}
          large
          disabled={!editable}
          onclick={() => onAction('split')}
        /><RibbonButton
          label="Remove Duplicates"
          icon={Rows2}
          large
          disabled={!editable}
          onclick={() => onAction('duplicates')}
        /><RibbonButton
          label="Data validation"
          icon={CheckCheck}
          large
          disabled={!editable}
          onclick={() => onAction('validation')}
        /><RibbonButton
          label="Find and replace cells"
          icon={Replace}
          large
          onclick={() => onAction('find')}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">PivotTable</span>
      <div class="group-body">
        <RibbonButton
          label="Refresh PivotTable"
          icon={RefreshCw}
          large
          disabled={!editable || !hasPivot}
          onclick={() => onAction('refresh-pivot')}
        />
      </div>
    </div>
  {:else if tab === 'Review'}
    <div class="group">
      <span class="group-label">Comments & notes</span>
      <div class="group-body">
        <RibbonButton
          label="Comment"
          icon={MessageSquare}
          large
          onclick={() => onAction('comment')}
        /><RibbonButton
          label="Cell note"
          icon={StickyNote}
          large
          disabled={!editable}
          onclick={() => onAction('note')}
        /><RibbonButton
          label="Delete cell note"
          icon={Eraser}
          large
          disabled={!editable}
          onclick={() => onFormat({ note: '' })}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Protection</span>
      <div class="group-body">
        <RibbonButton
          label={protectedSheet ? 'Unprotect sheet' : 'Protect sheet'}
          icon={protectedSheet ? UnlockKeyhole : LockKeyhole}
          large
          onclick={() => onAction('protect')}
        />
      </div>
    </div>
  {:else}
    <div class="group">
      <span class="group-label">Show</span>
      <div class="group-body">
        <RibbonButton
          label="Gridlines"
          icon={Grid2X2}
          large
          active={gridlines}
          onclick={() => onAction('gridlines')}
        /><RibbonButton
          label="Show formulas"
          icon={Eye}
          large
          active={showFormulas}
          onclick={() => onAction('formulas')}
        /><RibbonButton
          label="Show hidden data"
          icon={EyeOff}
          large
          onclick={() => onAction('hidden')}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Freeze panes</span>
      <div class="group-body">
        <label
          >Freeze rows<select
            aria-label="Freeze rows"
            value={layout.frozenRows}
            disabled={!editable}
            onchange={(e) => onAction('freeze-rows', e.currentTarget.value)}
            >{#each [0, 1, 2, 3, 5, 10] as n}<option value={n}>{n === 0 ? 'None' : n}</option
              >{/each}</select
          ></label
        ><label
          >Freeze columns<select
            aria-label="Freeze columns"
            value={layout.frozenColumns}
            disabled={!editable}
            onchange={(e) => onAction('freeze-columns', e.currentTarget.value)}
            >{#each [0, 1, 2, 3, 5] as n}<option value={n}>{n === 0 ? 'None' : n}</option
              >{/each}</select
          ></label
        >
      </div>
    </div>
    <div class="group">
      <span class="group-label">Window</span>
      <div class="group-body">
        <RibbonButton
          label="Column and row dimensions"
          text="Row & column size"
          icon={Settings2}
          large
          disabled={!editable}
          onclick={() => onAction('dimensions')}
        /><RibbonButton
          label="Zoom in"
          icon={ZoomIn}
          large
          onclick={() => onAction('zoom-in')}
        /><RibbonButton
          label="Zoom out"
          icon={ZoomOut}
          large
          onclick={() => onAction('zoom-out')}
        />
      </div>
    </div>
  {/if}
  <div class="group history">
    <span class="group-label">History</span>
    <div class="group-body">
      <RibbonButton
        label="Undo"
        icon={Undo2}
        disabled={!editable}
        onclick={() => onAction('undo')}
      /><RibbonButton
        label="Redo"
        icon={Redo2}
        disabled={!editable}
        onclick={() => onAction('redo')}
      />
    </div>
  </div>
</div>

<style>
  .ribbon-tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 16px;
    height: 34px;
    background: var(--color-chrome);
    flex-shrink: 0;
    overflow-x: auto;
    white-space: nowrap;
  }
  .tab-list {
    display: flex;
    gap: 2px;
  }
  .ribbon-tabs button {
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    padding: 7px 12px;
    font:
      13px 'Segoe UI',
      Arial,
      sans-serif;
    color: var(--color-ink-strong);
    cursor: pointer;
  }
  .ribbon-tabs button[aria-selected='true'] {
    border-color: var(--color-excel);
    font-weight: 600;
  }
  .help {
    margin-left: 10px;
  }
  .ribbon {
    display: flex;
    gap: 0;
    align-items: stretch;
    min-height: 98px;
    height: 98px;
    flex-shrink: 0;
    margin: 0 7px 5px;
    padding: 5px 2px 3px;
    border: 1px solid var(--color-border);
    border-radius: 7px;
    background: var(--color-surface-raised);
    overflow-x: auto;
    color: var(--color-ink-strong);
    font:
      12px 'Segoe UI',
      Arial,
      sans-serif;
  }
  /* Commands above a one-line caption; captions never wrap over the commands. */
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 2px 10px 0;
    border-right: 1px solid var(--color-border);
    flex-shrink: 0;
  }
  .group-body {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: 1;
    min-height: 70px;
  }
  .group-label {
    order: 1;
    text-align: center;
    font-size: 11px;
    line-height: 14px;
    color: var(--color-ink-soft);
    white-space: nowrap;
    padding-bottom: 2px;
  }
  .group:last-child {
    border-right: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
  }
  .history {
    margin-left: auto;
  }
  select,
  input {
    font:
      12px 'Segoe UI',
      Arial,
      sans-serif;
    color: var(--color-ink-strong);
    border: 1px solid var(--color-border-strong);
    background: var(--color-surface-raised);
    border-radius: 2px;
    padding: 4px;
    max-width: 145px;
    min-height: 26px;
  }
  .size {
    width: 43px;
    appearance: textfield;
  }
  .group label:not(.color) {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .group button {
    display: flex;
    align-items: center;
    gap: 5px;
    border: 1px solid transparent;
    background: none;
    border-radius: 3px;
    padding: 4px;
    font:
      12px 'Segoe UI',
      Arial,
      sans-serif;
    color: var(--color-ink-strong);
    cursor: pointer;
    white-space: nowrap;
  }
  .group button:hover,
  .group button.active {
    border-color: var(--color-border-strong);
    background: var(--color-surface-selected);
  }
  button:focus-visible,
  select:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--color-excel);
    outline-offset: 1px;
  }
  button:disabled,
  input:disabled,
  select:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .color {
    width: 27px;
    height: 27px;
    display: grid;
    place-items: center;
    position: relative;
    border-bottom: 3px solid var(--color-excel);
  }
  .color input {
    position: absolute;
    inset: 0;
    opacity: 0;
    width: 100%;
    height: 100%;
    cursor: pointer;
  }
  .arrow {
    font-size: 11px;
  }
</style>
