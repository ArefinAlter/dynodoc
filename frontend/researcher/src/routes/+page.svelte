<script lang="ts">
  import { font } from '$lib/tokens';
</script>

<main>
  <div class="eyebrow">dynodoc · researcher frontend</div>
  <h1 style="font-family: {font.display}">Research IDE scaffold</h1>
  <p>
    The frontend now includes a PoC-safe editor shell with Document, Outline, Spec, and Preview
    modes, plus inspector, activity, and conflict-resolution scaffolding.
  </p>
  <div class="actions">
    <a class="primary" href="/documents/demo-phq9">Open sample document</a>
    <span class="muted">Agent and export actions are intentionally placeholders for now.</span>
  </div>
</main>

<style>
  main {
    max-width: 52rem;
    margin: 5rem auto;
    padding: 0 1.5rem;
  }

  .eyebrow,
  .muted,
  .actions {
    font-family: var(--font-ui);
  }

  .eyebrow {
    color: var(--color-muted);
    font-size: 0.76rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }

  h1 {
    color: var(--color-terracotta);
    font-size: clamp(2.5rem, 4vw, 4rem);
    margin: 0.5rem 0 0.75rem;
  }

  p {
    max-width: 44rem;
    font-size: 1.1rem;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
    align-items: center;
    margin-top: 2rem;
  }

  .primary {
    display: inline-flex;
    align-items: center;
    padding: 0.8rem 1rem;
    border-radius: 0.8rem;
    background: var(--color-ink);
    color: var(--color-surface);
    text-decoration: none;
  }

  .muted {
    color: var(--color-muted);
    font-size: 0.92rem;
  }
</style>
