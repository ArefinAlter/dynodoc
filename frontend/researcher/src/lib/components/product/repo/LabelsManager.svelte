<script lang="ts">
  import { onMount } from 'svelte';
  import { Check, Pencil, Plus, Tag, Trash2 } from '@lucide/svelte';
  import {
    DEFAULT_LABEL_COLOR,
    FALLBACK_COLOR,
    LABEL_DESCRIPTION_MAX,
    LABEL_NAME_MAX,
    LABEL_PALETTE,
    createLabel,
    deleteLabel,
    issueMessage,
    labelTextColor,
    normalizeColor,
    updateLabel,
    type LabelRow
  } from '$lib/workspace/issues';
  import { plural } from '$lib/workspace/repo-ui';
  import LabelPill from './LabelPill.svelte';

  let {
    projectId,
    labels,
    canManage,
    failed = false,
    onReload,
    onBack,
    onShowIssues
  }: {
    projectId: string;
    labels: LabelRow[];
    canManage: boolean;
    failed?: boolean;
    /** Read the labels again after a change, so counts come from the server. */
    onReload: () => Promise<void>;
    onBack: () => void;
    onShowIssues: (labelId: string) => void;
  } = $props();

  type Form = { name: string; color: string; description: string };
  const blank = (): Form => ({ name: '', color: DEFAULT_LABEL_COLOR, description: '' });
  let creating = $state(false),
    draft = $state<Form>(blank()),
    editing = $state<string | null>(null),
    edit = $state<Form>(blank()),
    deleting = $state<string | null>(null),
    busy = $state(false),
    problem = $state(''),
    note = $state(''),
    heading = $state<HTMLElement | null>(null);

  onMount(() => heading?.focus({ preventScroll: true }));

  const colorOk = (form: Form) => normalizeColor(form.color) !== null;
  const valid = (form: Form) => !!form.name.trim() && colorOk(form);

  async function run(work: () => Promise<void>, done: string) {
    if (busy) return;
    busy = true;
    problem = '';
    note = '';
    try {
      await work();
      await onReload();
      note = done;
    } catch (error) {
      problem = issueMessage(error, 'label');
    } finally {
      busy = false;
    }
  }
  const create = () =>
    run(async () => {
      await createLabel(projectId, { ...draft, color: normalizeColor(draft.color) ?? '' });
      draft = blank();
      creating = false;
    }, 'Label created.');
  const save = (id: string) =>
    run(async () => {
      await updateLabel(projectId, id, { ...edit, color: normalizeColor(edit.color) ?? '' });
      editing = null;
    }, 'Label saved. Issues that use it show the new name and colour.');
  const remove = (label: LabelRow) =>
    run(async () => {
      await deleteLabel(projectId, label.id);
      deleting = null;
    }, `Label "${label.name}" deleted.`);
  function begin(label: LabelRow) {
    editing = label.id;
    deleting = null;
    edit = { name: label.name, color: label.color, description: label.description };
    problem = '';
    note = '';
  }
</script>

