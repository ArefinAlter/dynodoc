<script lang="ts">
  import type { EditorDocument, EditorItem } from '$lib/types/editor';

  interface Props {
    document: EditorDocument;
    selectedId: string;
    onSelect: (item: EditorItem) => void;
  }

  let { document, selectedId, onSelect }: Props = $props();
</script>

<div class="outline">
  <div class="tree">
    <h2>Outline</h2>
    {#each document.sections as section}
      <div class="section">
        <div class="section-title">{section.number}. {section.title}</div>
        <div class="children">
          {#each section.items as item}
            <button
              type="button"
              class:selected={item.id === selectedId}
              onclick={() => onSelect(item)}
            >
              <span>{item.number}</span>
              <strong>{item.varName ?? item.type}</strong>
            </button>
          {/each}
        </div>
      </div>
    {/each}
  </div>

  <div class="detail">
    <h3>Why this view is PoC-safe</h3>
    <p>
      Outline mode can ship early because it depends only on stable node IDs, ordering, and the
      current materialized tree. Drag-reorder can be added later once move events are wired
      end-to-end.
    </p>
  </div>
</div>

<style>
  .outline {
    display: grid;
    grid-template-columns: minmax(18rem, 24rem) 1fr;
    gap: 1rem;
  }

  .tree,
  .detail {
    border: 1px solid var(--color-border);
    border-radius: 1rem;
    background: var(--color-surface);
    padding: 1rem;
  }

  h2,
  h3 {
    margin: 0 0 0.75rem;
    font-family: var(--font-display);
    font-weight: 500;
  }

  .section + .section {
    margin-top: 1rem;
  }

  .section-title {
    font-family: var(--font-ui);
    color: var(--color-muted);
    font-size: 0.82rem;
    margin-bottom: 0.4rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .children {
    display: grid;
    gap: 0.35rem;
  }

  button {
    display: grid;
    grid-template-columns: 3.5rem 1fr;
    gap: 0.6rem;
    align-items: center;
    border: 0;
    border-radius: 0.7rem;
    padding: 0.7rem 0.8rem;
    background: transparent;
    text-align: left;
    color: inherit;
  }

  button.selected,
  button:hover {
    background: var(--color-accent-bg);
  }

  span,
  strong {
    font-family: var(--font-ui);
  }

  span {
    color: var(--color-subtle);
    font-size: 0.78rem;
  }

  strong {
    font-weight: 500;
  }

  @media (max-width: 960px) {
    .outline {
      grid-template-columns: 1fr;
    }
  }
</style>
