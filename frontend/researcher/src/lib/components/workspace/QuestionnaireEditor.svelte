<script lang="ts">
  import { questionTypes, advancedColumns } from '$lib/workspace/question-types';
  import { ulid } from 'ulid';
  import SurveyPreview from './SurveyPreview.svelte';
  import {
    Plus,
    Trash2,
    Eye,
    List,
    ArrowUp,
    ArrowDown,
    Download,
    ClipboardList
  } from '@lucide/svelte';
  import type { DocumentState, EventPayload, MaterializedNode } from '$lib/api/types';
  import { between } from '$lib/workspace/changes';
  import { exportXlsForm, exportRedcap } from '$lib/workspace/research-files';
  let {
    documentState,
    editable,
    onChange,
    onSelect
  }: {
    documentState: DocumentState;
    editable: boolean;
    onChange: (ops: EventPayload[]) => void;
    onSelect?: (id: string | null) => void;
  } = $props();
  let preview = $state(false),
    selected = $state(''),
    error = $state('');
  const root = $derived(
    Object.values(documentState.nodes).find((n) => !n.deleted && n.type === 'form')
  );
  const items = $derived(
    Object.values(documentState.nodes)
      .filter((n) => !n.deleted && (n.type === 'item' || n.type === 'section'))
      .sort((a, b) => (a.pos < b.pos ? -1 : a.pos > b.pos ? 1 : 0))
  );
  const current = $derived(documentState.nodes[selected]);
  const types = questionTypes.flatMap((group) => group.items.map(([type]) => type));
  function column(n: MaterializedNode, key: string) {
    return String(
      n.current_fields[key] ??
        (n.current_fields.xlsform as Record<string, unknown> | undefined)?.[key] ??
        ''
    );
  }
  function extra(n: MaterializedNode, text: string) {
    try {
      const value = JSON.parse(text);
      if (
        !value ||
        Array.isArray(value) ||
        typeof value !== 'object' ||
        Object.values(value).some((x) => typeof x !== 'string')
      )
        throw new Error('Use an object with string values.');
      edit(n, 'xlsform', value);
      error = '';
    } catch (e) {
      error = e instanceof Error ? e.message : 'Invalid JSON';
    }
  }
  function nested(n: MaterializedNode, parent: string): boolean {
    let p = n.parent_id;
    const seen = new Set<string>();
    while (p && !seen.has(p)) {
      if (p === parent) return true;
      seen.add(p);
      p = documentState.nodes[p]?.parent_id || null;
    }
    return false;
  }
  function relocate(n: MaterializedNode, parent: string) {
    if (parent === n.id || nested(documentState.nodes[parent], n.id)) return;
    onChange([
      {
        type: 'NodeMoved',
        node_id: n.id,
        new_parent_id: parent,
        new_pos: between(items.filter((x) => x.parent_id === parent).at(-1)?.pos || '')
      }
    ]);
  }
  const field = (n: MaterializedNode, k: string) => String(n.current_fields[k] ?? '');
  function edit(n: MaterializedNode, k: string, v: unknown) {
    onChange([{ type: 'FieldEdited', node_id: n.id, field: k, value: v }]);
  }
  function choices(id: string) {
    return Object.values(documentState.nodes)
      .filter(
        (n) =>
          !n.deleted &&
          !documentState.removed_choices.includes(n.id) &&
          n.type === 'choice' &&
          n.parent_id === id
      )
      .sort((a, b) => (a.pos < b.pos ? -1 : 1));
  }
  function add(section = false, repeat = false) {
    if (!root) return;
    const id = ulid();
    onChange([
      {
        type: 'NodeCreated',
        node_id: id,
        node_type: section ? 'section' : 'item',
        parent_id: current?.type === 'section' ? current.id : current?.parent_id || root.id,
        pos: between(items.at(-1)?.pos || ''),
        fields: {
          label: section ? (repeat ? 'Repeating group' : 'New section') : 'Untitled question',
          question_type: 'text',
          group_type: section ? (repeat ? 'repeat' : 'group') : null
        },
        var_name: `${section ? 'group' : 'q'}_${id.slice(-8).toLowerCase()}`
      }
    ]);
    selected = id;
    onSelect?.(id);
  }
  function remove(n: MaterializedNode) {
    const ids = new Set([n.id]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const x of Object.values(documentState.nodes))
        if (x.parent_id && ids.has(x.parent_id) && !ids.has(x.id)) {
          ids.add(x.id);
          changed = true;
        }
    }
    onChange(
      [...ids]
        .filter((id) => !documentState.nodes[id].deleted)
        .reverse()
        .map((node_id) => ({ type: 'NodeDeleted', node_id }))
    );
    selected = '';
  }
  function move(n: MaterializedNode, dir: number) {
    const siblings = items.filter((x) => x.parent_id === n.parent_id);
    const i = siblings.findIndex((x) => x.id === n.id),
      other = siblings[i + dir];
    if (!other) return;
    onChange([
      { type: 'NodeMoved', node_id: n.id, new_parent_id: n.parent_id, new_pos: other.pos },
      { type: 'NodeMoved', node_id: other.id, new_parent_id: other.parent_id, new_pos: n.pos }
    ]);
  }
  async function exportFile(type: string) {
    try {
      if (type === 'xlsform')
        await exportXlsForm(documentState, field(root!, 'label') || 'Questionnaire');
      else exportRedcap(documentState, field(root!, 'label') || 'Questionnaire');
    } catch (e) {
      error = e instanceof Error ? e.message : 'Export failed';
    }
  }
