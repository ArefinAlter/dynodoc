<script lang="ts">
  import type { EditorItem } from '$lib/types/editor';

  interface Props {
    item: EditorItem | undefined;
    suggesting: boolean;
    /** Commit an edit to a field of the selected node. */
    onEdit: (field: string, value: string) => Promise<void> | void;
  }

  let { item, suggesting, onEdit }: Props = $props();

  let draftText = $state('');
  let draftHint = $state('');
  let editingField = $state<'label' | 'hint' | null>(null);
  let saving = $state(false);

  // Reset drafts when the selected item changes.
  $effect(() => {
    draftText = item?.text ?? '';
    draftHint = item?.hint ?? '';
    editingField = null;
  });

  async function commit(field: 'label' | 'hint', value: string) {
    saving = true;
    try {
      await onEdit(field, value);
      editingField = null;
    } finally {
      saving = false;
    }
  }
</script>

<aside class="inspector">
  {#if item}
    <div class="overline">Selected item · {suggesting ? 'suggesting' : 'editing'}</div>
    <h2>{item.number}</h2>

    <section>
      <h3>Question text</h3>
      <textarea
        bind:value={draftText}
        rows="3"
        aria-label="Question text"
        onfocus={() => (editingField = 'label')}
      ></textarea>
      {#if editingField === 'label' && draftText !== item.text}
        <button
          type="button"
          class="save"
          disabled={saving}
          onclick={() => commit('label', draftText)}
        >
          {saving ? 'Saving…' : suggesting ? 'Propose change' : 'Save'}
        </button>
      {/if}
    </section>

    <section>
      <h3>Hint</h3>
      <input bind:value={draftHint} aria-label="Hint" onfocus={() => (editingField = 'hint')} />
      {#if editingField === 'hint' && draftHint !== (item.hint ?? '')}
        <button
          type="button"
          class="save"
          disabled={saving}
          onclick={() => commit('hint', draftHint)}
        >
          {saving ? 'Saving…' : suggesting ? 'Propose change' : 'Save'}
        </button>
      {/if}
    </section>

    <section>
      <h3>Properties</h3>
      <dl>
        <div>
          <dt>Type</dt>
          <dd>{item.type}</dd>
        </div>
        <div>
          <dt>Var name</dt>
          <dd>{item.varName ?? '—'}</dd>
        </div>
        <div>
          <dt>Relevance</dt>
          <dd><code>{item.relevance ?? '—'}</code></dd>
        </div>
      </dl>
    </section>

    <section>
      <h3>Discussion</h3>
      <ul class="meta-list">
        <li>{item.commentsCount ?? 0} comments</li>
        <li>{item.suggestionsCount ?? 0} pending suggestions</li>
      </ul>
    </section>
  {:else}
    <div class="empty">
      <h2>Inspector</h2>
      <p>Select a question to inspect and edit its fields.</p>
    </div>
  {/if}
</aside>

<style>
  .inspector {
    position: sticky;
    top: 5.5rem;
    align-self: start;
    border: 1px solid var(--color-border);
    border-radius: 1rem;
    background: var(--color-surface);
    padding: 1.2rem;
  }

  .overline,
  dt,
  .meta-list,
  button,
  textarea,
  input {
    font-family: var(--font-ui);
  }

  .overline {
    color: var(--color-muted);
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  h2,
  h3 {
    margin: 0.4rem 0 0.7rem;
    font-family: var(--font-display);
    font-weight: 500;
  }

  section + section {
    margin-top: 1rem;
    padding-top: 1rem;
    border-top: 1px solid var(--color-border);
  }

  textarea,
  input {
    width: 100%;
    border: 1px solid var(--color-border);
    border-radius: 0.6rem;
    padding: 0.6rem 0.7rem;
    background: var(--color-parchment);
    color: var(--color-ink);
    resize: vertical;
  }

  .save {
    margin-top: 0.5rem;
    border: 0;
    border-radius: 0.55rem;
    padding: 0.45rem 0.8rem;
    background: var(--color-ink);
    color: var(--color-surface);
    cursor: pointer;
  }

  .save:disabled {
    opacity: 0.6;
  }

  dl {
    margin: 0;
    display: grid;
    gap: 0.7rem;
  }

  dt {
    color: var(--color-subtle);
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin-bottom: 0.15rem;
  }

  dd {
    margin: 0;
  }

  code {
    font-family: var(--font-mono);
    color: var(--color-warn);
  }

  .meta-list {
    margin: 0;
    padding-left: 1rem;
    color: var(--color-muted);
  }

  .empty p {
    color: var(--color-muted);
    font-family: var(--font-ui);
  }
</style>
