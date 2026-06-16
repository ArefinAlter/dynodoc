<script lang="ts">
  import type { EditorItem } from '$lib/types/editor';

  interface Props {
    item: EditorItem | undefined;
  }

  let { item }: Props = $props();
</script>

<aside class="inspector">
  {#if item}
    <div class="overline">Selected item</div>
    <h2>{item.number}</h2>
    <p class="lede">{item.text}</p>

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
          <dt>Hint</dt>
          <dd>{item.hint ?? '—'}</dd>
        </div>
        <div>
          <dt>Relevance</dt>
          <dd><code>{item.relevance ?? '—'}</code></dd>
        </div>
      </dl>
    </section>

    <section>
      <h3>Discussion</h3>
      <p>
        This panel is safe to build now because comments and suggestions are item-scoped in the PoC.
      </p>
      <ul class="meta-list">
        <li>{item.commentsCount ?? 0} comments</li>
        <li>{item.suggestionsCount ?? 0} suggestions</li>
      </ul>
    </section>

    <section class="placeholder">
      <h3>Later-stage placeholders</h3>
      <button type="button" disabled>Ask agent</button>
      <button type="button" disabled>Export this item</button>
    </section>
  {:else}
    <div class="empty">
      <h2>Inspector</h2>
      <p>Select a question to inspect its typed fields, discussion state, and history.</p>
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
  button {
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

  .lede {
    margin-bottom: 1rem;
  }

  section + section {
    margin-top: 1rem;
    padding-top: 1rem;
    border-top: 1px solid var(--color-border);
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

  .placeholder {
    display: grid;
    gap: 0.5rem;
  }

  button {
    border: 1px solid var(--color-border);
    border-radius: 0.7rem;
    padding: 0.7rem 0.9rem;
    background: var(--color-surface-muted);
    color: var(--color-subtle);
    text-align: left;
  }

  .empty p {
    color: var(--color-muted);
    font-family: var(--font-ui);
  }
</style>