</script>

<div class="questionnaire">
  <div class="tools">
    <button class:active={!preview} onclick={() => (preview = false)}
      ><List size={15} /> Build</button
    ><button class:active={preview} onclick={() => (preview = true)}
      ><Eye size={15} /> Preview</button
    ><span></span><button onclick={() => void exportFile('xlsform')}
      ><Download size={14} /> XLSForm</button
    ><button onclick={() => void exportFile('redcap')}
      ><Download size={14} /> REDCap dictionary</button
    >
  </div>
  {#if error}<p role="alert" class="error">{error}</p>{/if}
  <div class="layout">
    <nav>
      <strong>QUESTIONNAIRE</strong>{#each items as n}<button
          class:selected={selected === n.id}
          onclick={() => {
            selected = n.id;
            onSelect?.(n.id);
          }}
          >{#if n.type === 'section'}<ClipboardList size={14} />{:else}<span
              >{items.filter((x) => x.type === 'item').findIndex((x) => x.id === n.id) + 1}</span
            >{/if}{field(n, 'label') || 'Untitled'}</button
        >{/each}{#if editable && !preview}<button class="add" onclick={() => add()}
          ><Plus size={14} /> Add question</button
        ><button class="add" onclick={() => add(true)}><Plus size={14} /> Add section</button
        ><button class="add" onclick={() => add(true, true)}
          ><Plus size={14} />Add repeating group</button
        >{/if}
    </nav>
    <div class="canvas">
      {#if preview}<SurveyPreview {documentState} />{:else}{#each items as n}<section
            class:focused={selected === n.id}
            class:section={n.type === 'section'}
          >
            <div class="question-heading">
              <span>{n.type === 'section' ? 'SECTION' : n.var_name || 'QUESTION'}</span
              >{#if editable}<button aria-label="Move up" onclick={() => move(n, -1)}
                  ><ArrowUp size={14} /></button
                ><button aria-label="Move down" onclick={() => move(n, 1)}
                  ><ArrowDown size={14} /></button
                ><button aria-label="Delete question" onclick={() => remove(n)}
                  ><Trash2 size={14} /></button
                >{/if}
            </div>
            <input
              class="label"
              aria-label="Question label"
              value={field(n, 'label')}
              disabled={!editable}
              onfocus={() => {
                selected = n.id;
                onSelect?.(n.id);
              }}
              onchange={(e) => edit(n, 'label', e.currentTarget.value)}
            />
            <details class="structure">
              <summary>Location & grouping</summary><label
                >Inside<select
                  disabled={!editable}
                  value={n.parent_id || root?.id}
                  onchange={(e) => relocate(n, e.currentTarget.value)}
                  ><option value={root?.id}>Top level</option
                  >{#each items.filter((g) => g.type === 'section' && g.id !== n.id && !nested(g, n.id)) as g}<option
                      value={g.id}>{field(g, 'label')}</option
                    >{/each}</select
                ></label
              >{#if n.type === 'section'}<label
                  >Group variable<input
                    disabled={!editable}
                    value={n.var_name || ''}
                    pattern="[a-zA-Z_][a-zA-Z0-9_]*"
                    onchange={(e) => edit(n, 'var_name', e.currentTarget.value)}
                  /></label
                ><label
                  >Group behavior<select
                    disabled={!editable}
                    value={field(n, 'group_type') || 'group'}
                    onchange={(e) => edit(n, 'group_type', e.currentTarget.value)}
                    ><option value="group">Show once</option><option value="repeat"
                      >Repeat for multiple records</option
                    ></select
                  ></label
                ><label
                  >Repeat count expression<input
                    disabled={!editable}
                    value={column(n, 'repeat_count')}
                    onchange={(e) => edit(n, 'repeat_count', e.currentTarget.value)}
                  /></label
                ><label
                  >Show group when<input
                    disabled={!editable}
                    value={field(n, 'relevance')}
                    onchange={(e) => edit(n, 'relevance', e.currentTarget.value)}
                  /></label
                >{/if}
            </details>
            {#if n.type === 'item'}<input
                class="hint"
                aria-label="Question hint"
                placeholder="Add a hint for the respondent"
                value={field(n, 'hint')}
                disabled={!editable}
                onchange={(e) => edit(n, 'hint', e.currentTarget.value)}
              />
              <div class="settings">
                <label
                  >Answer type<select
                    disabled={!editable}
                    value={field(n, 'question_type') || 'text'}
                    onchange={(e) => edit(n, 'question_type', e.currentTarget.value)}
                    >{#if !types.includes(field(n, 'question_type')) && field(n, 'question_type')}<option
                        value={field(n, 'question_type')}
                        >{field(n, 'question_type')} · imported type</option
                      >{/if}{#each questionTypes as group}<optgroup label={group.group}
                        >{#each group.items as [type, label]}<option value={type}>{label}</option
                          >{/each}</optgroup
                      >{/each}</select
                  ></label
                ><label
                  >Variable name<input
                    disabled={!editable}
                    value={n.var_name || ''}
                    onchange={(e) => edit(n, 'var_name', e.currentTarget.value)}
                    pattern="[a-zA-Z_][a-zA-Z0-9_]*"
                  /></label
                ><label class="check"
                  ><input
                    type="checkbox"
                    disabled={!editable}
                    checked={!!n.current_fields.required}
                    onchange={(e) => edit(n, 'required', e.currentTarget.checked)}
                  /> Required</label
                >
              </div>
              {#if ['select_one', 'select_multiple', 'rank'].includes(field(n, 'question_type'))}<div
                  class="choices"
                >
                  {#each choices(n.id) as choice}<div>
                      <input
                        aria-label="Choice value"
                        placeholder="value"
                        disabled={!editable}
                        value={field(choice, 'value')}
                        onchange={(e) => edit(choice, 'value', e.currentTarget.value)}
                      /><input
                        aria-label="Choice label"
                        placeholder="Label"
                        disabled={!editable}
                        value={field(choice, 'label')}
                        onchange={(e) => edit(choice, 'label', e.currentTarget.value)}
                      />{#if editable}<button
                          aria-label="Remove choice"
                          onclick={() =>
                            onChange([
                              { type: 'ChoiceRemoved', node_id: n.id, choice_id: choice.id }
                            ])}><Trash2 size={13} /></button
                        >{/if}
                    </div>{/each}{#if editable}<button
                      onclick={() =>
                        onChange([
                          {
                            type: 'ChoiceAdded',
                            node_id: n.id,
                            choice_id: ulid(),
                            fields: { value: String(choices(n.id).length + 1), label: 'New option' }
                          }
                        ])}><Plus size={12} /> Add option</button
                    >{/if}
                </div>{/if}
              {#if field(n, 'question_type').endsWith('_from_file') || field(n, 'question_type') === 'xml-external'}<label
                  >Dataset filename<input
                    disabled={!editable}
                    placeholder="choices.csv"
                    value={field(n, 'external_file')}
                    onchange={(e) => edit(n, 'external_file', e.currentTarget.value)}
                  /></label
                >
                <p class="support-note">
                  Attach this dataset to the form in ODK or Kobo before field use.
                </p>{/if}
              <details>
                <summary>Logic & validation</summary
                >{#each [{ key: 'relevance', label: 'Show when (XLSForm expression)' }, { key: 'required_expression', label: 'Required when (XLSForm expression)' }, { key: 'constraint', label: 'Validation expression' }, { key: 'constraint_message', label: 'Validation message' }, { key: 'calculation', label: 'Calculation' }, { key: 'default', label: 'Default value' }] as f}<label
                    >{f.label}<input
                      disabled={!editable}
                      value={field(n, f.key)}
                      onchange={(e) => edit(n, f.key, e.currentTarget.value)}
                    /></label
                  >{/each}
              </details>{/if}
            <details>
              <summary>Appearance, translations & advanced XLSForm</summary>
              <p class="support-note">
                These settings are written to the XLSForm. Device capture, repeats, external
                datasets and advanced expressions need validation in your destination platform.
              </p>
              {#each advancedColumns.filter((key) => n.type !== 'section' || ['appearance', 'parameters', 'repeat_count', 'read_only'].includes(key)) as key}<label
                  >{key}<input
                    disabled={!editable}
                    value={column(n, key)}
                    onchange={(e) => edit(n, key, e.currentTarget.value)}
                  /></label
                >{/each}<label
                >Additional XLSForm columns and translations (JSON)<textarea
                  disabled={!editable}
                  rows="5"
                  value={JSON.stringify(n.current_fields.xlsform || {}, null, 2)}
                  onchange={(e) => extra(n, e.currentTarget.value)}
                ></textarea></label
              >
              <p class="support-note">
                Use keys such as label::English (en), label::বাংলা (bn), hint::বাংলা (bn), or custom
                bind columns. Keep values as strings.
              </p>
            </details>
          </section>{:else}<div class="empty">
            <ClipboardList size={35} />
            <h2>Your questions start here</h2>
            <p>Add a question, or import an XLSForm from your workspace.</p>
            {#if editable}<button onclick={() => add()}>Add first question</button>{/if}
          </div>{/each}{/if}
    </div>
  </div>
</div>

<style>
  .support-note {
    font-size: 14px;
    line-height: 1.7;
    color: var(--color-ink-soft);
  }
  .structure {
    margin: 12px 0;
  }
  textarea {
    width: 100%;
    padding: 12px;
    border: 1px solid var(--color-border-strong);
    border-radius: 5px;
    color: var(--color-ink);
    background: var(--color-surface-raised);
    font: 13px monospace;
  }

  .questionnaire {
    background: var(--color-surface-sunken);
    flex: 1;
    overflow: auto;
    font: 13px Arial;
    color: var(--color-ink-soft);
  }
  .tools {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 8px 18px;
    background: var(--color-surface-hover);
    margin: 0 12px;
    border-radius: 8px;
  }
  .tools button {
    display: flex;
    gap: 6px;
    align-items: center;
    border: 0;
    background: none;
    color: var(--color-ink-soft);
    padding: 7px 12px;
    border-radius: 5px;
    font-size: 13px;
    cursor: pointer;
  }
  .tools button.active {
    background: var(--color-surface-raised);
    color: var(--color-ink-soft);
    box-shadow: var(--shadow-panel);
  }
  .tools span {
    flex: 1;
  }
  .layout {
    display: flex;
    min-height: calc(100vh - 170px);
    min-height: calc(100dvh - 170px);
  }
  nav {
    width: 220px;
    min-width: 220px;
    padding: 25px 14px;
    border-right: 1px solid var(--color-border);
  }
  nav strong {
    display: block;
    font-size: 13px;
    letter-spacing: 1.5px;
    color: var(--color-ink-soft);
    padding: 0 10px 15px;
  }
  nav button {
    display: flex;
    gap: 9px;
    align-items: center;
    text-align: left;
    background: none;
    border: 0;
    font-size: 13px;
    color: var(--color-ink-soft);
    padding: 10px;
    border-radius: 5px;
    width: 100%;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  nav button span {
    font-size: 13px;
    color: var(--color-ink-soft);
  }
  nav button.selected {
    background: var(--color-success-bg);
    color: var(--color-success);
  }
  nav .add {
    color: var(--color-ink-soft);
    margin-top: 10px;
  }
  .canvas {
    flex: 1;
    min-width: 0;
    max-width: 950px;
    margin: auto;
    padding: 30px 50px;
    align-self: start;
  }
  section {
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 23px 26px;
    margin-bottom: 20px;
    box-shadow: var(--shadow-panel);
  }
  section.focused {
    border-left: 3px solid var(--color-success);
    padding-left: 24px;
  }
  section.section {
    background: var(--color-success-bg);
    border-color: var(--color-success-border);
  }
  .question-heading {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
  }
  .question-heading span {
    flex: 1;
    font-size: 13px;
    letter-spacing: 1px;
    color: var(--color-ink-soft);
  }
  .question-heading button,
  .choices button {
    background: none;
    border: 0;
    color: var(--color-ink-soft);
    cursor: pointer;
    padding: 3px;
  }
  .label {
    font-size: 17px;
    color: var(--color-ink-soft);
    border: 0;
    border-bottom: 1px solid transparent;
    padding: 4px 0;
    width: 100%;
    background: none;
  }
  .label:focus {
    outline: none;
    border-color: color-mix(in srgb, var(--color-success) 45%, var(--color-surface-raised));
  }
  .hint {
    font-size: 13px;
    color: var(--color-ink-soft);
    border: 0;
    background: none;
    width: 100%;
    padding: 10px 0;
    margin-bottom: 10px;
  }
  .settings {
    display: flex;
    gap: 18px;
    align-items: end;
    padding-top: 15px;
    border-top: 1px solid var(--color-border-soft);
  }
  .settings label,
  details label {
    display: grid;
    gap: 7px;
    font-size: 13px;
    color: var(--color-ink-soft);
    flex: 1;
  }
  .settings select,
  .settings input,
  details input {
    padding: 8px 9px;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    color: var(--color-ink-soft);
    font: 11px Arial;
    width: 100%;
    background: var(--color-surface-raised);
  }
  .settings label.check {
    display: flex;
    align-items: center;
    white-space: nowrap;
    gap: 5px;
    padding-bottom: 10px;
    font-size: 13px;
    flex: 0;
  }
  .settings .check input {
    width: auto;
  }
  details {
    margin-top: 22px;
    color: var(--color-ink-soft);
    font-size: 13px;
  }
  summary {
    cursor: pointer;
  }
  details label {
    margin-top: 14px;
  }
  .choices {
    margin-top: 20px;
    padding: 12px 0;
  }
  .choices > div {
    display: flex;
    gap: 8px;
    margin: 7px 0;
  }
  .choices input {
    border: 1px solid var(--color-border-soft);
    border-radius: 4px;
    padding: 7px 9px;
    font-size: 13px;
    color: var(--color-ink-soft);
    background: var(--color-surface-raised);
    width: 100%;
    min-width: 0;
  }
  .choices input:first-child {
    max-width: 80px;
  }
  .choices > button {
    display: flex;
    gap: 4px;
    align-items: center;
    font-size: 13px;
    color: var(--color-ink-soft);
    margin-top: 12px;
  }
  .empty {
    text-align: center;
    padding: 80px 40px;
    color: var(--color-ink-soft);
  }
  .empty h2 {
    font-size: 22px;
    font-weight: 400;
    color: var(--color-ink-soft);
  }
  .empty p {
    font-size: 13px;
  }
  .empty button {
    border: 0;
    background: var(--color-success-bg);
    color: var(--color-success);
    padding: 10px 18px;
    border-radius: 5px;
    font-size: 13px;
    cursor: pointer;
  }
  .error {
    padding: 10px 25px;
    background: var(--color-danger-bg);
    color: var(--color-ink-soft);
  }
  @media (max-width: 1000px) {
    nav {
      display: none;
    }
    .canvas {
      padding: 25px;
      width: 100%;
    }
  }
  @media (max-width: 650px) {
    .settings {
      flex-wrap: wrap;
    }
    .tools {
      flex-wrap: wrap;
    }
    .tools span {
      display: none;
    }
    .canvas {
      padding: 15px;
    }
    section {
      padding: 20px;
    }
    .settings label {
      min-width: 100px;
    }
  }
</style>
