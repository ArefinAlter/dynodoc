<script lang="ts">
  import { byPosition } from '$lib/workspace/api';
  import type { DocumentState, MaterializedNode } from '$lib/api/types';
  import { previewSurvey, type Answer } from '$lib/workspace/survey-logic';
  let { documentState }: { documentState: DocumentState } = $props();
  let answers = $state<Record<string, Answer>>({}),
    checked = $state(false);
  const result = $derived(previewSurvey(documentState, answers, checked));
  const items = $derived(
    Object.values(documentState.nodes)
      .filter((n) => !n.deleted && ['item', 'section'].includes(n.type))
      .sort((a, b) => byPosition(a, b))
  );
  const field = (n: MaterializedNode, k: string) => String(n.current_fields[k] || '');
  const choices = (n: MaterializedNode) =>
    Object.values(documentState.nodes)
      .filter(
        (c) =>
          c.parent_id === n.id &&
          c.type === 'choice' &&
          !c.deleted &&
          !documentState.removed_choices.includes(c.id)
      )
      .sort((a, b) => byPosition(a, b));
  function multiple(id: string, option: string, selected: boolean) {
    const old = Array.isArray(answers[id]) ? (answers[id] as string[]) : [];
    answers[id] = selected ? [...old, option] : old.filter((v) => v !== option);
  }
</script>

<div class="preview-note">
  Try the respondent flow. Supported XLSForm relevance, calculations, required answers and
  constraints run here. Answers stay in this browser view and are never saved as research responses.
  Repeat groups, media and vendor-specific logic require destination-platform testing.
</div>
<form
  onsubmit={(e) => {
    e.preventDefault();
    checked = true;
  }}
  novalidate
>
  {#each items as n}
    {#if result.visible[n.id]}
      <section class:group={n.type === 'section'}>
        <h3>
          {field(n, 'label')}{#if n.current_fields.required}<span aria-label="required">
              *</span
            >{/if}
        </h3>
        {#if field(n, 'hint')}<p>{field(n, 'hint')}</p>{/if}
        {#if n.type === 'item'}{@const type = field(n, 'question_type') || 'text'}
          {#if type.startsWith('select_')}
            <fieldset>
              <legend>{field(n, 'label')}</legend>{#each choices(n) as c}<label class="choice"
                  ><input
                    type={type === 'select_multiple' ? 'checkbox' : 'radio'}
                    name={n.id}
                    value={field(c, 'value')}
                    checked={type === 'select_multiple'
                      ? Array.isArray(answers[n.id]) &&
                        (answers[n.id] as string[]).includes(field(c, 'value'))
                      : String(result.values[n.id] ?? '') === field(c, 'value')}
                    onchange={(e) =>
                      type === 'select_multiple'
                        ? multiple(n.id, field(c, 'value'), e.currentTarget.checked)
                        : (answers[n.id] = field(c, 'value'))}
                  />{field(c, 'label')}</label
                >{/each}
            </fieldset>
          {:else if type === 'calculate'}<output aria-label={field(n, 'label')}
              >{String(result.values[n.id] ?? '')}</output
            >
          {:else if ['text', 'integer', 'decimal', 'date'].includes(type)}<label class="answer"
              ><span>{field(n, 'label')}</span><input
                type={type === 'date'
                  ? 'date'
                  : ['integer', 'decimal'].includes(type)
                    ? 'number'
                    : 'text'}
                step={type === 'integer' ? '1' : 'any'}
                value={String(result.values[n.id] ?? '')}
                oninput={(e) => (answers[n.id] = e.currentTarget.value)}
                aria-invalid={checked && !!result.errors[n.id]}
              /></label
            >{/if}
        {/if}
        {#each result.errors[n.id] || [] as message}<p class="problem" role="status">
            {message}
          </p>{/each}
      </section>
    {/if}
  {/each}
  <div class="actions">
    <button type="submit">Check answers</button><button
      type="button"
      onclick={() => {
        answers = {};
        checked = false;
      }}>Reset preview</button
    >
  </div>
  {#if checked}<p role="status" class:problem={!result.valid}>
      {result.valid
        ? 'These answers pass the supported preview checks.'
        : 'Review the highlighted answers and unsupported logic before fieldwork.'}
    </p>{/if}
</form>

<style>
  .preview-note {
    background: var(--color-accent-bg);
    border: 1px solid var(--color-accent-border);
    color: var(--color-accent-strong);
    padding: 18px;
    line-height: 1.7;
    margin-bottom: 24px;
    border-radius: 6px;
  }
  section {
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border);
    padding: 24px;
    margin-bottom: 18px;
    border-radius: 7px;
  }
  .group {
    background: var(--color-success-bg);
  }
  h3 {
    font-size: 17px;
    color: var(--color-ink);
    margin: 0 0 12px;
  }
  p {
    line-height: 1.6;
    color: var(--color-ink-soft);
  }
  fieldset {
    border: 0;
    padding: 0;
  }
  legend,
  .answer > span {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
  .choice {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 10px 0;
    color: var(--color-ink);
  }
  .answer input {
    padding: 12px;
    border: 1px solid var(--color-border-strong);
    border-radius: 5px;
    width: 100%;
    background: var(--color-surface-raised);
    color: var(--color-ink-strong);
  }
  .problem {
    color: var(--color-danger);
    background: var(--color-danger-bg);
    padding: 12px;
    border-radius: 5px;
  }
  output {
    display: block;
    font-size: 22px;
    color: var(--color-success);
  }
  .actions {
    display: flex;
    gap: 12px;
  }
  button {
    padding: 12px 18px;
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
    border: 1px solid var(--color-accent-border);
    border-radius: 6px;
    cursor: pointer;
  }
</style>