{#snippet fields(form: Form, prefix: string)}
  <label
    >Name<input
      bind:value={form.name}
      required
      maxlength={LABEL_NAME_MAX}
      autocomplete="off"
      disabled={busy}
    /></label
  >
  <label
    >Description (optional)<input
      bind:value={form.description}
      maxlength={LABEL_DESCRIPTION_MAX}
      autocomplete="off"
      disabled={busy}
    /></label
  >
  <fieldset class="iss-fieldset" disabled={busy}>
    <legend>Colour</legend>
    <div class="iss-swatches">
      {#each LABEL_PALETTE as swatch (swatch.color)}
        <label class="iss-swatch" title={swatch.name}>
          <input
            type="radio"
            name={`${prefix}-color`}
            value={swatch.color}
            checked={normalizeColor(form.color) === swatch.color}
            onchange={() => (form.color = swatch.color)}
          />
          <span
            class="dot"
            style:background-color={`#${swatch.color}`}
            style:color={labelTextColor(swatch.color)}
            aria-hidden="true"><Check size={14} aria-hidden="true" /></span
          >
          <span class="sr-only">{swatch.name}</span>
        </label>
      {/each}
    </div>
    <div class="iss-hex">
      <label
        >Custom colour (6 hexadecimal digits)<input
          bind:value={form.color}
          maxlength="7"
          autocomplete="off"
          spellcheck="false"
          aria-invalid={colorOk(form) ? undefined : 'true'}
        /></label
      >
      <span class="iss-preview" aria-hidden="true"
        ><LabelPill
          name={form.name.trim() || 'Label preview'}
          color={normalizeColor(form.color) ?? FALLBACK_COLOR}
        /></span
      >
    </div>
    {#if !colorOk(form)}<p class="repo-small iss-invalid">
        Use six hexadecimal digits, such as 0969da.
      </p>{/if}
  </fieldset>
{/snippet}

<section class="iss-labels" aria-labelledby="iss-labels-title">
  <div class="rp-section-head">
    <h2 id="iss-labels-title" tabindex="-1" bind:this={heading}>
      Labels{#if labels.length}<span class="repo-counter">{labels.length}</span>{/if}
    </h2>
    <div class="repo-actions">
      {#if canManage && !creating}<button
          type="button"
          class="repo-primary"
          disabled={busy}
          onclick={() => {
            creating = true;
            editing = null;
            draft = blank();
          }}><Plus size={15} aria-hidden="true" />New label</button
        >{/if}
      <button type="button" onclick={onBack}>Back to issues</button>
    </div>
  </div>
  <p class="repo-muted">
    {canManage
      ? 'Labels group issues by topic or status. Changing a label updates every issue that uses it.'
      : 'Labels group issues by topic or status. Only managers and owners can create or change them.'}
  </p>

  <div aria-live="polite">
    {#if note}<p class="repo-notice">{note}</p>{/if}
  </div>
  <div aria-live="assertive">
    {#if problem}<p class="repo-alert">{problem}</p>{/if}
  </div>

  {#if creating}
    <form
      class="repo-box repo-panel-body repo-form iss-form"
      aria-label="Create a label"
      onsubmit={(e) => {
        e.preventDefault();
        if (valid(draft)) void create();
      }}
    >
      {@render fields(draft, 'new-label')}
      <div class="repo-actions">
        <button class="repo-primary" disabled={busy || !valid(draft)}>Create label</button>
        <button type="button" disabled={busy} onclick={() => (creating = false)}>Cancel</button>
      </div>
    </form>
  {/if}

  {#if failed}
    <div class="repo-box">
      <p class="repo-empty">Labels could not be loaded. Try again later.</p>
    </div>
  {:else if !labels.length}
    <div class="repo-box">
      <div class="repo-empty">
        <Tag size={30} aria-hidden="true" />
        <h3>No labels yet</h3>
        <p>
          {canManage
            ? 'Create a label such as "Needs evidence" or "Blocked" to group issues.'
            : 'A manager or owner can create labels to group issues.'}
        </p>
      </div>
    </div>
  {:else}
    <ul class="repo-box iss-label-list" aria-label="Project labels">
      {#each labels as label (label.id)}
        <li>
          {#if editing === label.id}
            <form
              class="repo-form iss-form iss-inline-form"
              aria-label={`Editing label ${label.name}`}
              onsubmit={(e) => {
                e.preventDefault();
                if (valid(edit)) void save(label.id);
              }}
            >
              {@render fields(edit, `edit-${label.id}`)}
              <div class="repo-actions">
                <button class="repo-primary" disabled={busy || !valid(edit)}>Save label</button>
                <button type="button" disabled={busy} onclick={() => (editing = null)}
                  >Cancel</button
                >
              </div>
            </form>
          {:else}
            <div class="iss-label-row">
              <span class="iss-label-name"><LabelPill name={label.name} color={label.color} /></span
              >
              <span class="iss-label-desc repo-muted">{label.description}</span>
              <button
                type="button"
                class="repo-linkbtn"
                aria-label={`${plural(label.openIssueCount, 'open issue')} labelled ${label.name}`}
                onclick={() => onShowIssues(label.id)}
                >{plural(label.openIssueCount, 'open issue')}</button
              >
              {#if canManage}
                <span class="repo-actions">
                  <button
                    type="button"
                    disabled={busy}
                    aria-label={`Edit label ${label.name}`}
                    onclick={() => begin(label)}><Pencil size={14} aria-hidden="true" />Edit</button
                  >
                  <button
                    type="button"
                    class="iss-danger"
                    disabled={busy}
                    aria-label={`Delete label ${label.name}`}
                    onclick={() => {
                      deleting = deleting === label.id ? null : label.id;
                      editing = null;
                    }}><Trash2 size={14} aria-hidden="true" />Delete</button
                  >
                </span>
              {/if}
            </div>
            {#if deleting === label.id}
              <div class="iss-confirm" role="group" aria-label={`Confirm deleting ${label.name}`}>
                <p>
                  Delete <strong>{label.name}</strong>? It is removed from every issue that uses it
                  ({plural(label.openIssueCount, 'open issue')}, plus any closed ones). This cannot
                  be undone.
                </p>
                <div class="repo-actions">
                  <button
                    type="button"
                    class="iss-danger-solid"
                    disabled={busy}
                    onclick={() => void remove(label)}>Delete label</button
                  >
                  <button type="button" disabled={busy} onclick={() => (deleting = null)}
                    >Cancel</button
                  >
                </div>
              </div>
            {/if}
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>
