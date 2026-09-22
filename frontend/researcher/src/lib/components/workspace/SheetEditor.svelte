<script lang="ts">
  import PrintPreview from './PrintPreview.svelte';
  import RibbonButton from './RibbonButton.svelte';
  import { defaultPage } from '$lib/workspace/office';
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
    type SheetLayout
  } from '$lib/workspace/sheet-model';
  import {
    fillRange,
    mergeRange,
    parseAddress,
    rangeAddress
  } from '$lib/workspace/sheet-operations';
  import {
    bounded,
    safeColor,
    safeFont,
    officeFonts,
    scriptFontStack
  } from '$lib/workspace/office';
  import '$lib/workspace/fonts.css';
  import { stableJson } from '$lib/workspace/equality';
  import { ulid } from 'ulid';
  import { tick } from 'svelte';
  import {
    Plus,
    Undo2,
    Redo2,
    Bold,
    Italic,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Filter,
    ArrowDownAZ,
    ArrowUpAZ,
    Search,
    Trash2,
    Printer,
    Underline,
    Strikethrough,
    Grid2X2,
    Merge,
    Split,
    ArrowDownToLine,
    ArrowRightToLine,
    PaintBucket
  } from '@lucide/svelte';
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
    editable,
    onChange,
    onSelect,
    onUndo,
    onRedo,
    onComment,
    onEditingChange
  }: Props = $props();
  let printHtml = $state<string | null>(null);
  let grid: HTMLDivElement;
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
  onMount(() => {
    window.addEventListener('dynodoc-print', printSheet);
    return () => {
      calculator.destroy();
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
  const rowHeight = (n: MaterializedNode) => bounded(n.current_fields.row_height, 28, 160, 28);
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
      computed = calculator.calculate(JSON.parse(calculationInput));
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
      46 +
      visibleColumns
        .filter((v) => v < c)
        .reduce((sum, v) => sum + (layout.widths[String(v)] || 118), 0)
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
      onChange(mergeRange(documentState, sheet, selectedRange, split));
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
    const x = parseAddress(address);
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
  function validation() {
    const list = prompt('Allowed values, separated by commas. Leave empty to remove the rule.');
    if (list === null) return;
    if (list.length > 240 || list.includes('"')) {
      error = 'Use up to 240 characters without quotation marks.';
      return;
    }
    format({
      validation: list
        ? {
            type: 'list',
            allowBlank: true,
            formulae: [`"${list}"`],
            showErrorMessage: true,
            error: 'Choose a value from the allowed list.'
          }
        : null
    });
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
        value: bounded(height, 28, 160, 28)
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
      if (format.number === 'percent' || (!format.number && format.numFmt?.includes('%')))
        return (result * 100).toFixed(1) + '%';
      if (format.number === 'integer') return result.toFixed(0);
      if (format.number === 'scientific') return result.toExponential(2);
      if (format.number === 'thousands')
        return result.toLocaleString('en-US', {
          minimumFractionDigits: 2,
          maximumFractionDigits: 2
        });
      if (format?.number === 'decimal') return result.toFixed(2);
      if (format?.number === 'currency')
        return result.toLocaleString('en-US', { style: 'currency', currency: 'USD' });
    }
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
    return `font-family:${safeFont(f.fontFamily)},${scriptFontStack};font-size:${bounded(f.fontSize, 6, 96, 11)}pt;font-weight:${f.bold ? '700' : '400'};font-style:${f.italic ? 'italic' : 'normal'};text-decoration:${[f.underline ? 'underline' : '', f.strike ? 'line-through' : ''].join(' ')};text-align:${['left', 'center', 'right'].includes(f.align || '') ? f.align : 'left'};vertical-align:${['top', 'middle', 'bottom'].includes(f.vertical || '') ? f.vertical : 'middle'};background:${safeColor(f.background, 'white')};color:${safeColor(f.color, '#303b47')};white-space:${f.wrap ? 'normal' : 'nowrap'};${f.border ? 'border:1px solid #718d7e;' : ''}${sticky ? `position:sticky;z-index:${r < frozen && c < layout.frozenColumns ? 5 : 4};${r < frozen ? `top:${28 + offsets[r]}px;` : ''}${c < layout.frozenColumns ? `left:${left(c)}px;` : ''}` : ''}`;
  }
  function printSheet() {
    // Generate all visible rows from state, never from the virtualized screen DOM.
    const table = document.createElement('table');
    for (let r = 0; r < shown.length; r++) {
      const tr = table.insertRow();
      for (const c of visibleColumns) {
        const merged = span(r, c);
        if (merged.hidden) continue;
        const td = tr.insertCell();
        td.textContent = display(shown[r], c);
        td.rowSpan = merged.rows;
        td.colSpan = merged.columns;
        td.setAttribute('style', style(shown[r], c, r, true) + ';white-space:pre-wrap');
      }
    }
    printHtml = table.outerHTML;
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
    value = raw(selected.r, selected.c);
    onSelect?.(shown[selected.r]?.id || null);
    if (grid) {
      const top = offsets[selected.r] || 0;
      if (top < grid.scrollTop) grid.scrollTop = top;
      else if (top + 56 > grid.scrollTop + grid.clientHeight)
        grid.scrollTop = top + 56 - grid.clientHeight;
      scrollTop = grid.scrollTop;
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
    if ((e.ctrlKey || e.metaKey) && e.key === 'z') {
      e.preventDefault();
      if (e.shiftKey) onRedo();
      else onUndo();
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
        fields: { label: name, kind: 'sheet', columns: 12 }
      },
      ...Array.from(
        { length: 30 },
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
    if (!sheet) return;
    if (layout.merges.length) {
      error = 'Unmerge cells before sorting this sheet.';
      return;
    }
    if (
      Object.values(documentState.nodes).some((n) =>
        Object.values(n.current_fields).some((v) => typeof v === 'string' && v.startsWith('='))
      )
    ) {
      error = 'Sorting formula rows is not supported yet. Make a values-only copy first.';
      return;
    }
    const ordered = [...rows].sort(
      (a, b) =>
        String(a.current_fields[`cell_${selected.c}`] || '').localeCompare(
          String(b.current_fields[`cell_${selected.c}`] || ''),
          undefined,
          { numeric: true }
        ) * (desc ? -1 : 1)
    );
    onChange(
      ordered.map((n, i) => ({
        type: 'NodeMoved',
        node_id: n.id,
        new_parent_id: sheet.id,
        new_pos: String(i).padStart(8, '0')
      }))
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
      error = 'Your browser blocked clipboard access. Use Ctrl+C, Ctrl+X or Ctrl+V in the sheet.';
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
</script>

{#if printHtml !== null}<PrintPreview
    html={printHtml}
    page={{
      ...defaultPage,
      landscape: true,
      margin: 15,
      header: String(sheet?.current_fields.label || '')
    }}
    description="Preview all visible rows in this sheet. Hidden and filtered rows are excluded."
    onClose={() => (printHtml = null)}
  />{/if}

<div class="sheet-editor" role="region" aria-label="Spreadsheet editor">
  <nav class="sheet-ribbon" aria-label="Spreadsheet ribbon">
    {#each ['Home', 'Insert', 'Data', 'View'] as tab}<button
        aria-pressed={ribbon === tab}
        onclick={() => (ribbon = tab)}>{tab}</button
      >{/each}
  </nav>
  {#if ribbon === 'Home'}<div class="extra-tools">
      <label
        >Font<select
          aria-label="Cell font"
          disabled={!editable}
          onchange={(e) => format({ fontFamily: e.currentTarget.value })}
          >{#each officeFonts as font}<option>{font}</option>{/each}</select
        ></label
      >
      <label
        >Size<input
          aria-label="Cell font size"
          type="number"
          min="6"
          max="96"
          value="11"
          disabled={!editable}
          onchange={(e) => format({ fontSize: bounded(e.currentTarget.value, 6, 96, 11) })}
        /></label
      >
      <RibbonButton
        label="Underline"
        icon={Underline}
        disabled={!editable}
        onclick={() => toggleFormat('underline')}
      />
      <RibbonButton
        label="Strikethrough"
        icon={Strikethrough}
        disabled={!editable}
        onclick={() => toggleFormat('strike')}
      />
      <RibbonButton
        label="Borders"
        icon={Grid2X2}
        disabled={!editable}
        onclick={() => toggleFormat('border')}
      />
      <RibbonButton label="Merge cells" icon={Merge} disabled={!editable} onclick={() => merge()} />
      <RibbonButton label="Unmerge" icon={Split} disabled={!editable} onclick={() => merge(true)} />
      <label
        >Vertical<select
          aria-label="Cell vertical alignment"
          disabled={!editable}
          onchange={(e) => format({ vertical: e.currentTarget.value })}
          ><option value="middle">Middle</option><option value="top">Top</option><option
            value="bottom">Bottom</option
          ></select
        ></label
      >
      <RibbonButton
        label="Fill down"
        icon={ArrowDownToLine}
        disabled={!editable}
        onclick={() => fill('down')}
      />
      <RibbonButton
        label="Fill right"
        icon={ArrowRightToLine}
        disabled={!editable}
        onclick={() => fill('right')}
      />
      <RibbonButton
        label="Conditional formatting"
        icon={PaintBucket}
        large
        disabled={!editable}
        onclick={() => conditionalDialog.showModal()}
      />
    </div>{:else if ribbon === 'Insert'}<div class="extra-tools">
      <button disabled={!editable} onclick={() => insert('row')}>Row above</button><button
        disabled={!editable}
        onclick={() => insert('row', true)}>Row below</button
      ><button disabled={!editable} onclick={() => insert('column')}>Column left</button><button
        disabled={!editable}
        onclick={() => insert('column', true)}>Column right</button
      ><button disabled={!editable} onclick={addSheet}>New sheet</button>{#if onComment}<button
          onclick={onComment}>Comment</button
        >{/if}<label
        >Summary<select
          aria-label="Insert summary formula"
          disabled={!editable}
          onchange={(e) => {
            autoSum(e.currentTarget.value);
            e.currentTarget.value = '';
          }}
          ><option value="">Choose function</option
          >{#each ['SUM', 'AVERAGE', 'COUNT', 'MIN', 'MAX'] as fn}<option>{fn}</option
            >{/each}</select
        ></label
      >
    </div>
  {:else if ribbon === 'Data'}<div class="extra-tools">
      <button disabled={!editable} onclick={validation}>Data validation</button><label
        >Replace with<input aria-label="Replace cell text with" bind:value={replacing} /></label
      ><button disabled={!editable || !find} onclick={replaceCells}>Replace matching text</button
      ><span>Uses Find below; formulas are excluded.</span>
    </div>
  {:else}<div class="extra-tools">
      <label
        >Freeze rows<select
          aria-label="Freeze rows"
          value={layout.frozenRows}
          disabled={!editable}
          onchange={(e) => updateLayout({ frozenRows: Number(e.currentTarget.value) })}
          >{#each [0, 1, 2, 3, 5, 10] as n}<option value={n}>{n === 0 ? 'None' : n}</option
            >{/each}</select
        ></label
      ><label
        >Freeze columns<select
          aria-label="Freeze columns"
          value={layout.frozenColumns}
          disabled={!editable}
          onchange={(e) => updateLayout({ frozenColumns: Number(e.currentTarget.value) })}
          >{#each [0, 1, 2, 3, 5] as n}<option value={n}>{n === 0 ? 'None' : n}</option
            >{/each}</select
        ></label
      >
      <label
        >Column width<input
          aria-label="Column width"
          type="number"
          min="40"
          max="500"
          value={layout.widths[String(selected.c)] || 118}
          disabled={!editable}
          onchange={(e) =>
            updateLayout({
              widths: {
                ...layout.widths,
                [selected.c]: bounded(e.currentTarget.value, 40, 500, 118)
              }
            })}
        /></label
      ><label
        >Row height<input
          aria-label="Row height"
          type="number"
          min="28"
          max="160"
          value={shown[selected.r] ? rowHeight(shown[selected.r]) : 28}
          disabled={!editable}
          onchange={(e) => sizeRows(Number(e.currentTarget.value))}
        /></label
      >
      <button disabled={!editable} onclick={() => hideRows(true)}>Hide rows</button><button
        disabled={!editable}
        onclick={() => hideRows(false)}>Unhide rows</button
      ><button
        disabled={!editable}
        onclick={() => updateLayout({ hiddenColumns: [...layout.hiddenColumns, selected.c] })}
        >Hide column</button
      ><button disabled={!editable} onclick={() => updateLayout({ hiddenColumns: [] })}
        >Unhide columns</button
      ><button aria-pressed={showHidden} onclick={() => (showHidden = !showHidden)}
        >Show hidden data</button
      ><button aria-pressed={showFormulas} onclick={() => (showFormulas = !showFormulas)}
        >Show formulas</button
      >
    </div>{/if}
  <div class="tools">
    <button aria-label="Undo" onclick={onUndo} disabled={!editable}><Undo2 size={16} /></button
    ><button aria-label="Redo" onclick={onRedo} disabled={!editable}><Redo2 size={16} /></button
    ><button aria-label="Print spreadsheet" onclick={printSheet}><Printer size={16} /></button><i
    ></i><select
      aria-label="Number format"
      disabled={!editable}
      onchange={(e) => format({ number: e.currentTarget.value })}
      ><option value="auto">Automatic</option><option value="decimal">Number · 0.00</option><option
        value="percent">Percent · %</option
      ><option value="currency">Currency · USD</option><option value="integer">Integer</option
      ><option value="thousands">Thousands</option><option value="scientific">Scientific</option
      ></select
    ><i></i><button
      aria-label="Bold cells"
      disabled={!editable}
      onclick={() => toggleFormat('bold')}><Bold size={16} /></button
    ><button aria-label="Italic cells" disabled={!editable} onclick={() => toggleFormat('italic')}
      ><Italic size={16} /></button
    ><label title="Text color"
      >A<input
        type="color"
        aria-label="Cell text color"
        disabled={!editable}
        onchange={(e) => format({ color: e.currentTarget.value })}
      /></label
    ><label title="Fill color" class="fill"
      >▧<input
        type="color"
        aria-label="Cell fill color"
        disabled={!editable}
        value="#ffffff"
        onchange={(e) => format({ background: e.currentTarget.value })}
      /></label
    ><i></i><button
      aria-label="Align cells left"
      disabled={!editable}
      onclick={() => format({ align: 'left' })}><AlignLeft size={16} /></button
    ><button
      aria-label="Align cells center"
      disabled={!editable}
      onclick={() => format({ align: 'center' })}><AlignCenter size={16} /></button
    ><button
      aria-label="Align cells right"
      disabled={!editable}
      onclick={() => format({ align: 'right' })}><AlignRight size={16} /></button
    ><button title="Wrap text" disabled={!editable} onclick={() => toggleFormat('wrap')}>↵</button
    ><i></i><button aria-label="Sort ascending" disabled={!editable} onclick={() => sort(false)}
      ><ArrowDownAZ size={16} /></button
    ><button aria-label="Sort descending" disabled={!editable} onclick={() => sort(true)}
      ><ArrowUpAZ size={16} /></button
    ><button aria-label="Filter rows" aria-pressed={filter} onclick={() => (filter = !filter)}
      ><Filter size={16} /></button
    >
    <div class="search">
      <Search size={14} /><input
        aria-label="Find cells"
        placeholder={filter ? 'Filter matching rows' : 'Find in sheet'}
        bind:value={find}
      />
    </div>
  </div>
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
    /><b>ƒx</b><input
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
  <div class="grid" bind:this={grid} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}>
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
        ><col style="width:46px" />{#each visibleColumns as c}<col
            style={`width:${layout.widths[String(c)] || 118}px`}
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
              style={r < frozen ? `top:${28 + offsets[r]}px;z-index:6` : ''}
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
    ><span aria-live="polite">{summary} · {shown.length} rows · {columns} columns</span>
  </div>
</div>

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
    border: 1px solid #ccc;
    border-radius: 6px;
    padding: 24px;
    color: #333;
    width: 360px;
    max-width: calc(100vw - 32px);
  }
  dialog::backdrop {
    background: #0004;
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
    border: 1px solid #ccc;
    background: white;
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
  .sheet-ribbon {
    display: flex;
    padding: 0 14px;
    background: #edf6f0;
    border-bottom: 1px solid #c5dbcd;
  }
  .sheet-ribbon button {
    padding: 10px 18px;
    border: 0;
    border-bottom: 3px solid transparent;
    background: none;
    color: #255841;
    font: 14px Arial;
    cursor: pointer;
  }
  .sheet-ribbon button[aria-pressed='true'] {
    border-bottom-color: #107c41;
    font-weight: 600;
  }
  .extra-tools {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 8px;
    padding: 10px 16px;
    background: #f3f8f4;
    color: #255841;
    font: 13px Arial;
  }
  .extra-tools label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
  }
  .extra-tools input,
  .extra-tools select,
  .extra-tools button {
    border: 1px solid #aac9b7;
    background: white;
    border-radius: 4px;
    padding: 7px;
    min-height: 34px;
    font: inherit;
    color: inherit;
  }
  .extra-tools input {
    max-width: 150px;
  }
  .extra-tools input[type='number'] {
    max-width: 75px;
  }
  .extra-tools button {
    cursor: pointer;
  }
  .extra-tools button:disabled,
  .extra-tools select:disabled,
  .extra-tools input:disabled {
    opacity: 0.5;
  }
  .formula input.name-box {
    width: 110px;
    flex: 0 0 110px;
    border-right: 1px solid #d2dfd7;
    padding: 7px;
    color: #345c46;
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
    outline: 2px solid #256a44;
    outline-offset: -2px;
  }
  @media print {
    .sheet-ribbon,
    .extra-tools {
      display: none;
    }
  }

  .sheet-tabs select {
    padding: 7px;
    border: 1px solid #b6d0c1;
    background: #eff7f1;
    color: #255841;
    font-size: 13px;
    border-radius: 4px;
  }

  .sheet-editor {
    font:
      12px Arial,
      sans-serif;
    background: white;
    flex: 1;
    min-width: 0;
    outline: none;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    color: #354455;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 7px 14px;
    background: #edf6f0;
    border-radius: 8px;
    margin: 0 12px;
    flex-wrap: wrap;
  }
  .tools button,
  .sheet-tabs button {
    display: grid;
    place-items: center;
    min-width: 28px;
    height: 28px;
    border: 0;
    background: none;
    color: #5c7064;
    border-radius: 4px;
    cursor: pointer;
  }
  .tools button:hover,
  .tools button[aria-pressed='true'] {
    background: #d7eddf;
  }
  .tools button:disabled {
    opacity: 0.4;
  }
  .tools i {
    height: 19px;
    width: 1px;
    background: #d3e0d8;
    margin: 0 7px;
  }
  .tools select {
    border: 0;
    background: none;
    color: #5a6f62;
    font-size: 13px;
    max-width: 130px;
  }
  .tools label {
    position: relative;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-bottom: 3px solid #345d48;
  }
  .tools label input {
    position: absolute;
    inset: 0;
    width: 100%;
    opacity: 0;
    cursor: pointer;
  }
  .tools .fill {
    border-color: #e6be64;
  }
  .search {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 6px;
    color: #46617a;
  }
  .search input {
    background: none;
    border: 0;
    font-size: 13px;
    outline: 0;
    max-width: 130px;
  }
  .formula {
    display: flex;
    align-items: center;
    height: 33px;
    border-bottom: 1px solid #d7dfe7;
    margin-top: 6px;
  }
  .formula b {
    font: italic 17px Georgia;
    color: #46617a;
    padding: 0 15px;
  }
  .formula input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: white;
    font: 12px Arial;
    color: #364b5b;
  }
  .grid {
    flex: 1;
    overflow: auto;
    max-height: calc(100vh - 215px);
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
    background: #f5f7f8;
    color: #46617a;
    font-weight: 400;
    height: 28px;
    border-right: 1px solid #dfe5ea;
    border-bottom: 1px solid #dfe5ea;
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
    background: #f4f7f9;
  }
  .grid .corner {
    z-index: 4;
  }
  .grid th.chosen {
    background: #d5e9dc;
    color: #235e3b;
  }
  .grid td {
    height: 28px;
    min-width: 118px;
    width: 118px;
    max-width: 118px;
    border-right: 1px solid #e2e7ec;
    border-bottom: 1px solid #e2e7ec;
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
    background: #39825615;
    pointer-events: none;
  }
  .grid td.selected {
    outline: 2px solid #107c41;
    outline-offset: -2px;
    z-index: 1;
  }
  .grid td.match {
    box-shadow: inset 0 0 0 2px #edc263;
  }
  .cell-entry {
    position: absolute;
    inset: 0;
    width: 100%;
    border: 2px solid #107c41;
    outline: none;
    padding: 3px 5px;
    font: 12px Arial;
    z-index: 4;
    color: #243c2d;
    background: white;
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
    color: #46617a;
    background: #f1f6f3;
    border: 1px solid #dfebe4;
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
    background: #f6f9f7;
    border-top: 1px solid #e0e8e3;
    min-height: 44px;
    overflow: auto;
  }
  .sheet-tabs button {
    display: block;
    white-space: nowrap;
    font-size: 13px;
    padding: 4px 14px;
  }
  .sheet-tabs button.active {
    background: #d8ebe0;
    color: #2d6948;
  }
  .sheet-tabs span {
    margin-left: auto;
    white-space: nowrap;
    font-size: 13px;
    color: #46617a;
  }
  .notice {
    padding: 9px 20px;
    background: #fff7e4;
    color: #95652e;
    font-size: 13px;
  }
  .notice button {
    float: right;
    border: 0;
    background: none;
    color: inherit;
    text-decoration: underline;
  }
  @media print {
    .tools,
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
</style>
