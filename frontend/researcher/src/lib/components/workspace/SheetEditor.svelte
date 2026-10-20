<script lang="ts">
  import PrintPreview from './PrintPreview.svelte';
  import SheetRibbon from './SheetRibbon.svelte';
  import SheetChart from './SheetChart.svelte';
  import OfficeDialog, { type ToolDialog } from './OfficeDialog.svelte';
  import { FormulaEngine, formatGeneral } from '$lib/workspace/formula';
  import {
    sortSheet,
    sheetCharts,
    workbookNames,
    namedRangeOps,
    checkRange,
    pivotOps,
    splitColumn,
    removeDuplicateRows,
    type SheetChart as SheetChartDefinition,
    type PivotDefinition
  } from '$lib/workspace/sheet-analysis';
  import { defaultPage, pageSetup } from '$lib/workspace/office';
  import ContextMenu from '$lib/components/product/ContextMenu.svelte';
  import type { MenuAction } from '$lib/workspace/context-menu';
  import { sheetStructure, type SheetOperation } from '$lib/workspace/sheet-structure';
  import { onMount, untrack } from 'svelte';
  import { WorkbookCalculator } from '$lib/workspace/workbook-calculator';
  import {
    cellFormat,
    sheetLayout,
    workbookInput,
    validationError,
    conditionalStyle,
    type ConditionalRule,
    type CellFormat,
    type SheetLayout
  } from '$lib/workspace/sheet-model';
  import {
    fillRange,
    mergeRange,
    parseAddress,
    rangeAddress
  } from '$lib/workspace/sheet-operations';
  import { bounded, safeColor, safeFont, scriptFontStack } from '$lib/workspace/office';
  import { cellColors } from '$lib/workspace/theme-colors';
  import { theme } from '$lib/theme.svelte';
  import '$lib/workspace/fonts.css';
  import { stableJson } from '$lib/workspace/equality';
  import { ulid } from 'ulid';
  import { tick } from 'svelte';
  import { Plus, Search, Trash2, LockKeyhole } from '@lucide/svelte';
  import type { DocumentState, EventPayload, MaterializedNode } from '$lib/api/types';
  import { rowsForSheet } from '$lib/workspace/files';
  import { between } from '$lib/workspace/changes';
  interface Props {
    documentState: DocumentState;
    editable: boolean;
    onChange: (ops: EventPayload[]) => void;
    onSelect?: (id: string | null) => void;
    onUndo: () => void;
    onRedo: () => void;
    onComment?: () => void;
    onEditingChange?: (editing: boolean) => void;
  }
  let {
    documentState,
    editable: canEdit,
    onChange,
    onSelect,
    onUndo,
    onRedo,
    onComment,
    onEditingChange
  }: Props = $props();
  let printHtml = $state<string | null>(null);
  let grid: HTMLDivElement;
  let preferencesReady = $state(false);
  const preferenceKey = $derived(
    'dynodoc:sheet:' +
      Object.values(documentState.nodes).find((n) => n.type === 'form' && !n.deleted)?.id
  );
  $effect(() => {
    if (preferencesReady && sheet) {
      try {
        localStorage.setItem(preferenceKey, sheet.id);
      } catch {
        /* Storage can be disabled. */
      }
    }
  });
  let activeSheet = $state(''),
    selected = $state({ r: 0, c: 0 }),
    anchor = $state({ r: 0, c: 0 }),
    editing = $state(false),
    value = $state(''),
    find = $state(''),
    filter = $state(false),
    scrollTop = $state(0),
    error = $state('');
  $effect(() => {
    const active = editing;
    untrack(() => onEditingChange?.(active));
  });
  let ribbon = $state('Home'),
    showHidden = $state(false),
    showFormulas = $state(false),
    address = $state(''),
    replacing = $state('');
  const calculator = new WorkbookCalculator();
  // A phone-sized window can read, select cells and use the formula bar, but typing into
  // cells one by one is cramped there. Say so once per visit rather than leave it to guesswork.
  const sizeNoticeKey = 'dynodoc.sheet-size-notice';
  let smallScreen = $state(false),
    sizeNoticeSeen = $state(false);
  function dismissSizeNotice() {
    sizeNoticeSeen = true;
    try {
      sessionStorage.setItem(sizeNoticeKey, '1');
    } catch {
      /* The notice only returns on reload. */
    }
  }
  onMount(() => {
    try {
      activeSheet = localStorage.getItem(preferenceKey) || '';
      sizeNoticeSeen = sessionStorage.getItem(sizeNoticeKey) === '1';
    } catch {
      /* Storage can be disabled. */
    }
    preferencesReady = true;
    const narrow = window.matchMedia('(max-width: 700px)');
    const narrowChanged = () => (smallScreen = narrow.matches);
    narrowChanged();
    narrow.addEventListener('change', narrowChanged);
    window.addEventListener('dynodoc-print', printSheet);
    return () => {
      calculator.destroy();
      narrow.removeEventListener('change', narrowChanged);
      window.removeEventListener('dynodoc-print', printSheet);
    };
  });
  const sheets = $derived(
    Object.values(documentState.nodes)
      .filter((n) => !n.deleted && n.type === 'section' && n.current_fields.kind === 'sheet')
      .sort((a, b) => (a.pos < b.pos ? -1 : a.pos > b.pos ? 1 : 0))
  );
  const sheet = $derived(sheets.find((n) => n.id === activeSheet) || sheets[0]);
  const rows = $derived(sheet ? rowsForSheet(documentState, sheet.id) : []);
  const columns = $derived(Math.min(100, Number(sheet?.current_fields.columns) || 12));
  const layout = $derived(sheetLayout(sheet?.current_fields.sheet_layout));
  const visibleColumns = $derived(
    Array.from({ length: columns }, (_, i) => i).filter(
      (c) => showHidden || !layout.hiddenColumns.includes(c)
    )
  );
  const rowIndex = $derived(new Map(rows.map((n, i) => [n.id, i])));
  const shown = $derived(
    rows.filter(
      (n) =>
        (showHidden || n.current_fields.hidden !== true) &&
        (!filter ||
          !find ||
          Object.entries(n.current_fields).some(
            ([k, v]) =>
              k.startsWith('cell_') && String(v).toLowerCase().includes(find.toLowerCase())
          ))
    )
  );
  const shownIndex = $derived(new Map(shown.map((n, i) => [n.id, i])));
  const rowHeight = (n: MaterializedNode) => bounded(n.current_fields.row_height, 20, 160, 20);
  const offsets = $derived.by(() => {
    const result = [0];
    for (const n of shown) result.push(result.at(-1)! + rowHeight(n));
    return result;
  });
  const frozen = $derived(filter ? 0 : Math.min(layout.frozenRows, shown.length));
  const start = $derived.by(() => {
    let lo = frozen,
      hi = shown.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if (offsets[mid] < scrollTop) lo = mid + 1;
      else hi = mid;
    }
    return Math.max(frozen, lo - 5);
  });
  // Merges may start above the viewport: extend just enough to retain their anchor row.
  const renderStart = $derived.by(() => {
    let first = start;
    for (const m of layout.merges) {
      const a = shownIndex.get(m.fromRow) ?? -1,
        b = shownIndex.get(m.toRow) ?? -1;
      if (a >= frozen && a < first && b >= start) first = a;
    }
    return first;
  });
  const visible = $derived([
    ...shown.slice(0, frozen).map((n, r) => ({ n, r })),
    ...shown.slice(renderStart, start + 65).map((n, i) => ({ n, r: renderStart + i }))
  ]);
  let computed = $state<Record<string, unknown[][]>>({});
  const raw = (r: number, c: number) => String(shown[r]?.current_fields[`cell_${c}`] ?? '');
  const col = (n: number) => {
    let s = '';
    for (n++; n > 0; n = Math.floor((n - 1) / 26)) s = String.fromCharCode(65 + ((n - 1) % 26)) + s;
    return s;
  };
  const calculationInput = $derived(stableJson(workbookInput(documentState)));
  $effect(() => {
    try {
      computed = calculator.calculate(JSON.parse(calculationInput), names);
    } catch {
      computed = {};
    }
  });
  const summary = $derived.by(() => {
    const x = range(),
      numbers: number[] = [];
    let count = 0;
    for (let r = x.r1; r <= x.r2; r++)
      for (let c = x.c1; c <= x.c2; c++) {
        const n = shown[r];
        if (!n) continue;
        const v =
          computed[String(sheet?.current_fields.label || sheet?.id)]?.[rowIndex.get(n.id)!]?.[c];
        if (String(n.current_fields[`cell_${c}`] ?? '')) count++;
        if (typeof v === 'number') numbers.push(v);
      }
    const sum = numbers.reduce((a, b) => a + b, 0);
    return numbers.length
      ? `Count ${count} · Sum ${sum.toLocaleString()} · Average ${(sum / numbers.length).toLocaleString()}`
      : `Count ${count}`;
  });
  function span(r: number, c: number) {
    if (filter || showHidden) return { rows: 1, columns: 1, hidden: false };
    for (const m of layout.merges) {
      const a = shownIndex.get(m.fromRow) ?? -1,
        b = shownIndex.get(m.toRow) ?? -1;
      if (a < 0 || b < a || r < a || r > b || c < m.fromCol || c > m.toCol) continue;
      return {
        rows: b - a + 1,
        columns: visibleColumns.filter((v) => v >= m.fromCol && v <= m.toCol).length,
        hidden: r !== a || c !== m.fromCol
      };
    }
    return { rows: 1, columns: 1, hidden: false };
  }
  function left(c: number) {
    return (
      32 +
      visibleColumns
        .filter((v) => v < c)
        .reduce((sum, v) => sum + (layout.widths[String(v)] || 64), 0)
    );
  }
  function updateLayout(patch: Partial<SheetLayout>) {
    if (sheet && editable)
      onChange([
        {
          type: 'FieldEdited',
          node_id: sheet.id,
          field: 'sheet_layout',
          value: { ...layout, ...patch }
        }
      ]);
  }
  function operation(run: () => void) {
    try {
      run();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not change the sheet.';
    }
  }
  function fill(direction: 'down' | 'right') {
    operation(() => {
      if (!sheet || filter || shown.length !== rows.length)
        throw new Error('Show all rows and clear the filter before filling cells.');
      onChange(fillRange(documentState, sheet, range(), direction));
    });
  }
  function merge(split = false) {
    operation(() => {
      if (!sheet || filter || shown.length !== rows.length)
        throw new Error('Show all rows and clear the filter before merging cells.');
      const selectedRange = range();
      const ops = mergeRange(documentState, sheet, selectedRange, split);
      if (!split)
        ops.push({
          type: 'FieldEdited',
          node_id: shown[selectedRange.r1].id,
          field: `format_${selectedRange.c1}`,
          value: {
            ...cellFormat(shown[selectedRange.r1], selectedRange.c1),
            align: 'center',
            vertical: 'middle'
          }
        });
      onChange(ops);
      choose(selectedRange.r1, selectedRange.c1);
    });
  }
  function autoSum(fn: string) {
    const x = range();
    if (!sheet || filter || shown.length !== rows.length) {
      error = 'Show all rows before inserting a summary formula.';
      return;
    }
    const n = shown[x.r2 + 1];
    if (!n) {
      error = 'Add a row below the selected range for the result.';
      return;
    }
    if (String(n.current_fields[`cell_${x.c1}`] || '')) {
      error = 'The cell below the selection must be empty.';
      return;
    }
    onChange([
      {
        type: 'FieldEdited',
        node_id: n.id,
        field: `cell_${x.c1}`,
        value: `=${fn}(${rangeAddress(x)})`
      },
      {
        type: 'FieldEdited',
        node_id: n.id,
        field: `format_${x.c1}`,
        value: { ...cellFormat(n, x.c1), type: 'number' }
      }
    ]);
  }
  function goToAddress() {
    const named = names.find((n) => n.name.toLowerCase() === address.toLowerCase());
    const reference = named?.expression.match(/^='((?:[^']|'')+)'!(.+)$/);
    if (reference) {
      const target = sheets.find(
        (s) => s.current_fields.label === reference[1].replaceAll("''", "'")
      );
      if (target && target.id !== sheet?.id) {
        activeSheet = target.id;
        address = reference[2].replaceAll('$', '');
        void tick().then(goToAddress);
        return;
      }
    }
    const x = parseAddress(reference ? reference[2].replaceAll('$', '') : address);
    if (!x || x.r2 >= rows.length || x.c2 >= columns) {
      error = 'Enter an existing cell or range, such as B2 or A1:C10.';
      return;
    }
    if (filter || shown.length !== rows.length) {
      error = 'Clear filters and show hidden rows before going to a range.';
      return;
    }
    choose(x.r2, x.c2);
    anchor = { r: x.r1, c: x.c1 };
  }
  function replaceCells() {
    if (!find || !editable) return;
    const ops: EventPayload[] = [];
    for (const n of shown)
      for (let c = 0; c < columns; c++) {
        const old = String(n.current_fields[`cell_${c}`] ?? '');
        if (old.startsWith('=') || !old.includes(find)) continue;
        const value = old.split(find).join(replacing),
          problem = validationError(value, cellFormat(n, c));
        if (problem) {
          error = problem;
          return;
        }
        ops.push({ type: 'FieldEdited', node_id: n.id, field: `cell_${c}`, value });
      }
    if (ops.length > 4000) {
      error = 'Replace up to 4,000 cells at a time.';
      return;
    }
    onChange(ops);
  }
  function hideRows(hidden: boolean) {
    const x = range();
    onChange(
      (hidden ? shown.slice(x.r1, x.r2 + 1) : rows).map((n) => ({
        type: 'FieldEdited',
        node_id: n.id,
        field: 'hidden',
        value: hidden
      }))
    );
  }
  function sizeRows(height: number) {
    const x = range();
    onChange(
      shown.slice(x.r1, x.r2 + 1).map((n) => ({
        type: 'FieldEdited',
        node_id: n.id,
        field: 'row_height',
        value: bounded(height, 20, 160, 20)
      }))
    );
  }
  function display(n: MaterializedNode, c: number) {
    const r = rowIndex.get(n.id)!;
    if (showFormulas) return String(n.current_fields[`cell_${c}`] ?? '');
    const result = computed[String(sheet?.current_fields.label || sheet?.id)]?.[r]?.[c];
    if (result && typeof result === 'object')
      return String((result as { value?: unknown }).value || '#ERROR!');
    const format = cellFormat(n, c);
    if (
      format.type === 'date' &&
      Number.isFinite(Date.parse(String(n.current_fields[`cell_${c}`])))
    )
      return new Date(String(n.current_fields[`cell_${c}`])).toISOString().slice(0, 10);
    if (typeof result === 'number') {
      const places = Math.max(0, Math.min(10, format.decimals ?? 2));
      if (format.number === 'date')
        return new Date((result - 25569) * 86400000).toISOString().slice(0, 10);
      if (format.number === 'text') return String(result);
      if (format.number === 'percent' || (!format.number && format.numFmt?.includes('%')))
        return (result * 100).toFixed(format.decimals ?? 1) + '%';
      if (format.number === 'integer') return result.toFixed(0);
      if (format.number === 'scientific') return result.toExponential(places);
      if (format.number === 'thousands')
        return result.toLocaleString('en-US', {
          minimumFractionDigits: places,
          maximumFractionDigits: places
        });
      if (format?.number === 'decimal') return result.toFixed(places);
      if (format?.number === 'currency')
        return result.toLocaleString('en-US', {
          style: 'currency',
          currency: 'USD',
          minimumFractionDigits: places,
          maximumFractionDigits: places
        });
    }
    // Whole numbers keep every digit; fractions are trimmed the way Excel's General format does.
    if (typeof result === 'number')
      return Number.isSafeInteger(result) ? String(result) : formatGeneral(result);
    return result === null || result === undefined
      ? String(n.current_fields[`cell_${c}`] ?? '')
      : String(result);
  }
  let conditionalDialog: HTMLDialogElement;
  let condition = $state<ConditionalRule['operator']>('greaterThan'),
    threshold = $state(0),
    upper = $state(100),
    conditionalFill = $state('#ffc7ce'),
    conditionalColor = $state('#9c0006');
  function applyConditional() {
    if (
      !Number.isFinite(threshold) ||
      (condition === 'between' && (!Number.isFinite(upper) || upper < threshold))
    )
      return;
    format({
      conditional: [
        {
          operator: condition,
          value: threshold,
          upper,
          background: conditionalFill,
          color: conditionalColor
        }
      ]
    });
    conditionalDialog.close();
  }
  function style(n: MaterializedNode, c: number, r: number, printing = false) {
    const value =
      computed[String(sheet?.current_fields.label)]?.[rowIndex.get(n.id)!]?.[c] ??
      n.current_fields[`cell_${c}`];
    const f = conditionalStyle(value, cellFormat(n, c)),
      sticky = !printing && (r < frozen || c < layout.frozenColumns);
    // Print keeps the day defaults; on screen, unformatted cells use the grid tokens and
    // authored fills and fonts stay as written (see cellColors).
    const paint = printing
      ? { background: safeColor(f.background, 'white'), color: safeColor(f.color, '#303b47') }
      : cellColors(
          { fill: safeColor(f.background), color: safeColor(f.color) },
          theme.resolved === 'dark'
        );
    return `font-family:${safeFont(f.fontFamily)},${scriptFontStack};font-size:${bounded(f.fontSize, 6, 96, 11)}pt;font-weight:${f.bold ? '700' : '400'};font-style:${f.italic ? 'italic' : 'normal'};text-decoration:${[f.underline ? 'underline' : '', f.strike ? 'line-through' : ''].join(' ')};text-align:${['left', 'center', 'right'].includes(f.align || '') ? f.align : 'left'};vertical-align:${['top', 'middle', 'bottom'].includes(f.vertical || '') ? f.vertical : 'middle'};background:${paint.background ?? 'var(--color-grid-bg)'};color:${paint.color ?? 'var(--color-grid-ink)'};white-space:${f.wrap ? 'normal' : 'nowrap'};${f.border ? 'border:1px solid #718d7e;' : ''}${sticky ? `position:sticky;z-index:${r < frozen && c < layout.frozenColumns ? 5 : 4};${r < frozen ? `top:${20 + offsets[r]}px;` : ''}${c < layout.frozenColumns ? `left:${left(c)}px;` : ''}` : ''}`;
  }
  function printSheet() {
    // Generate all visible rows from state, never from the virtualized screen DOM.
    const table = document.createElement('table');
    const area =
      typeof sheet?.current_fields.print_area === 'string'
        ? parseAddress(sheet.current_fields.print_area)
        : null;
    for (let r = 0; r < shown.length; r++) {
      const index = rowIndex.get(shown[r].id) ?? r;
      if (area && (index < area.r1 || index > area.r2)) continue;
      const tr = table.insertRow();
      for (const c of visibleColumns) {
        if (area && (c < area.c1 || c > area.c2)) continue;
        const merged = span(r, c);
        if (merged.hidden) continue;
        const td = tr.insertCell();
        td.textContent = display(shown[r], c);
        td.rowSpan = merged.rows;
        td.colSpan = merged.columns;
        td.setAttribute('style', style(shown[r], c, r, true) + ';white-space:pre-wrap');
      }
    }
    const figures = [...grid.querySelectorAll('figure.chart')].map((figure, i) => {
      const wrapper = document.createElement('section'),
        title = document.createElement('h2');
      title.textContent = charts[i]?.title || 'Chart';
      wrapper.append(title);
      const svg = figure.querySelector('svg')?.cloneNode(true) as SVGElement | undefined;
      if (svg) {
        svg.style.width = '100%';
        svg.style.height = 'auto';
        wrapper.append(svg);
      }
      wrapper.style.breakInside = 'avoid';
      return wrapper.outerHTML;
    });
    printHtml = table.outerHTML + figures.join('');
  }
  function range() {
    return {
      r1: Math.min(selected.r, anchor.r),
      r2: Math.max(selected.r, anchor.r),
      c1: Math.min(selected.c, anchor.c),
      c2: Math.max(selected.c, anchor.c)
    };
  }
  function inRange(r: number, c: number) {
    const x = range();
    return r >= x.r1 && r <= x.r2 && c >= x.c1 && c <= x.c2;
  }
  function choose(r: number, c: number, extend = false) {
    if (editing) {
      commit();
      if (editing) return;
    }
    selected = {
      r: Math.max(0, Math.min(shown.length - 1, r)),
      c: Math.max(0, Math.min(columns - 1, c))
    };
    if (!visibleColumns.includes(selected.c))
      selected.c = visibleColumns.find((c) => c >= selected.c) ?? visibleColumns.at(-1) ?? 0;
    if (!filter && !showHidden) {
      for (const m of layout.merges) {
        const a = shownIndex.get(m.fromRow),
          b = shownIndex.get(m.toRow);
        if (
          a !== undefined &&
          b !== undefined &&
          selected.r >= a &&
          selected.r <= b &&
          selected.c >= m.fromCol &&
          selected.c <= m.toCol &&
          visibleColumns.includes(m.fromCol)
        ) {
          selected = { r: a, c: m.fromCol };
          break;
        }
      }
    }
    if (!extend) anchor = { ...selected };
    if (paintFormat && editable) {
      format(paintFormat as Record<string, unknown>);
      paintFormat = null;
    }
    value = raw(selected.r, selected.c);
    onSelect?.(shown[selected.r]?.id || null);
    if (grid) {
      const scale = zoom / 100,
        top = (offsets[selected.r] || 0) * scale;
      if (top < grid.scrollTop) grid.scrollTop = top;
      else if (top + 40 * scale > grid.scrollTop + grid.clientHeight)
        grid.scrollTop = top + 40 * scale - grid.clientHeight;
      scrollTop = grid.scrollTop / scale;
    }
  }
  async function edit(initial?: string) {
    if (!editable) return;
    value = initial ?? raw(selected.r, selected.c);
    editing = true;
    await tick();
    document.querySelector<HTMLInputElement>('.cell-entry')?.focus();
  }
  function commit() {
    if (!editing) return;
    const n = shown[selected.r];
    const problem = n ? validationError(value, cellFormat(n, selected.c)) : null;
    if (problem) {
      error = problem;
      return;
    }
    editing = false;
    if (n && value !== raw(selected.r, selected.c))
      onChange([
        { type: 'FieldEdited', node_id: n.id, field: `cell_${selected.c}`, value },
        ...(value.startsWith('=') && cellFormat(n, selected.c).type === 'text'
          ? [
              {
                type: 'FieldEdited',
                node_id: n.id,
                field: `format_${selected.c}`,
                value: { ...cellFormat(n, selected.c), type: 'number' }
              } as EventPayload
            ]
          : [])
      ]);
  }
  function format(patch: Record<string, unknown>) {
    if (!editable) return;
    const x = range();
    if ((x.r2 - x.r1 + 1) * (x.c2 - x.c1 + 1) > 4000) {
      error = 'Change up to 4,000 cells at a time.';
      return;
    }
    const ops: EventPayload[] = [];
    for (let r = x.r1; r <= x.r2; r++) {
      const n = shown[r];
      if (!n) continue;
      for (let c = x.c1; c <= x.c2; c++)
        ops.push({
          type: 'FieldEdited',
          node_id: n.id,
          field: `format_${c}`,
          value: { ...((n.current_fields[`format_${c}`] as object) || {}), ...patch }
        });
    }
    onChange(ops);
  }
  function toggleFormat(key: string) {
    const n = shown[selected.r];
    format({
      [key]: !(n?.current_fields[`format_${selected.c}`] as Record<string, unknown>)?.[key]
    });
  }
  function clear() {
    if (!editable) return;
    const x = range();
    if ((x.r2 - x.r1 + 1) * (x.c2 - x.c1 + 1) > 4000) {
      error = 'Change up to 4,000 cells at a time.';
      return;
    }
    const ops: EventPayload[] = [];
    for (let r = x.r1; r <= x.r2; r++)
      for (let c = x.c1; c <= x.c2; c++)
        if (shown[r])
          ops.push({ type: 'FieldEdited', node_id: shown[r].id, field: `cell_${c}`, value: '' });
    onChange(ops);
  }
  function key(e: KeyboardEvent) {
    if (e.isComposing || e.keyCode === 229) return;
    if ((e.target as HTMLElement).closest('input,select')) return;
    if ((e.ctrlKey || e.metaKey) && ['d', 'r'].includes(e.key)) {
      e.preventDefault();
      fill(e.key === 'd' ? 'down' : 'right');
      return;
    }
    if (editable && (e.ctrlKey || e.metaKey) && e.key === 'z') {
      e.preventDefault();
      if (e.shiftKey) onRedo();
      else onUndo();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      anchor = { r: 0, c: 0 };
      selected = { r: shown.length - 1, c: columns - 1 };
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      searchOpen = true;
      return;
    }
    if (e.key === 'Home') {
      e.preventDefault();
      choose(e.ctrlKey ? 0 : selected.r, 0, e.shiftKey);
      return;
    }
    if (e.key === 'End' && e.ctrlKey) {
      e.preventDefault();
      choose(shown.length - 1, columns - 1, e.shiftKey);
      return;
    }
    const moves: Record<string, [number, number]> = {
      ArrowUp: [-1, 0],
      ArrowDown: [1, 0],
      ArrowLeft: [0, -1],
      ArrowRight: [0, 1],
      Tab: [0, e.shiftKey ? -1 : 1],
      Enter: [1, 0]
    };
    if (moves[e.key]) {
      e.preventDefault();
      const [r, c] = moves[e.key];
      if (!shown.length || !visibleColumns.length) return;
      const merged = span(selected.r, selected.c);
      const index = visibleColumns.indexOf(selected.c);
      const targetColumn = c
        ? visibleColumns[
            Math.max(0, Math.min(visibleColumns.length - 1, index + (c > 0 ? merged.columns : -1)))
          ]
        : selected.c;
      choose(selected.r + (r > 0 ? merged.rows : r), targetColumn, e.shiftKey && e.key !== 'Tab');
    } else if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      clear();
    } else if (e.key === 'F2') {
      e.preventDefault();
      void edit();
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      void edit(e.key);
    }
  }
  function paste(e: ClipboardEvent) {
    if (!editable || editing || (e.target as HTMLElement).tagName === 'INPUT') return;
    const text = e.clipboardData?.getData('text/plain');
    if (!text) return;
    e.preventDefault();
    pasteText(text);
  }
  function pasteText(text: string) {
    if (!editable || !text) return;
    const cells = text
      .replace(/\r/g, '')
      .replace(/\n$/, '')
      .split('\n')
      .map((r) => r.split('\t'));
    if (
      selected.r + cells.length > shown.length ||
      Math.max(...cells.map((r) => r.length)) + selected.c > columns
    ) {
      error = 'Add enough rows and columns before pasting this range.';
      return;
    }
    if (cells.reduce((count, row) => count + row.length, 0) > 4000) {
      error = 'Paste up to 4,000 cells at a time.';
      return;
    }
    for (const [r, row] of cells.entries())
      for (const [c, value] of row.entries()) {
        const problem = validationError(value, cellFormat(shown[selected.r + r], selected.c + c));
        if (problem) {
          error = problem;
          return;
        }
      }
    const ops: EventPayload[] = [];
    cells.forEach((row, r) =>
      row.forEach((value, c) =>
        ops.push({
          type: 'FieldEdited',
          node_id: shown[selected.r + r].id,
          field: `cell_${selected.c + c}`,
          value
        })
      )
    );
    onChange(ops);
  }
  function copy(e: ClipboardEvent) {
    if (editing || (e.target as HTMLElement).tagName === 'INPUT') return;
    const x = range();
    e.clipboardData?.setData(
      'text/plain',
      shown
        .slice(x.r1, x.r2 + 1)
        .map((n, r) =>
          Array.from({ length: x.c2 - x.c1 + 1 }, (_, c) => raw(x.r1 + r, x.c1 + c)).join('\t')
        )
        .join('\n')
    );
    e.preventDefault();
  }
  function addRows(count = 1) {
    if (!sheet) return;
    const last = rows.at(-1)?.pos || '';
    let pos = last;
    onChange(
      Array.from({ length: Math.min(count, 3000 - rows.length) }, () => {
        pos = between(pos);
        return {
          type: 'NodeCreated',
          node_id: ulid(),
          node_type: 'item',
          parent_id: sheet.id,
          pos,
          fields: {}
        };
      })
    );
  }
  function addSheet() {
    const root = Object.values(documentState.nodes).find((n) => n.type === 'form' && !n.deleted);
    if (!root) return;
    const id = ulid();
    let name = `Sheet ${sheets.length + 1}`;
    while (sheets.some((s) => s.current_fields.label === name)) name += ' copy';
    onChange([
      {
        type: 'NodeCreated',
        node_id: id,
        node_type: 'section',
        parent_id: root.id,
        pos: between(sheets.at(-1)?.pos || ''),
        fields: { label: name, kind: 'sheet', columns: 30 }
      },
      ...Array.from(
        { length: 100 },
        (_, i): EventPayload => ({
          type: 'NodeCreated',
          node_id: ulid(),
          node_type: 'item',
          parent_id: id,
          pos: String(i).padStart(8, '0'),
          fields: {}
        })
      )
    ]);
    activeSheet = id;
    choose(0, 0);
  }
  function sort(desc: boolean) {
    if (!sheet || !editable) return;
    operation(() =>
      onChange(
        sortSheet(documentState, sheet, selected.c, desc, !!sheet.current_fields.table_range)
      )
    );
  }
  function removeRow() {
    resize('row');
  }
  function structure(action: SheetOperation, index = 0, name = '') {
    if (!sheet || !editable) return;
    try {
      onChange(sheetStructure(documentState, sheet.id, action, index, name));
      if (action === 'delete-sheet') activeSheet = sheets.find((s) => s.id !== sheet.id)?.id || '';
      choose(0, 0);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not update the sheet';
    }
  }
  function resize(kind: 'row' | 'column') {
    structure(
      kind === 'row' ? 'delete-row' : 'delete-column',
      kind === 'row' ? rows.findIndex((n) => n.id === shown[selected.r]?.id) : selected.c
    );
  }
  function insert(kind: 'row' | 'column', after = false) {
    structure(
      kind === 'row' ? 'insert-row' : 'insert-column',
      (kind === 'row' ? rows.findIndex((n) => n.id === shown[selected.r]?.id) : selected.c) +
        (after ? 1 : 0)
    );
  }
  function renameSheet() {
    if (!sheet) return;
    const name = prompt('Rename sheet', String(sheet.current_fields.label));
    if (name) structure('rename', 0, name);
  }
  function deleteSheet() {
    if (
      confirm(
        'Delete this sheet? Formulas that reference it will show #REF!. Restore from version history if needed.'
      )
    )
      structure('delete-sheet');
  }
  function duplicateSheet() {
    if (!sheet || !editable) return;
    const id = ulid();
    const rowIds = new Map(rows.map((n) => [n.id, ulid()]));
    const copiedLayout = {
      ...layout,
      merges: layout.merges.map((m) => ({
        ...m,
        fromRow: rowIds.get(m.fromRow)!,
        toRow: rowIds.get(m.toRow)!
      }))
    };
    let name = String(sheet.current_fields.label).slice(0, 23) + ' copy';
    let i = 2;
    while (sheets.some((s) => s.current_fields.label === name))
      name = String(sheet.current_fields.label).slice(0, 20) + ` copy ${i++}`;
    onChange([
      {
        type: 'NodeCreated',
        node_id: id,
        node_type: 'section',
        parent_id: sheet.parent_id,
        pos: between(sheets.at(-1)?.pos || ''),
        fields: { ...sheet.current_fields, label: name, sheet_layout: copiedLayout }
      },
      ...rows.map(
        (n) =>
          ({
            type: 'NodeCreated',
            node_id: rowIds.get(n.id)!,
            node_type: 'item',
            parent_id: id,
            pos: n.pos,
            fields: JSON.parse(JSON.stringify(n.current_fields))
          }) as EventPayload
      )
    ]);
    activeSheet = id;
  }
  function moveSheet(direction: number) {
    if (!sheet || !editable) return;
    const other = sheets[sheets.findIndex((s) => s.id === sheet.id) + direction];
    if (!other) return;
    onChange([
      { type: 'NodeMoved', node_id: sheet.id, new_parent_id: sheet.parent_id, new_pos: other.pos },
      { type: 'NodeMoved', node_id: other.id, new_parent_id: other.parent_id, new_pos: sheet.pos }
    ]);
  }
  let context = $state<{ x: number; y: number; items: MenuAction[] } | null>(null);
  async function menuClipboard(action: string) {
    try {
      if (action === 'paste') {
        pasteText(await navigator.clipboard.readText());
        return;
      }
      const x = range();
      await navigator.clipboard.writeText(
        shown
          .slice(x.r1, x.r2 + 1)
          .map((_, r) =>
            Array.from({ length: x.c2 - x.c1 + 1 }, (_, c) => raw(x.r1 + r, x.c1 + c)).join('\t')
          )
          .join('\n')
      );
      if (action === 'cut' && editable) clear();
    } catch {
      error =
        'Your browser blocked clipboard access. Use Ctrl+C, Ctrl+X or Ctrl+V on a keyboard, or press and hold in the sheet on a touch screen.';
    }
  }
  function openCellMenu(e: MouseEvent, r: number, c: number) {
    e.preventDefault();
    if (!inRange(r, c)) choose(r, c);
    onSelect?.(shown[r]?.id || null);
    context = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: 'Cut cells', action: () => menuClipboard('cut'), disabled: !editable },
        { label: 'Copy cells', action: () => menuClipboard('copy') },
        ...(selectedFormat.hyperlink && /^(https?:\/\/|mailto:)/i.test(selectedFormat.hyperlink)
          ? [
              {
                label: 'Open hyperlink',
                action: () => {
                  window.open(selectedFormat.hyperlink, '_blank', 'noopener,noreferrer');
                }
              }
            ]
          : []),
        { label: 'Paste cells', action: () => menuClipboard('paste'), disabled: !editable },
        ...(onComment
          ? [{ label: 'Add comment', action: () => onComment?.(), separator: true }]
          : []),
        {
          label: 'Insert row above',
          action: () => insert('row'),
          disabled: !editable,
          separator: true
        },
        { label: 'Insert row below', action: () => insert('row', true), disabled: !editable },
        { label: 'Insert column left', action: () => insert('column'), disabled: !editable },
        { label: 'Insert column right', action: () => insert('column', true), disabled: !editable },
        { label: 'Delete row', action: () => resize('row'), disabled: !editable },
        { label: 'Delete column', action: () => resize('column'), disabled: !editable },
        { label: 'Clear contents', action: clear, disabled: !editable, separator: true },
        { label: 'Bold', action: () => toggleFormat('bold'), disabled: !editable },
        {
          label: 'Clear formatting',
          action: () => {
            const x = range();
            const ops: EventPayload[] = [];
            for (let rr = x.r1; rr <= x.r2; rr++)
              for (let cc = x.c1; cc <= x.c2; cc++)
                ops.push({
                  type: 'FieldEdited',
                  node_id: shown[rr].id,
                  field: `format_${cc}`,
                  value: {}
                });
            onChange(ops);
          },
          disabled: !editable
        }
      ]
    };
  }
  function openSheetMenu(e: MouseEvent, id: string) {
    e.preventDefault();
    if (editing) commit();
    activeSheet = id;
    context = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: 'Rename sheet', action: renameSheet, disabled: !editable },
        { label: 'Duplicate sheet', action: duplicateSheet, disabled: !editable },
        { label: 'Move left', action: () => moveSheet(-1), disabled: !editable },
        { label: 'Move right', action: () => moveSheet(1), disabled: !editable },
        {
          label: 'Delete sheet',
          action: deleteSheet,
          disabled: !editable || sheets.length <= 1,
          separator: true
        }
      ]
    };
  }
  let tool = $state<ToolDialog | null>(null),
    searchOpen = $state(false),
    zoom = $state(100),
    gridlines = $state(true);
  let paintFormat = $state<CellFormat | null>(null);
  const protectedSheet = $derived(sheet?.current_fields.sheet_protected === true);
  const editable = $derived(canEdit && !protectedSheet);
  const charts = $derived(sheetCharts(sheet?.current_fields.charts));
  const names = $derived(workbookNames(documentState));
  const selectedFormat = $derived(
    shown[selected.r] ? cellFormat(shown[selected.r], selected.c) : {}
  );
  const printPage = $derived(
    pageSetup(sheet?.current_fields.page_setup || { ...defaultPage, landscape: true, margin: 15 })
  );
  const functionNames = FormulaEngine.getRegisteredFunctionNames();
  function sheetField(field: string, value: unknown) {
    if (sheet && canEdit) onChange([{ type: 'FieldEdited', node_id: sheet.id, field, value }]);
  }
  function updateChart(chart: SheetChartDefinition) {
    if (editable)
      sheetField(
        'charts',
        charts.map((c) => (c.id === chart.id ? chart : c))
      );
  }
  function chartDialog(type = 'column', existing?: SheetChartDefinition) {
    tool = {
      title: existing ? 'Edit chart' : 'Insert chart',
      submit: existing ? 'Update chart' : 'Insert chart',
      fields: [
        { key: 'title', label: 'Chart title', value: existing?.title || 'Chart', required: true },
        {
          key: 'range',
          label: 'Chart data range',
          value: existing?.range || rangeAddress(range()),
          required: true
        },
        {
          key: 'type',
          label: 'Chart type',
          type: 'select',
          value: existing?.type || type,
          options: ['column', 'bar', 'line', 'pie'].map((value) => ({
            value,
            label: value[0].toUpperCase() + value.slice(1)
          }))
        }
      ],
      note: 'The first row supplies series names; the first column supplies categories.',
      onSubmit: (v) => {
        if (!sheet) return;
        const x = checkRange(documentState, sheet, v.range);
        if (x.r2 <= x.r1 || x.c2 <= x.c1)
          throw new Error('Select a header row, category column and numeric values.');
        if (x.r2 - x.r1 > 200 || x.c2 - x.c1 > 8)
          throw new Error('Charts support up to 200 categories and 8 series.');
        if (!existing && charts.length >= 12) throw new Error('Use up to 12 charts per sheet.');
        const chart: SheetChartDefinition = {
          id: existing?.id || ulid(),
          title: v.title,
          type: v.type as SheetChartDefinition['type'],
          range: rangeAddress(x),
          x: existing?.x ?? 200,
          y: existing?.y ?? 60,
          width: existing?.width ?? 560,
          height: existing?.height ?? 320
        };
        sheetField(
          'charts',
          existing ? charts.map((c) => (c.id === existing.id ? chart : c)) : [...charts, chart]
        );
      }
    };
  }
  function refreshPivot() {
    if (!sheet || !editable) return;
    const def = sheet.current_fields.pivot_source as PivotDefinition | undefined;
    const source = def && documentState.nodes[def.sheetId];
    if (!def || !source || source.deleted) throw new Error('The source worksheet is unavailable.');
    const result = pivotOps(
      documentState,
      def,
      computed[String(source.current_fields.label || source.id)] || [],
      sheet
    );
    onChange(result.ops);
  }
  function pivotDialog() {
    const x = range();
    const options = Array.from({ length: columns }, (_, i) => ({
      value: String(i),
      label: `${col(i)} · ${raw(x.r1, i) || 'Column ' + col(i)}`
    }));
    tool = {
      title: 'Create PivotTable',
      submit: 'Create PivotTable',
      fields: [
        { key: 'range', label: 'Pivot source range', value: rangeAddress(x), required: true },
        { key: 'row', label: 'Rows field', type: 'select', value: String(x.c1), options },
        {
          key: 'column',
          label: 'Columns field',
          type: 'select',
          value: '',
          options: [{ value: '', label: 'None' }, ...options]
        },
        {
          key: 'value',
          label: 'Values field',
          type: 'select',
          value: String(Math.min(x.c1 + 1, columns - 1)),
          options
        },
        {
          key: 'aggregate',
          label: 'Summarize values by',
          type: 'select',
          value: 'sum',
          options: ['sum', 'count', 'average', 'min', 'max'].map((value) => ({
            value,
            label: value.toUpperCase()
          }))
        }
      ],
      note: 'Creates a worksheet from the source range. Use Data → Refresh PivotTable after source changes.',
      onSubmit: (v) => {
        if (!sheet) return;
        checkRange(documentState, sheet, v.range);
        const def: PivotDefinition = {
          sheetId: sheet.id,
          range: v.range,
          row: Number(v.row),
          column: v.column === '' ? null : Number(v.column),
          value: Number(v.value),
          aggregate: v.aggregate as PivotDefinition['aggregate']
        };
        const result = pivotOps(
          documentState,
          def,
          computed[String(sheet.current_fields.label || sheet.id)] || []
        );
        onChange(result.ops);
        activeSheet = result.id;
        selected = { r: 0, c: 0 };
        anchor = { r: 0, c: 0 };
      }
    };
  }
  function setStyle(name: string) {
    const styles: Record<string, Partial<CellFormat>> = {
      normal: { bold: false, background: '#ffffff', color: '#202124', border: false },
      heading: { bold: true, background: '#107c41', color: '#ffffff' },
      good: { background: '#c6efce', color: '#006100' },
      bad: { background: '#ffc7ce', color: '#9c0006' },
      input: { background: '#fff2cc', color: '#202124' },
      total: { bold: true, border: true, background: '#e2f0d9', color: '#202124' }
    };
    format(styles[name] || styles.normal);
  }
  function asTable() {
    if (!sheet || !editable) return;
    const x = range(),
      ops: EventPayload[] = [];
    if ((x.r2 - x.r1 + 1) * (x.c2 - x.c1 + 1) > 4000)
      throw new Error('Format up to 4,000 cells at a time.');
    for (let r = x.r1; r <= x.r2; r++)
      for (let c = x.c1; c <= x.c2; c++)
        if (shown[r])
          ops.push({
            type: 'FieldEdited',
            node_id: shown[r].id,
            field: `format_${c}`,
            value: {
              ...cellFormat(shown[r], c),
              background: r === x.r1 ? '#107c41' : (r - x.r1) % 2 ? '#e2f0d9' : '#ffffff',
              color: r === x.r1 ? '#ffffff' : '#202124',
              bold: r === x.r1
            }
          });
    ops.push({
      type: 'FieldEdited',
      node_id: sheet.id,
      field: 'table_range',
      value: rangeAddress(x)
    });
    onChange(ops);
  }
  function ribbonAction(action: string, argument = '') {
    operation(() => {
      const viewActions = [
        'copy',
        'find',
        'filter',
        'formulas',
        'hidden',
        'gridlines',
        'zoom-in',
        'zoom-out',
        'calculate',
        'print',
        'comment',
        'protect'
      ];
      if (!editable && !viewActions.includes(action)) return;
      if (
        [
          'chart',
          'pivot',
          'names',
          'split',
          'duplicates',
          'table-style',
          'print-area',
          'sort-asc',
          'sort-desc',
          'sort'
        ].includes(action) &&
        (filter || shown.length !== rows.length)
      )
        throw new Error('Show all rows and clear the filter before using this range command.');
      switch (action) {
        case 'paste':
        case 'cut':
        case 'copy':
          void menuClipboard(action);
          break;
        case 'undo':
          onUndo();
          break;
        case 'redo':
          onRedo();
          break;
        case 'fill-down':
          fill('down');
          break;
        case 'fill-right':
          fill('right');
          break;
        case 'merge':
          merge();
          break;
        case 'unmerge':
          merge(true);
          break;
        case 'clear':
          clear();
          break;
        case 'conditional':
          conditionalDialog.showModal();
          break;
        case 'new-sheet':
          addSheet();
          break;
        case 'insert-row':
          insert('row');
          break;
        case 'insert-column':
          insert('column');
          break;
        case 'delete-row':
          resize('row');
          break;
        case 'sort-asc':
          sort(false);
          break;
        case 'sort-desc':
          sort(true);
          break;
        case 'sum':
          autoSum(argument);
          break;
        case 'filter':
          filter = !filter;
          searchOpen = filter;
          break;
        case 'find':
          searchOpen = !searchOpen;
          break;
        case 'formulas':
          showFormulas = !showFormulas;
          break;
        case 'hidden':
          showHidden = !showHidden;
          break;
        case 'gridlines':
          gridlines = !gridlines;
          break;
        case 'zoom-in':
          zoom = Math.min(200, zoom + 10);
          break;
        case 'zoom-out':
          zoom = Math.max(50, zoom - 10);
          break;
        case 'freeze-rows':
          updateLayout({ frozenRows: Number(argument) });
          break;
        case 'freeze-columns':
          updateLayout({ frozenColumns: Number(argument) });
          break;
        case 'print':
          printSheet();
          break;
        case 'comment':
          onComment?.();
          break;
        case 'calculate':
          computed = calculator.calculate(JSON.parse(calculationInput), names);
          break;
        case 'chart':
          chartDialog(argument);
          break;
        case 'pivot':
          pivotDialog();
          break;
        case 'refresh-pivot':
          refreshPivot();
          break;
        case 'format-painter':
          paintFormat = JSON.parse(JSON.stringify(selectedFormat));
          break;
        case 'decimals':
          format({
            decimals: Math.max(0, Math.min(10, (selectedFormat.decimals ?? 2) + Number(argument))),
            number:
              selectedFormat.number === 'auto' ? 'decimal' : selectedFormat.number || 'decimal'
          });
          break;
        case 'table-style':
          asTable();
          break;
        case 'cell-style':
          tool = {
            title: 'Cell Styles',
            fields: [
              {
                key: 'style',
                label: 'Cell style',
                type: 'select',
                value: 'heading',
                options: ['normal', 'heading', 'good', 'bad', 'input', 'total'].map((value) => ({
                  value,
                  label: value[0].toUpperCase() + value.slice(1)
                }))
              }
            ],
            onSubmit: (v) => setStyle(v.style)
          };
          break;
        case 'sort':
          tool = {
            title: 'Sort worksheet',
            fields: [
              {
                key: 'column',
                label: 'Sort by column',
                type: 'select',
                value: String(selected.c),
                options: Array.from({ length: columns }, (_, i) => ({
                  value: String(i),
                  label: col(i)
                }))
              },
              {
                key: 'order',
                label: 'Sort order',
                type: 'select',
                value: 'ascending',
                options: [
                  { value: 'ascending', label: 'Smallest to largest' },
                  { value: 'descending', label: 'Largest to smallest' }
                ]
              },
              { key: 'header', label: 'My data has headers', type: 'checkbox', value: 'true' }
            ],
            onSubmit: (v) => {
              if (sheet)
                onChange(
                  sortSheet(
                    documentState,
                    sheet,
                    Number(v.column),
                    v.order === 'descending',
                    v.header === 'true'
                  )
                );
            }
          };
          break;
        case 'split':
          tool = {
            title: 'Text to Columns',
            fields: [
              {
                key: 'delimiter',
                label: 'Delimiter',
                type: 'select',
                value: ',',
                options: [
                  { value: ',', label: 'Comma' },
                  { value: ';', label: 'Semicolon' },
                  { value: '\t', label: 'Tab' },
                  { value: ' ', label: 'Space' },
                  { value: '|', label: 'Pipe' }
                ]
              }
            ],
            note: 'Splits the first selected column. Destination cells must be empty.',
            onSubmit: (v) => {
              if (!sheet) return;
              if (filter || shown.length !== rows.length)
                throw new Error('Show every row before splitting.');
              onChange(splitColumn(documentState, sheet, range(), v.delimiter));
            }
          };
          break;
        case 'duplicates':
          tool = {
            title: 'Remove Duplicates',
            submit: 'Remove duplicates',
            fields: [
              {
                key: 'range',
                label: 'Compare columns in range',
                value: rangeAddress(range()),
                required: true
              },
              { key: 'header', label: 'My data has headers', type: 'checkbox', value: 'true' }
            ],
            note: 'Removes duplicate whole rows, using the selected columns as the key. Undo restores removed rows.',
            onSubmit: (v) => {
              if (!sheet) return;
              onChange(
                removeDuplicateRows(
                  documentState,
                  sheet,
                  checkRange(documentState, sheet, v.range),
                  v.header === 'true'
                )
              );
            }
          };
          break;
        case 'function':
          tool = {
            title: 'Insert Function',
            fields: [
              { key: 'fn', label: 'Function', value: 'SUM', list: functionNames, required: true },
              { key: 'args', label: 'Arguments', value: rangeAddress(range()) }
            ],
            onSubmit: (v) => {
              const fn = v.fn.trim().toUpperCase();
              if (!functionNames.includes(fn))
                throw new Error('Choose a function supported by the calculation engine.');
              void edit(`=${fn}(${v.args})`);
            }
          };
          break;
        case 'names':
          tool = {
            title: 'Name Manager',
            fields: [
              { key: 'name', label: 'Name', value: 'SelectedRange', required: true },
              {
                key: 'expression',
                label: 'Refers to',
                value: `='${String(sheet?.current_fields.label || '').replaceAll("'", "''")}'!$${col(range().c1)}$${range().r1 + 1}:$${col(range().c2)}$${range().r2 + 1}`,
                required: true
              }
            ],
            note: names.length
              ? names.map((n) => `${n.name}: ${n.expression}`).join('\n')
              : 'Workbook names can be used in formulas.',
            onSubmit: (v) =>
              onChange(namedRangeOps(documentState, v.name.trim(), v.expression.trim())),
            actions: names.map((n) => ({
              label: `Delete ${n.name}`,
              run: () => onChange(namedRangeOps(documentState, n.name, ''))
            }))
          };
          break;
        case 'validation':
          tool = {
            title: 'Data Validation',
            fields: [
              {
                key: 'type',
                label: 'Allow',
                type: 'select',
                value: 'list',
                options: [
                  { value: 'list', label: 'List' },
                  { value: 'whole', label: 'Whole number' },
                  { value: 'decimal', label: 'Decimal' },
                  { value: 'textLength', label: 'Text length' },
                  { value: 'none', label: 'Any value' }
                ]
              },
              { key: 'source', label: 'List values (comma separated)', value: '' },
              { key: 'min', label: 'Minimum', type: 'number', value: '0', step: 'any' },
              { key: 'max', label: 'Maximum', type: 'number', value: '100', step: 'any' },
              { key: 'blank', label: 'Ignore blank', type: 'checkbox', value: 'true' }
            ],
            onSubmit: (v) => {
              if (
                v.type === 'list' &&
                (!v.source || v.source.length > 240 || v.source.includes('"'))
              )
                throw new Error('Enter up to 240 characters without quotation marks.');
              if (v.type !== 'list' && v.type !== 'none' && Number(v.max) < Number(v.min))
                throw new Error('Maximum must not be less than minimum.');
              format({
                validation:
                  v.type === 'none'
                    ? null
                    : {
                        type: v.type,
                        allowBlank: v.blank === 'true',
                        operator: 'between',
                        formulae:
                          v.type === 'list' ? [`"${v.source}"`] : [Number(v.min), Number(v.max)],
                        showErrorMessage: true,
                        error: 'This value does not meet the cell validation rule.'
                      }
              });
            }
          };
          break;
        case 'note':
          tool = {
            title: 'Cell note',
            fields: [
              { key: 'note', label: 'Note', type: 'textarea', value: selectedFormat.note || '' }
            ],
            onSubmit: (v) => format({ note: v.note.slice(0, 5000) })
          };
          break;
        case 'hyperlink':
          tool = {
            title: 'Insert hyperlink',
            fields: [
              {
                key: 'url',
                label: 'Link address',
                value: selectedFormat.hyperlink || 'https://',
                required: true
              }
            ],
            onSubmit: (v) => {
              if (!/^(https?:\/\/|mailto:)/i.test(v.url))
                throw new Error('Use an https, http or mailto link.');
              format({ hyperlink: v.url, color: '#0563c1', underline: true });
            }
          };
          break;
        case 'protect':
          if (canEdit)
            tool = {
              title: protectedSheet ? 'Unprotect sheet' : 'Protect sheet',
              submit: protectedSheet ? 'Unprotect' : 'Protect',
              fields: [],
              note: 'Prevents accidental worksheet edits until an editor unprotects it. Sharing permissions still control access.',
              onSubmit: () => sheetField('sheet_protected', !protectedSheet)
            };
          break;
        case 'dimensions':
          tool = {
            title: 'Cell dimensions',
            fields: [
              {
                key: 'width',
                label: 'Column width',
                type: 'number',
                value: String(layout.widths[String(selected.c)] || 64),
                min: 40,
                max: 500
              },
              {
                key: 'height',
                label: 'Row height',
                type: 'number',
                value: String(shown[selected.r] ? rowHeight(shown[selected.r]) : 20),
                min: 20,
                max: 160
              }
            ],
            onSubmit: (v) => {
              updateLayout({
                widths: { ...layout.widths, [selected.c]: bounded(v.width, 40, 500, 64) }
              });
              sizeRows(Number(v.height));
            },
            actions: [
              {
                label: 'AutoFit column',
                run: () => {
                  const max = Math.max(...rows.map((n) => display(n, selected.c).length));
                  updateLayout({
                    widths: {
                      ...layout.widths,
                      [selected.c]: Math.min(500, Math.max(64, max * 8 + 14))
                    }
                  });
                }
              },
              { label: 'Hide selected rows', run: () => hideRows(true) },
              { label: 'Unhide rows', run: () => hideRows(false) },
              {
                label: 'Hide column',
                run: () => updateLayout({ hiddenColumns: [...layout.hiddenColumns, selected.c] })
              },
              { label: 'Unhide columns', run: () => updateLayout({ hiddenColumns: [] }) }
            ]
          };
          break;
        case 'print-area':
          sheetField('print_area', rangeAddress(range()));
          break;
        case 'clear-print-area':
          sheetField('print_area', null);
          break;
        case 'page-setup':
          tool = {
            title: 'Sheet page setup',
            fields: [
              {
                key: 'size',
                label: 'Paper size',
                type: 'select',
                value: printPage.size,
                options: ['A4', 'Letter', 'Legal'].map((value) => ({ value, label: value }))
              },
              {
                key: 'orientation',
                label: 'Orientation',
                type: 'select',
                value: printPage.landscape ? 'landscape' : 'portrait',
                options: [
                  { value: 'portrait', label: 'Portrait' },
                  { value: 'landscape', label: 'Landscape' }
                ]
              },
              {
                key: 'margin',
                label: 'Margins (mm)',
                type: 'number',
                min: 5,
                max: 50,
                value: String(printPage.margin)
              },
              { key: 'header', label: 'Header', value: printPage.header },
              { key: 'footer', label: 'Footer', value: printPage.footer },
              {
                key: 'numbers',
                label: 'Page numbers',
                type: 'checkbox',
                value: String(printPage.pageNumbers)
              }
            ],
            onSubmit: (v) =>
              sheetField(
                'page_setup',
                pageSetup({
                  ...printPage,
                  size: v.size,
                  landscape: v.orientation === 'landscape',
                  margin: Number(v.margin),
                  header: v.header,
                  footer: v.footer,
                  pageNumbers: v.numbers === 'true'
                })
              )
          };
          break;
      }
    });
  }
