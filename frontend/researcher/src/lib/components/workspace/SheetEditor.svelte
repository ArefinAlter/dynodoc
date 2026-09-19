<script lang="ts">
  import ContextMenu from '$lib/components/product/ContextMenu.svelte';
  import type { MenuAction } from '$lib/workspace/context-menu';
  import { sheetStructure, type SheetOperation } from '$lib/workspace/sheet-structure';
  import { HyperFormula } from 'hyperformula';
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
    Printer
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
  }
  let { documentState, editable, onChange, onSelect, onUndo, onRedo, onComment }: Props = $props();
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
  const sheets = $derived(
    Object.values(documentState.nodes)
      .filter((n) => !n.deleted && n.type === 'section' && n.current_fields.kind === 'sheet')
      .sort((a, b) => (a.pos < b.pos ? -1 : a.pos > b.pos ? 1 : 0))
  );
  const sheet = $derived(sheets.find((n) => n.id === activeSheet) || sheets[0]);
  const rows = $derived(sheet ? rowsForSheet(documentState, sheet.id) : []);
  const columns = $derived(Math.min(100, Number(sheet?.current_fields.columns) || 12));
  const shown = $derived(
    filter && find
      ? rows.filter((n) =>
          Object.entries(n.current_fields).some(
            ([k, v]) =>
              k.startsWith('cell_') && String(v).toLowerCase().includes(find.toLowerCase())
          )
        )
      : rows
  );
  const start = $derived(Math.max(0, Math.floor(scrollTop / 28) - 5));
  const visible = $derived(shown.slice(start, start + 65));
  let computed = $state<Record<string, unknown[][]>>({});
  const raw = (r: number, c: number) => String(shown[r]?.current_fields[`cell_${c}`] ?? '');
  const col = (n: number) => {
    let s = '';
    for (n++; n > 0; n = Math.floor((n - 1) / 26)) s = String.fromCharCode(65 + ((n - 1) % 26)) + s;
    return s;
  };
  $effect(() => {
    const input: Record<string, string[][]> = {};
    for (const s of sheets) {
      const count = Number(s.current_fields.columns) || 12;
      input[String(s.current_fields.label || s.id)] = rowsForSheet(documentState, s.id).map((n) =>
        Array.from({ length: count }, (_, c) => String(n.current_fields[`cell_${c}`] ?? ''))
      );
    }
    try {
      const hf = HyperFormula.buildFromSheets(input, { licenseKey: 'gpl-v3' });
      const output: Record<string, unknown[][]> = {};
      for (const s of sheets) {
        const id = hf.getSheetId(String(s.current_fields.label || s.id));
        output[s.id] = id === undefined ? [] : hf.getSheetValues(id);
      }
      computed = output;
      hf.destroy();
    } catch {
      computed = {};
    }
  });
  function display(n: MaterializedNode, c: number) {
    const r = rows.findIndex((row) => row.id === n.id);
    const result = computed[sheet?.id]?.[r]?.[c];
    if (result && typeof result === 'object')
      return String((result as { value?: unknown }).value || '#ERROR!');
    const format = n.current_fields[`format_${c}`] as { number?: string } | undefined;
    if (typeof result === 'number') {
      if (format?.number === 'percent') return (result * 100).toFixed(1) + '%';
      if (format?.number === 'decimal') return result.toFixed(2);
      if (format?.number === 'currency')
        return result.toLocaleString('en-US', { style: 'currency', currency: 'USD' });
    }
    return result === null || result === undefined
      ? String(n.current_fields[`cell_${c}`] ?? '')
      : String(result);
  }
  function style(n: MaterializedNode, c: number) {
    const f = n.current_fields[`format_${c}`] as Record<string, string | boolean> | undefined;
    return `font-weight:${f?.bold ? '700' : '400'};font-style:${f?.italic ? 'italic' : 'normal'};text-align:${f?.align || 'left'};background:${f?.background || 'white'};color:${f?.color || '#303b47'};white-space:${f?.wrap ? 'normal' : 'nowrap'}`;
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
    if (editing) commit();
    selected = {
      r: Math.max(0, Math.min(shown.length - 1, r)),
      c: Math.max(0, Math.min(columns - 1, c))
    };
    if (!extend) anchor = { ...selected };
    value = raw(selected.r, selected.c);
    onSelect?.(shown[selected.r]?.id || null);
    if (grid) {
      const top = selected.r * 28;
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
    editing = false;
    const n = shown[selected.r];
    if (n && value !== raw(selected.r, selected.c))
      onChange([{ type: 'FieldEdited', node_id: n.id, field: `cell_${selected.c}`, value }]);
  }
  function format(patch: Record<string, unknown>) {
    if (!editable) return;
    const x = range();
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
    const ops: EventPayload[] = [];
    for (let r = x.r1; r <= x.r2; r++)
      for (let c = x.c1; c <= x.c2; c++)
        if (shown[r])
          ops.push({ type: 'FieldEdited', node_id: shown[r].id, field: `cell_${c}`, value: '' });
    onChange(ops);
  }
  function key(e: KeyboardEvent) {
    if ((e.target as HTMLElement).closest('input,select')) return;
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
      choose(selected.r + r, selected.c + c, e.shiftKey && e.key !== 'Tab');
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
      Array.from({ length: count }, () => {
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
        fields: { ...sheet.current_fields, label: name }
      },
      ...rows.map(
        (n) =>
          ({
            type: 'NodeCreated',
            node_id: ulid(),
            node_type: 'item',
            parent_id: id,
            pos: n.pos,
            fields: structuredClone(n.current_fields)
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

<div
  class="sheet-editor"
  onkeydown={key}
  onpaste={paste}
  oncopy={copy}
  role="grid"
  aria-label="Spreadsheet editor"
  tabindex="0"
>
  <div class="tools">
    <button aria-label="Undo" onclick={onUndo} disabled={!editable}><Undo2 size={16} /></button
    ><button aria-label="Redo" onclick={onRedo} disabled={!editable}><Redo2 size={16} /></button
    ><button aria-label="Print spreadsheet" onclick={() => window.print()}
      ><Printer size={16} /></button
    ><i></i><select
      aria-label="Number format"
      disabled={!editable}
      onchange={(e) => format({ number: e.currentTarget.value })}
      ><option value="auto">Automatic</option><option value="decimal">Number · 0.00</option><option
        value="percent">Percent · %</option
      ><option value="currency">Currency · USD</option></select
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
    <span>{col(selected.c)}{selected.r + 1}</span><b>ƒx</b><input
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
    <table aria-label={String(sheet?.current_fields.label || 'Sheet')}>
      <thead
        ><tr
          ><th class="corner"></th>{#each Array.from({ length: columns }, (_, i) => i) as c}<th
              scope="col"
              class:chosen={c === selected.c}>{col(c)}</th
            >{/each}</tr
        ></thead
      ><tbody
        >{#if start > 0}<tr aria-hidden="true" style={`height:${start * 28}px`}
            ><td colspan={columns + 1}></td></tr
          >{/if}{#each visible as n, i (n.id)}{@const r = start + i}<tr
            ><th scope="row" class:chosen={r === selected.r}>{r + 1}</th
            >{#each Array.from({ length: columns }, (_, i) => i) as c}<td
                class:selected={r === selected.r && c === selected.c}
                class:range={inRange(r, c)}
                class:match={find &&
                  !filter &&
                  String(n.current_fields[`cell_${c}`] || '')
                    .toLowerCase()
                    .includes(find.toLowerCase())}
                style={style(n, c)}
                ><button
                  class="cell"
                  aria-label={`${col(c)}${r + 1}: ${display(n, c)}`}
                  onmousedown={(e) => {
                    if (e.button !== 0) return;
                    e.preventDefault();
                    choose(r, c, e.shiftKey);
                    (e.currentTarget.closest('.sheet-editor') as HTMLElement)?.focus();
                  }}
                  oncontextmenu={(e) => openCellMenu(e, r, c)}
                  ondblclick={() => void edit()}>{display(n, c)}</button
                >{#if editing && r === selected.r && c === selected.c}<input
                    class="cell-entry"
                    aria-label={`Edit ${col(c)}${r + 1}`}
                    bind:value
                    onblur={commit}
                    onkeydown={(e) => {
                      if (e.key === 'Enter' || e.key === 'Tab') {
                        e.preventDefault();
                        commit();
                        choose(r + (e.key === 'Enter' ? 1 : 0), c + (e.key === 'Tab' ? 1 : 0));
                        (e.currentTarget.closest('.sheet-editor') as HTMLElement)?.focus();
                      }
                      if (e.key === 'Escape') {
                        editing = false;
                      }
                    }}
                  />{/if}</td
              >{/each}</tr
          >{/each}{#if shown.length > start + 65}<tr
            aria-hidden="true"
            style={`height:${(shown.length - start - 65) * 28}px`}
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
    ><span>{shown.length} rows · {columns} columns</span>
  </div>
</div>

{#if context}<ContextMenu
    {...context}
    onClose={() => {
      context = null;
    }}
  />{/if}

<style>
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
  .formula span {
    width: 90px;
    border-right: 1px solid #dfe4eb;
    padding: 0 16px;
    color: #556b7d;
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
