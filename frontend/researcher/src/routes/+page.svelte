<script lang="ts">
  import { font } from '$lib/tokens';
</script>

<main>
  <h1 style="font-family: {font.display}">dynodoc</h1>
  <p>The Research IDE — environment scaffolding is up.</p>
  <p class="muted">Stage 12 complete. The four-mode editor lands in stage 19.</p>
</main>

<style>
  main {
    max-width: 42rem;
    margin: 4rem auto;
    padding: 0 1.5rem;
  }
  h1 {
    color: var(--color-terracotta);
    font-size: 3rem;
    margin-bottom: 0.5rem;
  }
  .muted {
    color: var(--color-muted);
    font-family: var(--font-ui);
    font-size: 0.9rem;
  }
</style>
