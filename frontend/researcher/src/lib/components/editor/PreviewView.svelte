<script lang="ts">
  import type { EditorDocument } from '$lib/types/editor';

  interface Props {
    document: EditorDocument;
  }

  let { document }: Props = $props();
  const firstQuestion = $derived(
    document.sections.flatMap((section) => section.items).find((item) => item.choices?.length)
  );
</script>

<section class="preview">
  <div class="frame">
    <div class="frame-head">
      <div>
        <div class="label">Preview</div>
        <strong>Respondent-facing shell</strong>
      </div>
      <span>PoC-safe placeholder</span>
    </div>

    {#if firstQuestion}
      <div class="question">
        <div class="count">
          Question 1 of {document.sections.flatMap((section) => section.items).length}
        </div>
        <h2>{firstQuestion.text}</h2>
        <div class="choices">
          {#each firstQuestion.choices ?? [] as choice}
            <div class="choice">
              <span class="dot"></span>
              {choice.label}
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
</section>

<style>
  .frame {
    max-width: 42rem;
    padding: 1.25rem;
    border: 1px solid var(--color-border);
    border-radius: 1.2rem;
    background: var(--color-surface);
    box-shadow: var(--shadow-panel);
  }

  .frame-head {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
    margin-bottom: 1rem;
    padding-bottom: 1rem;
    border-bottom: 1px solid var(--color-border);
    font-family: var(--font-ui);
  }

  .label,
  .count {
    color: var(--color-muted);
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  h2 {
    margin: 0.5rem 0 1rem;
    font-family: var(--font-ui);
    font-size: 1.25rem;
  }

  .choices {
    display: grid;
    gap: 0.75rem;
  }

  .choice {
    display: flex;
    gap: 0.75rem;
    align-items: center;
    padding: 0.95rem 1rem;
    border: 1px solid var(--color-border);
    border-radius: 0.9rem;
    font-family: var(--font-ui);
  }

  .dot {
    width: 1rem;
    height: 1rem;
    border: 1.5px solid var(--color-subtle);
    border-radius: 999px;
  }
</style>
