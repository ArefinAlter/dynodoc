<script lang="ts">
  import { ulid } from 'ulid';
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
  const types = [
    'text',
    'integer',
    'decimal',
    'date',
    'select_one',
    'select_multiple',
    'note',
    'calculate',
    'geopoint',
    'image',
    'audio',
    'barcode'
  ];
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
  function add(section = false) {
    if (!root) return;
    const id = ulid();
    onChange([
      {
        type: 'NodeCreated',
        node_id: id,
        node_type: section ? 'section' : 'item',
        parent_id: current?.type === 'section' ? current.id : root.id,
        pos: between(items.at(-1)?.pos || ''),
        fields: { label: section ? 'New section' : 'Untitled question', question_type: 'text' },
        var_name: section ? null : `q_${id.slice(-6).toLowerCase()}`
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
        ><button class="add" onclick={() => add(true)}><Plus size={14} /> Add section</button>{/if}
    </nav>
    <div class="canvas">
      {#if preview}<div class="preview-note">
          Preview of labels, hints and answer controls. Relevance, validation expressions,
          calculations and media capture must be tested in the destination survey platform.
        </div>{/if}{#each items as n}{#if preview}<section class:section={n.type === 'section'}>
            <h3>
              {field(n, 'label')}{#if n.current_fields.required}<sup>*</sup>{/if}
            </h3>
            {#if field(n, 'hint')}<p>
                {field(n, 'hint')}
              </p>{/if}{#if n.type === 'item'}{@const type =
                field(n, 'question_type') ||
                'text'}{#if type.startsWith('select_')}{#each choices(n.id) as choice}<label
                    class="answer-choice"
                    ><input
                      type={type === 'select_multiple' ? 'checkbox' : 'radio'}
                      name={n.id}
                    />{field(choice, 'label')}</label
                  >{/each}{:else if type === 'note'}{:else if ['integer', 'decimal', 'text', 'date'].includes(type)}<input
                  aria-label={field(n, 'label')}
                  type={type === 'date'
                    ? 'date'
                    : type === 'integer' || type === 'decimal'
                      ? 'number'
                      : 'text'}
                  placeholder="Your answer"
                />{:else}<p class="device">
                  {type} is collected by the survey application.
                </p>{/if}{/if}
          </section>{:else}<section
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
            />{#if n.type === 'item'}<input
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
                    >{#each types as type}<option value={type}>{type.replaceAll('_', ' ')}</option
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
              {#if field(n, 'question_type').startsWith('select_')}<div class="choices">
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
              <details>
                <summary>Logic & validation</summary
                >{#each [{ key: 'relevance', label: 'Show when (XLSForm expression)' }, { key: 'constraint', label: 'Validation expression' }, { key: 'constraint_message', label: 'Validation message' }, { key: 'calculation', label: 'Calculation' }, { key: 'default', label: 'Default value' }] as f}<label
                    >{f.label}<input
                      disabled={!editable}
                      value={field(n, f.key)}
                      onchange={(e) => edit(n, f.key, e.currentTarget.value)}
                    /></label
                  >{/each}
              </details>{/if}
          </section>{/if}{:else}<div class="empty">
          <ClipboardList size={35} />
          <h2>Your questions start here</h2>
          <p>Add a question, or import an XLSForm from your workspace.</p>
          {#if editable}<button onclick={() => add()}>Add first question</button>{/if}
        </div>{/each}
    </div>
  </div>
</div>

<style>
  .questionnaire {
    background: #f5f7f8;
    flex: 1;
    overflow: auto;
    font: 13px Arial;
    color: #536577;
  }
  .tools {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 8px 18px;
    background: #edf2f7;
    margin: 0 12px;
    border-radius: 8px;
  }
  .tools button {
    display: flex;
    gap: 6px;
    align-items: center;
    border: 0;
    background: none;
    color: #7b8ca0;
    padding: 7px 12px;
    border-radius: 5px;
    font-size: 11px;
    cursor: pointer;
  }
  .tools button.active {
    background: white;
    color: #51718e;
    box-shadow: 0 1px 3px #38587212;
  }
  .tools span {
    flex: 1;
  }
  .layout {
    display: flex;
    min-height: calc(100vh - 170px);
  }
  nav {
    width: 220px;
    min-width: 220px;
    padding: 25px 14px;
    border-right: 1px solid #e3e9ef;
  }
  nav strong {
    display: block;
    font-size: 9px;
    letter-spacing: 1.5px;
    color: #a0adba;
    padding: 0 10px 15px;
  }
  nav button {
    display: flex;
    gap: 9px;
    align-items: center;
    text-align: left;
    background: none;
    border: 0;
    font-size: 11px;
    color: #7e8ea0;
    padding: 10px;
    border-radius: 5px;
    width: 100%;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  nav button span {
    font-size: 9px;
    color: #a0adba;
  }
  nav button.selected {
    background: #e7efe9;
    color: #577d65;
  }
  nav .add {
    color: #6b977d;
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
    background: white;
    border: 1px solid #e2e8ed;
    border-radius: 8px;
    padding: 23px 26px;
    margin-bottom: 20px;
    box-shadow: 0 2px 7px #1a3f6204;
  }
  section.focused {
    border-left: 3px solid #719d82;
    padding-left: 24px;
  }
  section.section {
    background: #edf3ef;
    border-color: #e0e9e3;
  }
  .question-heading {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
  }
  .question-heading span {
    flex: 1;
    font-size: 9px;
    letter-spacing: 1px;
    color: #a3b1bd;
  }
  .question-heading button,
  .choices button {
    background: none;
    border: 0;
    color: #9eafbb;
    cursor: pointer;
    padding: 3px;
  }
  .label {
    font-size: 17px;
    color: #5b7081;
    border: 0;
    border-bottom: 1px solid transparent;
    padding: 4px 0;
    width: 100%;
    background: none;
  }
  .label:focus {
    outline: none;
    border-color: #98b2a1;
  }
  .hint {
    font-size: 12px;
    color: #9aabb7;
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
    border-top: 1px solid #eff2f5;
  }
  .settings label,
  details label {
    display: grid;
    gap: 7px;
    font-size: 10px;
    color: #91a3b0;
    flex: 1;
  }
  .settings select,
  .settings input,
  details input {
    padding: 8px 9px;
    border: 1px solid #e0e7ed;
    border-radius: 4px;
    color: #6a8193;
    font: 11px Arial;
    width: 100%;
    background: white;
  }
  .settings label.check {
    display: flex;
    align-items: center;
    white-space: nowrap;
    gap: 5px;
    padding-bottom: 10px;
    font-size: 10px;
    flex: 0;
  }
  .settings .check input {
    width: auto;
  }
  details {
    margin-top: 22px;
    color: #95a8b7;
    font-size: 10px;
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
    border: 1px solid #e6ecf0;
    border-radius: 4px;
    padding: 7px 9px;
    font-size: 11px;
    color: #7890a1;
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
    font-size: 10px;
    color: #7ba389;
    margin-top: 12px;
  }
  .preview-note {
    font-size: 11px;
    line-height: 1.8;
    background: #fff8e9;
    color: #a79775;
    border: 1px solid #f2e8cf;
    padding: 15px 20px;
    margin-bottom: 24px;
    border-radius: 6px;
  }
  section h3 {
    font-size: 17px;
    font-weight: 500;
    color: #5d7384;
    margin: 0 0 10px;
  }
  section > p {
    font-size: 12px;
    color: #95a7b4;
    line-height: 1.7;
  }
  section > input:not(.label):not(.hint) {
    border: 1px solid #dfe7ed;
    border-radius: 5px;
    padding: 10px 12px;
    margin-top: 12px;
    width: 100%;
    font-size: 13px;
    color: #768a9d;
  }
  sup {
    color: #bf8673;
    margin-left: 4px;
  }
  .answer-choice {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 0;
    font-size: 13px;
    color: #7d91a1;
  }
  .empty {
    text-align: center;
    padding: 80px 40px;
    color: #a7b8c3;
  }
  .empty h2 {
    font-size: 22px;
    font-weight: 400;
    color: #6c8597;
  }
  .empty p {
    font-size: 13px;
  }
  .empty button {
    border: 0;
    background: #d8e9df;
    color: #538065;
    padding: 10px 18px;
    border-radius: 5px;
    font-size: 12px;
    cursor: pointer;
  }
  .error {
    padding: 10px 25px;
    background: #fff0e6;
    color: #b28970;
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