</script>

{#if printHtml !== null}<PrintPreview
    html={printHtml}
    page={printPage}
    description="Preview all visible rows in this sheet. Hidden and filtered rows are excluded."
    onClose={() => (printHtml = null)}
  />{/if}

<div class="sheet-editor" role="region" aria-label="Spreadsheet editor">
  <SheetRibbon
    bind:tab={ribbon}
    {editable}
    format={selectedFormat}
    {layout}
    onFormat={format}
    onAction={ribbonAction}
    {filter}
    {showFormulas}
    {gridlines}
    {protectedSheet}
    hasPivot={!!sheet?.current_fields.pivot_source}
  />
  {#if searchOpen}<div class="search-tools">
      <Search size={15} /><input
        aria-label="Find cells"
        placeholder={filter ? 'Filter matching rows' : 'Find in sheet'}
        bind:value={find}
      /><input
        aria-label="Replace cell text with"
        placeholder="Replace with"
        bind:value={replacing}
      /><button disabled={!editable || !find} onclick={replaceCells}>Replace matching text</button
      ><button aria-label="Close find" onclick={() => (searchOpen = false)}>×</button>
    </div>{/if}
  {#if paintFormat}<div class="notice">
      Select a cell to apply the copied formatting.<button onclick={() => (paintFormat = null)}
        >Cancel</button
      >
    </div>{/if}
  {#if smallScreen && !sizeNoticeSeen}<div class="notice size-notice" role="note">
      Spreadsheets are easier to edit on a larger screen. Here you can read, select cells and change
      a value in the formula bar.<button onclick={dismissSizeNotice}>Got it</button>
    </div>{/if}
  {#if protectedSheet}<div class="protection-notice">
      <LockKeyhole size={13} />Protected sheet
      <button disabled={!canEdit} onclick={() => ribbonAction('protect')}>Unprotect</button>
    </div>{/if}
  <div class="formula">
    <input
      class="name-box"
      aria-label="Go to cell or range"
      placeholder={`${col(selected.c)}${(rowIndex.get(shown[selected.r]?.id) || 0) + 1}`}
      bind:value={address}
      onkeydown={(e) => {
        if (e.key === 'Enter' && !e.isComposing) {
          goToAddress();
          address = '';
        }
      }}
    /><button
      class="fx"
      aria-label="Insert function"
      title="Insert function"
      disabled={!editable}
      onclick={() => ribbonAction('function')}>ƒx</button
    ><input
      aria-label="Formula bar"
      value={editing ? value : raw(selected.r, selected.c)}
      disabled={!editable || !shown.length}
      onfocus={() => {
        value = raw(selected.r, selected.c);
        editing = true;
      }}
      oninput={(e) => (value = e.currentTarget.value)}
      onblur={commit}
      onkeydown={(e) => {
        if (e.isComposing || e.keyCode === 229) return;
        if (e.key === 'Enter') {
          commit();
          e.currentTarget.blur();
        }
        if (e.key === 'Escape') {
          editing = false;
          e.currentTarget.blur();
        }
      }}
    />
  </div>
  {#if error}<div class="notice" role="alert">
      {error}<button onclick={() => (error = '')}>Dismiss</button>
    </div>{/if}
  <div
    class="grid"
    class:no-gridlines={!gridlines}
    bind:this={grid}
    onscroll={(e) => (scrollTop = e.currentTarget.scrollTop / (zoom / 100))}
  >
    <div class="sheet-surface" style:zoom={`${zoom}%`}>
      <table
        role="grid"
        tabindex="0"
        aria-label={String(sheet?.current_fields.label || 'Sheet')}
        aria-rowcount={rows.length + 1}
        aria-colcount={columns + 1}
        aria-activedescendant={`cell-${sheet?.id}-${shown[selected.r]?.id}-${selected.c}`}
        onkeydown={key}
        onpaste={paste}
        oncopy={copy}
      >
        <colgroup
          ><col style="width:32px" />{#each visibleColumns as c}<col
              style={`width:${layout.widths[String(c)] || 64}px`}
            />{/each}</colgroup
        >
        <thead
          ><tr
            ><th class="corner"></th>{#each visibleColumns as c}<th
                scope="col"
                style={c < layout.frozenColumns ? `left:${left(c)}px;z-index:6` : ''}
                class:chosen={c === selected.c}>{col(c)}</th
              >{/each}</tr
          ></thead
        ><tbody>
          {#each visible as { n, r }, i (n.id)}{#if i === frozen && renderStart > frozen}<tr
                aria-hidden="true"
                style={`height:${offsets[renderStart] - offsets[frozen]}px`}
                ><td colspan={visibleColumns.length + 1}></td></tr
              >{/if}<tr
              aria-rowindex={(rowIndex.get(n.id) || 0) + 2}
              style={`height:${rowHeight(n)}px`}
            >
              <th
                scope="row"
                class:chosen={r === selected.r}
                style={r < frozen ? `top:${20 + offsets[r]}px;z-index:6` : ''}
                >{(rowIndex.get(n.id) || 0) + 1}</th
              >
              {#each visibleColumns as c}{@const merged = span(r, c)}{#if !merged.hidden}<td
                    role="gridcell"
                    id={`cell-${sheet?.id}-${n.id}-${c}`}
                    aria-colindex={c + 2}
                    aria-selected={inRange(r, c)}
                    rowspan={merged.rows}
                    colspan={merged.columns}
                    class:selected={r === selected.r && c === selected.c}
                    class:range={inRange(r, c)}
                    class:match={find &&
                      !filter &&
                      String(n.current_fields[`cell_${c}`] || '')
                        .toLowerCase()
                        .includes(find.toLowerCase())}
                    style={style(n, c, r)}
                    ><button
                      class="cell"
                      tabindex="-1"
                      style={`height:${rowHeight(n) * merged.rows - 1}px;max-height:${rowHeight(n) * merged.rows - 1}px`}
                      title={cellFormat(n, c).note || ''}
                      aria-label={`${col(c)}${r + 1}: ${display(n, c)}`}
                      onmousedown={(e) => {
                        if (e.button !== 0) return;
                        e.preventDefault();
                        choose(r, c, e.shiftKey);
                        (e.currentTarget.closest('table') as HTMLElement)?.focus();
                      }}
                      oncontextmenu={(e) => openCellMenu(e, r, c)}
                      ondblclick={() => void edit()}>{display(n, c)}</button
                    >{#if editing && r === selected.r && c === selected.c}<input
                        class="cell-entry"
                        aria-label={`Edit ${col(c)}${r + 1}`}
                        bind:value
                        onblur={commit}
                        onkeydown={(e) => {
                          if (e.isComposing || e.keyCode === 229) return;
                          if (e.key === 'Enter' || e.key === 'Tab') {
                            e.preventDefault();
                            commit();
                            choose(r + (e.key === 'Enter' ? 1 : 0), c + (e.key === 'Tab' ? 1 : 0));
                            (e.currentTarget.closest('table') as HTMLElement)?.focus();
                          }
                          if (e.key === 'Escape') {
                            editing = false;
                          }
                        }}
                      />{/if}</td
                  >{/if}{/each}</tr
            >{/each}{#if shown.length > start + 65}<tr
              aria-hidden="true"
              style={`height:${offsets[shown.length] - offsets[start + 65]}px`}
              ><td colspan={columns + 1}></td></tr
            >{/if}</tbody
        >
      </table>
      {#each charts as chart (chart.id)}<SheetChart
          {chart}
          values={computed[String(sheet?.current_fields.label || sheet?.id)] || []}
          {editable}
          onChange={updateChart}
          onEdit={() => chartDialog(chart.type, chart)}
          onRemove={() =>
            sheetField(
              'charts',
              charts.filter((c) => c.id !== chart.id)
            )}
        />{/each}
      <div class="add-rows">
        <button disabled={!editable || rows.length >= 3000} onclick={() => addRows(20)}
          >Add 20 rows</button
        ><button
          disabled={!editable || columns >= 100}
          onclick={() =>
            sheet &&
            onChange([
              { type: 'FieldEdited', node_id: sheet.id, field: 'columns', value: columns + 1 }
            ])}>Add column</button
        ><button disabled={!editable || !shown.length} onclick={removeRow}
          ><Trash2 size={12} /> Delete selected row</button
        ><button disabled={!editable || columns <= 1} onclick={() => resize('column')}
          >Delete selected column</button
        >
      </div>
    </div>
  </div>
  <div class="sheet-tabs">
    <button aria-label="Add sheet" disabled={!editable} onclick={addSheet}
      ><Plus size={18} /></button
    >{#each sheets as s}<button
        oncontextmenu={(e) => openSheetMenu(e, s.id)}
        ondblclick={() => {
          activeSheet = s.id;
          renameSheet();
        }}
        class:active={sheet?.id === s.id}
        onclick={() => {
          if (editing) commit();
          activeSheet = s.id;
          selected = { r: 0, c: 0 };
          anchor = { r: 0, c: 0 };
        }}>{String(s.current_fields.label)}</button
      >{/each}<select
      aria-label="Sheet actions"
      disabled={!editable}
      onchange={(e) => {
        switch (e.currentTarget.value) {
          case 'rename':
            renameSheet();
            break;
          case 'duplicate':
            duplicateSheet();
            break;
          case 'delete':
            deleteSheet();
            break;
          case 'left':
            moveSheet(-1);
            break;
          case 'right':
            moveSheet(1);
            break;
        }
        e.currentTarget.value = '';
      }}
      ><option value="">Sheet actions</option><option value="rename">Rename</option><option
        value="duplicate">Duplicate</option
      ><option value="left">Move left</option><option value="right">Move right</option><option
        value="delete">Delete</option
      ></select
    >
  </div>
  <div class="sheet-status" role="status" aria-label="Workbook status">
    <span>{editing ? 'Enter' : protectedSheet ? 'Protected' : 'Ready'}</span><span
      class="selection-summary">{summary}</span
    ><button aria-label="Zoom out" onclick={() => ribbonAction('zoom-out')}>−</button><input
      aria-label="Sheet zoom"
      type="range"
      min="50"
      max="200"
      step="10"
      bind:value={zoom}
    /><button aria-label="Zoom in" onclick={() => ribbonAction('zoom-in')}>+</button><span
      >{zoom}%</span
    >
  </div>
</div>

{#if tool}<OfficeDialog config={tool} onClose={() => (tool = null)} />{/if}
{#if context}<ContextMenu
    {...context}
    onClose={() => {
      context = null;
    }}
  />{/if}

<dialog bind:this={conditionalDialog} aria-labelledby="conditional-title">
  <form
    onsubmit={(e) => {
      e.preventDefault();
      applyConditional();
    }}
  >
    <h2 id="conditional-title">Conditional formatting</h2>
    <label
      >Format cells if<select aria-label="Condition" bind:value={condition}
        ><option value="greaterThan">Greater than</option><option value="lessThan">Less than</option
        ><option value="equal">Equal to</option><option value="between">Between</option></select
      ></label
    >
    <label
      >Value<input
        aria-label="Condition value"
        type="number"
        step="any"
        required
        bind:value={threshold}
      /></label
    >
    {#if condition === 'between'}<label
        >Upper value<input
          aria-label="Upper condition value"
          type="number"
          min={threshold}
          step="any"
          required
          bind:value={upper}
        /></label
      >{/if}
    <label
      >Fill<input
        aria-label="Conditional fill color"
        type="color"
        bind:value={conditionalFill}
      /></label
    >
    <label
      >Text<input
        aria-label="Conditional text color"
        type="color"
        bind:value={conditionalColor}
      /></label
    >
    <div>
      <button type="button" onclick={() => conditionalDialog.close()}>Cancel</button><button
        type="button"
        onclick={() => {
          format({ conditional: [] });
          conditionalDialog.close();
        }}>Clear rules</button
      ><button type="submit">Apply rule</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    border: 1px solid var(--color-border-strong);
    border-radius: 6px;
    padding: 24px;
    color: var(--color-ink);
    background: var(--color-surface-raised);
    width: 360px;
    max-width: calc(100vw - 32px);
  }
  dialog::backdrop {
    background: var(--color-overlay);
  }
  dialog form {
    display: grid;
    gap: 12px;
  }
  dialog h2 {
    margin: 0 0 10px;
    font-size: 18px;
  }
  dialog label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }
  dialog input,
  dialog select,
  dialog button {
    padding: 6px;
    border: 1px solid var(--color-border-strong);
    background: var(--color-surface-raised);
    border-radius: 3px;
    max-width: 170px;
  }
  dialog form > div {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }
  dialog button {
    cursor: pointer;
  }
  .formula input.name-box {
    width: 110px;
    flex: 0 0 110px;
    border-right: 1px solid var(--color-border);
    padding: 7px;
    color: var(--color-ink);
  }
  .grid td {
    min-width: 0 !important;
    width: auto !important;
    max-width: none !important;
  }
  .grid th {
    min-width: 0 !important;
  }
  .grid .cell {
    line-height: 1.25;
    overflow: hidden;
  }
  .grid table:focus-visible {
    outline: 2px solid var(--color-excel);
    outline-offset: -2px;
  }
  .sheet-tabs select {
    padding: 7px;
    border: 1px solid var(--color-success-border);
    background: var(--color-success-bg);
    color: var(--color-success);
    font-size: 13px;
    border-radius: 4px;
  }

  .sheet-editor {
    font:
      12px Arial,
      sans-serif;
    background: var(--color-grid-bg);
    flex: 1;
    min-width: 0;
    outline: none;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    color: var(--color-ink);
    /* Excel-green selection over the neutral grid tokens. */
    --sheet-selection: color-mix(in srgb, var(--color-excel) 9%, transparent);
    --sheet-accent-ink: #0b5c30;
    --sheet-header-active: color-mix(
      in srgb,
      var(--color-grid-header-ink) 14%,
      var(--color-grid-header-bg)
    );
    --sheet-header-line: color-mix(
      in srgb,
      var(--color-grid-header-ink) 22%,
      var(--color-grid-header-bg)
    );
  }
  @media screen {
    :global([data-theme='dark']) .sheet-editor {
      --sheet-selection: color-mix(in srgb, var(--color-excel) 18%, transparent);
      --sheet-accent-ink: var(--color-excel);
    }
  }
  .formula {
    display: flex;
    align-items: center;
    height: 33px;
    border-bottom: 1px solid var(--color-border);
    margin-top: 6px;
  }
  .formula .fx {
    font: italic 17px Georgia;
    color: var(--color-ink-soft);
    padding: 0 15px;
  }
  .formula input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: var(--color-surface-raised);
    font: 12px Arial;
    color: var(--color-ink);
  }
  .grid {
    flex: 1;
    overflow: auto;
    max-height: calc(100vh - 215px);
    max-height: calc(100dvh - 215px);
    min-height: 300px;
  }
  .grid table {
    border-collapse: separate;
    border-spacing: 0;
    table-layout: fixed;
    font-size: 13px;
  }
  .grid th {
    position: sticky;
    background: var(--color-grid-header-bg);
    color: var(--color-grid-header-ink);
    font-weight: 400;
    height: 28px;
    border-right: 1px solid var(--color-grid-line);
    border-bottom: 1px solid var(--color-grid-line);
    padding: 0 8px;
    min-width: 118px;
  }
  .grid thead th {
    top: 0;
    z-index: 3;
  }
  .grid tbody th,
  .grid .corner {
    left: 0;
    min-width: 46px;
    width: 46px;
    text-align: center;
    z-index: 2;
    background: var(--color-grid-header-bg);
  }
  .grid .corner {
    z-index: 4;
  }
  .grid th.chosen {
    background: var(--sheet-header-active);
    color: var(--sheet-accent-ink);
  }
  .grid td {
    height: 28px;
    min-width: 118px;
    width: 118px;
    max-width: 118px;
    border-right: 1px solid var(--color-grid-line);
    border-bottom: 1px solid var(--color-grid-line);
    position: relative;
    padding: 0;
    line-height: 1.25;
  }
  .cell {
    display: block;
    background: none;
    border: 0;
    padding: 5px 7px;
    width: 100%;
    min-height: 27px;
    color: inherit;
    font: inherit;
    text-align: inherit;
    white-space: inherit;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: cell;
  }
  .grid td.range:after {
    content: '';
    position: absolute;
    inset: 0;
    background: var(--sheet-selection);
    pointer-events: none;
  }
  .grid td.selected {
    outline: 2px solid var(--color-excel);
    outline-offset: -2px;
    z-index: 1;
  }
  .grid td.match {
    box-shadow: inset 0 0 0 2px var(--color-warn);
  }
  .cell-entry {
    position: absolute;
    inset: 0;
    width: 100%;
    border: 2px solid var(--color-excel);
    outline: none;
    padding: 3px 5px;
    font: 12px Arial;
    z-index: 4;
    color: var(--color-grid-ink);
    background: var(--color-grid-bg);
  }
  .add-rows {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 22px 46px;
  }
  .add-rows button {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 13px;
    color: var(--color-ink-soft);
    background: var(--color-surface-sunken);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 7px 12px;
    cursor: pointer;
    white-space: nowrap;
  }
  .sheet-tabs {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 7px 20px;
    background: var(--color-chrome);
    border-top: 1px solid var(--color-border);
    min-height: 44px;
    overflow: auto;
  }
  .sheet-tabs button {
    border: 0;
    border-right: 1px solid var(--color-border);
    background: transparent;
    cursor: pointer;
    display: block;
    white-space: nowrap;
    font-size: 13px;
    padding: 4px 14px;
  }
  .sheet-tabs button.active {
    background: var(--color-grid-bg);
    color: var(--color-excel);
  }

  .notice {
    padding: 9px 20px;
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
    font-size: 13px;
  }
  .size-notice {
    background: var(--color-accent-bg);
    color: var(--color-ink);
  }
  .notice button {
    float: right;
    border: 0;
    background: none;
    color: inherit;
    text-decoration: underline;
  }
  @media print {
    .formula,
    .sheet-tabs,
    .add-rows {
      display: none;
    }
    .grid {
      max-height: none;
      overflow: visible;
    }
  }

  .sheet-surface {
    position: relative;
    min-width: 100%;
    min-height: 100%;
  }
  .sheet-editor {
    font-family: 'Segoe UI', Arial, sans-serif;
    background: var(--color-chrome);
    color: var(--color-ink-strong);
  }
  .formula {
    margin: 0 7px 5px;
    height: 30px;
    border: 1px solid var(--color-border);
    border-radius: 3px;
    background: var(--color-surface-raised);
    flex-shrink: 0;
  }
  .formula .fx {
    border: 0;
    background: var(--color-surface-raised);
    padding: 0 13px;
    cursor: pointer;
  }
  .formula input.name-box {
    flex-basis: 95px;
    width: 95px;
    color: var(--color-ink-strong);
  }
  .grid {
    max-height: none;
    min-height: 100px;
    background: var(--color-grid-bg);
  }
  .grid th {
    height: 20px;
    min-width: 64px;
    padding: 0 3px;
    background: var(--color-grid-header-bg);
    color: var(--color-grid-header-ink);
    font-size: 11px;
    border-color: var(--sheet-header-line);
  }
  .grid tbody th,
  .grid .corner {
    width: 32px;
    min-width: 32px;
    background: var(--color-grid-header-bg);
  }
  .grid th.chosen {
    background: var(--sheet-header-active);
    color: var(--sheet-accent-ink);
  }
  .grid td {
    height: 20px;
    min-width: 64px;
    width: 64px;
    max-width: 64px;
    border-color: var(--color-grid-line);
  }
  .cell {
    min-height: 19px;
    padding: 0 4px;
    font-family: Arial, sans-serif;
    line-height: 1.1;
  }
  .no-gridlines td {
    border-color: transparent;
  }
  .sheet-tabs {
    min-height: 30px;
    padding: 0 12px;
    gap: 3px;
    background: var(--color-chrome);
  }
  .sheet-tabs button {
    height: 29px;
    border-radius: 0;
    padding: 4px 12px;
    color: var(--color-ink-strong);
    font-size: 12px;
  }
  .sheet-tabs button.active {
    background: var(--color-grid-bg);
    border-bottom: 2px solid var(--color-excel);
    font-weight: 600;
    color: var(--color-excel);
  }
  .sheet-tabs select {
    margin-left: auto;
    padding: 3px;
    font-size: 11px;
    max-width: 110px;
    background: transparent;
    border-color: transparent;
  }
  .add-rows {
    padding: 12px 32px;
    gap: 8px;
  }
  .add-rows button {
    font-size: 11px;
    padding: 4px 8px;
    background: var(--color-surface-raised);
    color: var(--color-ink-soft);
  }
  .sheet-status {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 24px;
    padding: 0 10px;
    background: var(--color-chrome);
    border-top: 1px solid var(--color-border);
    font-size: 11px;
    flex-shrink: 0;
  }
  .selection-summary {
    margin-left: auto;
  }
  .sheet-status button {
    border: 0;
    background: none;
    color: var(--color-ink);
    cursor: pointer;
  }
  .sheet-status input {
    width: 100px;
    height: 16px;
    accent-color: var(--color-muted);
  }
  .search-tools {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 12px;
    background: var(--color-chrome);
    flex-wrap: wrap;
  }
  .search-tools input,
  .search-tools button {
    border: 1px solid var(--color-border-strong);
    padding: 5px;
    font:
      12px 'Segoe UI',
      Arial,
      sans-serif;
    background: var(--color-surface-raised);
    color: var(--color-ink);
    border-radius: 3px;
  }
  .protection-notice {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    font-size: 12px;
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
  }
  .protection-notice button {
    margin-left: auto;
    border: 0;
    background: none;
    text-decoration: underline;
    cursor: pointer;
  }
</style>
