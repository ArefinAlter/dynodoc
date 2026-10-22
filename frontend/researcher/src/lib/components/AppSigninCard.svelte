<script lang="ts">
  import type { Snippet } from 'svelte';
  import { page } from '$app/stores';
  // The frame of the two pages that sign a Dynodoc app in (/app-signin and /app-signin/finish):
  // one plain card, nothing else on the page.
  let { title, children }: { title: string; children: Snippet } = $props();
  const site = $derived($page.data.site);
</script>

<svelte:head
  ><title>{title} · {site.brand}</title><meta name="robots" content="noindex" /></svelte:head
>
<main class="auth-page">
  <section class="card" aria-labelledby="app-signin-title">
    <img src="/brand/mark.svg?v=2" width="52" height="52" alt="" />
    <h1 id="app-signin-title">{title}</h1>
    {@render children()}
  </section>
</main>

<style>
  .auth-page {
    min-height: 100vh;
    min-height: 100dvh;
    background: var(--color-parchment);
    font-family: var(--font-ui);
    padding: 32px;
  }
  .card {
    margin: 40px auto 24px;
    max-width: 460px;
    background: var(--color-surface-raised);
    padding: 40px;
    border: 1px solid var(--color-border-soft);
    border-radius: 20px;
    box-shadow: var(--shadow-panel);
    color: var(--color-ink);
    font-size: 15px;
    line-height: 1.6;
  }
  h1 {
    font: 600 28px/1.2 var(--font-ui);
    letter-spacing: -0.5px;
    color: var(--color-ink-strong);
    margin: 20px 0 14px;
  }
  .card :global(p) {
    margin: 0 0 14px;
  }
  .card :global(.fine) {
    font-size: 13px;
    color: var(--color-ink-soft);
  }
  .card :global(.who) {
    background: var(--color-parchment);
    border: 1px solid var(--color-border-soft);
    border-radius: 10px;
    padding: 12px 14px;
    overflow-wrap: anywhere;
  }
  .card :global(.actions) {
    display: grid;
    gap: 10px;
    margin-top: 20px;
  }
  .card :global(button),
  .card :global(a.button) {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    min-height: 46px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    font: 500 14px var(--font-ui);
    color: var(--color-ink);
    background: var(--color-surface-raised);
    text-decoration: none;
    cursor: pointer;
  }
  .card :global(button.primary) {
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
    border-color: var(--color-accent-solid);
  }
  .card :global(button:disabled) {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .card :global(.error) {
    color: var(--color-danger);
    background: var(--color-danger-bg);
    padding: 12px;
    border-radius: 8px;
    font-size: 13px;
  }
  .card :global(.notice) {
    color: var(--color-success);
    background: var(--color-success-bg);
    padding: 12px 14px;
    border-radius: 10px;
    font-size: 14px;
  }
  @media (max-width: 600px) {
    .auth-page {
      padding: 20px;
    }
    .card {
      padding: 28px 22px;
      margin-top: 28px;
    }
  }
</style>
