<script lang="ts">
  import type { EditorDocument, EditorItem } from '$lib/types/editor';

  interface Props {
    document: EditorDocument;
    selectedId: string;
    suggesting: boolean;
    onSelect: (item: EditorItem) => void;
  }

  let { document, selectedId, suggesting, onSelect }: Props = $props();
</script>

<div class="document-view">
  <section class="hero">
    <div class="overline">{document.overline}</div>
    <h1>{document.title}</h1>
    <p class="meta">
      <strong>{document.organization}</strong>
      <span>·</span>
      <span>{document.status}</span>
      <span>·</span>
      <span>{document.languages.join(', ')}</span>
    </p>
    {#if suggesting}
      <div class="banner">Suggesting mode is on. Edits should create proposals, not direct canonical changes.</div>
    {/if}
  </section>

  {#each document.sections as section}
    <section class="section">
      <div class="section-head">
        <div>
          <div class="section-num">{section.number}</div>
          <h2>{section.title}</h2>
        </div>
        {#if section.meta}
          <div class="section-meta">{section.meta}</div>
        {/if}
      </div>

      {#if section.moduleTag}
        <div class="module-banner">
          <span class="module-tag">{section.moduleTag}</span>
          <div>
            <strong>{section.moduleName}</strong>
            <div>{section.moduleCitation}</div>
          </div>
        </div>
      {/if}

      <div class="items">
        {#each section.items as item}
          <button type="button" class:selected={item.id === selectedId} class="item" onclick={() => onSelect(item)}>
            <div class="gutter">
              <div class="number">{item.number}</div>
              <div class="counts">
                {#if item.commentsCount}
                  <span>{item.commentsCount} c</span>
                {/if}
                {#if item.suggestionsCount}
                  <span>{item.suggestionsCount} s</span>
                {/if}
              </div>
            </div>

            <div class="body">
              <div class="item-meta">
                <span class="pill">{item.type}</span>
                {#if item.relevance}
                  <span class="logic">{item.relevance}</span>
                {/if}
              </div>
              <div class="text">{item.text}</div>
              {#if item.hint}
                <div class="hint">{item.hint}</div>
              {/if}
              {#if item.choices?.length}
                <ul class="choices">
                  {#each item.choices as choice}
                    <li>{choice.label}</li>
                  {/each}
                </ul>
              {/if}
            </div>
          </button>
        {/each}
      </div>
    </section>
  {/each}
</div>

<style>
  .hero {
    margin-bottom: 3rem;
    padding-bottom: 1.5rem;
    border-bottom: 1px solid var(--color-border);
  }

  .overline,
  .meta,
  .section-meta,
  .counts,
  .item-meta,
  .banner {
    font-family: var(--font-ui);
  }

  .overline {
    color: var(--color-muted);
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }

  h1 {
    margin: 0.65rem 0;
    font-family: var(--font-display);
    font-size: clamp(2.2rem, 3vw, 3rem);
    line-height: 1.08;
    font-weight: 500;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.55rem;
    color: var(--color-muted);
    font-size: 0.9rem;
  }

  .banner {
    margin-top: 1rem;
    padding: 0.8rem 1rem;
    border-radius: 0.75rem;
    background: var(--color-suggest-bg);
    color: var(--color-suggest);
    font-size: 0.86rem;
  }

  .section {
    margin-bottom: 2.5rem;
  }

  .section-head {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: end;
    margin-bottom: 1rem;
    padding-bottom: 0.8rem;
    border-bottom: 1px solid var(--color-border);
  }

  .section-num {
    color: var(--color-accent);
    font-family: var(--font-display);
    font-style: italic;
    margin-bottom: 0.25rem;
  }

  h2 {
    margin: 0;
    font-family: var(--font-display);
    font-size: 1.5rem;
    font-weight: 500;
  }

  .section-meta {
    color: var(--color-subtle);
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .module-banner {
    display: flex;
    gap: 0.9rem;
    align-items: center;
    padding: 0.9rem 1rem;
    margin-bottom: 1rem;
    background: var(--color-accent-bg);
    border-left: 3px solid var(--color-accent);
    border-radius: 0 0.6rem 0.6rem 0;
    font-family: var(--font-ui);
  }

  .module-tag {
    padding: 0.2rem 0.45rem;
    border-radius: 0.3rem;
    background: var(--color-accent);
    color: white;
    font-size: 0.72rem;
    letter-spacing: 0.04em;
  }

  .items {
    display: grid;
    gap: 0.65rem;
  }

  .item {
    display: grid;
    grid-template-columns: 4rem 1fr;
    gap: 1rem;
    width: 100%;
    padding: 1rem;
    border: 0;
    border-radius: 0.9rem;
    background: transparent;
    text-align: left;
    color: inherit;
  }

  .item:hover {
    background: color-mix(in srgb, var(--color-accent-bg) 42%, transparent);
  }

  .item.selected {
    background: var(--color-surface);
    box-shadow: inset 4px 0 0 var(--color-accent), 0 1px 2px rgba(0, 0, 0, 0.05);
  }

  .number {
    color: var(--color-subtle);
    font-family: var(--font-ui);
    font-size: 0.85rem;
  }

  .counts {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    margin-top: 0.45rem;
    color: var(--color-subtle);
    font-size: 0.7rem;
  }

  .body {
    min-width: 0;
  }

  .item-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-bottom: 0.45rem;
    color: var(--color-subtle);
    font-size: 0.75rem;
  }

  .pill {
    padding: 0.1rem 0.45rem;
    border-radius: 999px;
    background: var(--color-surface-muted);
  }

  .logic {
    font-family: var(--font-mono);
    color: var(--color-warn);
  }

  .text {
    font-size: 1.08rem;
    line-height: 1.55;
  }

  .hint {
    margin-top: 0.45rem;
    color: var(--color-muted);
    font-family: var(--font-ui);
    font-size: 0.84rem;
  }

  .choices {
    margin: 0.7rem 0 0 0;
    padding-left: 1.1rem;
    color: var(--color-ink);
  }
</style>
