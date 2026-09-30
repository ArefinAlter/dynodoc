<script lang="ts">
  import type { EditorDocument } from '$lib/types/editor';

  interface Props {
    document: EditorDocument;
  }

  let { document }: Props = $props();
</script>

<section class="spec">
  <div class="intro">
    <div class="label">Structured Spec</div>
    <h2>Technical fields over the same canonical content</h2>
    <p>
      This PoC-safe version shows a structured table for authors who think in terms of variable
      names and logic. It is intentionally not presented as live XLSForm export yet.
    </p>
  </div>

  <table>
    <thead>
      <tr>
        <th>No.</th>
        <th>Var name</th>
        <th>Type</th>
        <th>Prompt</th>
        <th>Relevance</th>
      </tr>
    </thead>
    <tbody>
      {#each document.sections as section}
        <tr class="section-row">
          <td colspan="5">{section.number}. {section.title}</td>
        </tr>
        {#each section.items as item}
          <tr>
            <td>{item.number}</td>
            <td>{item.varName ?? '—'}</td>
            <td>{item.type}</td>
            <td>{item.text}</td>
            <td><code>{item.relevance ?? '—'}</code></td>
          </tr>
        {/each}
      {/each}
    </tbody>
  </table>
</section>

<style>
  .intro {
    margin-bottom: 1rem;
  }

  .label {
    color: var(--color-muted);
    font-family: var(--font-ui);
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }

  h2 {
    margin: 0.45rem 0;
    font-family: var(--font-display);
    font-weight: 500;
  }

  p {
    max-width: 52rem;
    color: var(--color-muted);
    font-family: var(--font-ui);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 1rem;
    overflow: hidden;
  }

  th,
  td {
    padding: 0.8rem;
    border-bottom: 1px solid var(--color-border);
    text-align: left;
    vertical-align: top;
  }

  th {
    background: var(--color-surface-muted);
    font-family: var(--font-ui);
    font-size: 0.8rem;
  }

  .section-row td {
    font-family: var(--font-ui);
    color: var(--color-subtle);
    background: color-mix(in srgb, var(--color-accent-bg) 28%, var(--color-surface-raised));
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.76rem;
  }

  code {
    font-family: var(--font-mono);
    color: var(--color-warn);
  }
</style>
