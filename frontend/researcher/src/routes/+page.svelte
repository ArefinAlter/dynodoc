<script lang="ts">
  import { goto } from '$app/navigation';
  import { createDocument, listDocuments } from '$lib/api/client';
  import type { Document } from '$lib/api/types';
  import { isAuthenticated, clearSession } from '$lib/stores/session.svelte';
  import { pushToast } from '$lib/stores/toast.svelte';

  let documents = $state<Document[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let creating = $state(false);
  let newTitle = $state('');

  async function refresh() {
    loading = true;
    error = null;
    try {
      const page = await listDocuments({ limit: 100 });
      documents = page.items;
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not load documents';
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void refresh();
  });

  async function create(event: SubmitEvent) {
    event.preventDefault();
    if (!newTitle.trim()) return;
    if (!isAuthenticated()) {
      await goto('/login?next=/');
      return;
    }
    creating = true;
    try {
      const doc = await createDocument({ title: newTitle.trim() });
      newTitle = '';
      await goto(`/documents/${doc.id}`);
    } catch (e) {
      pushToast(e instanceof Error ? e.message : 'Create failed', 'error');
    } finally {
      creating = false;
    }
  }

  function logout() {
    clearSession();
    pushToast('Signed out.', 'info');
  }
</script>

<main>
  <header class="head">
    <div>
      <div class="eyebrow">dynodoc · researcher</div>
      <h1>Documents</h1>
    </div>
    <div class="head-actions">
      {#if isAuthenticated()}
        <button type="button" class="ghost" onclick={logout}>Sign out</button>
      {:else}
        <a class="ghost" href="/login">Sign in</a>
      {/if}
    </div>
  </header>

  <form class="create" onsubmit={create}>
    <input
      bind:value={newTitle}
      placeholder="New questionnaire title…"
      aria-label="New document title"
    />
    <button class="primary" type="submit" disabled={creating}>
      {creating ? 'Creating…' : 'Create document'}
    </button>
  </form>

  {#if loading}
    <p class="muted">Loading documents…</p>
  {:else if error}
    <p class="error">
      {error} · <button type="button" class="link" onclick={refresh}>retry</button>
    </p>
  {:else if documents.length === 0}
    <p class="muted">No documents yet. Create one to get started.</p>
  {:else}
    <ul class="doc-list">
      {#each documents as doc (doc.id)}
        <li>
          <a class="doc" href="/documents/{doc.id}">
            <div class="doc-title">{doc.title}</div>
            <div class="doc-meta">
              <span class="status {doc.status}">{doc.status}</span>
              <span>{doc.languages.join(', ') || 'en'}</span>
              <span class="id">{doc.id.slice(0, 8)}</span>
            </div>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
</main>

<style>
  main {
    max-width: 60rem;
    margin: 4rem auto;
    padding: 0 1.5rem;
  }

  .head {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 2rem;
  }

  .eyebrow {
    font-family: var(--font-ui);
    color: var(--color-muted);
    font-size: 0.76rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }

  h1 {
    font-family: var(--font-display);
    color: var(--color-terracotta);
    font-size: clamp(2.2rem, 4vw, 3.2rem);
    margin: 0.4rem 0 0;
    font-weight: 500;
  }

  .head-actions a,
  .head-actions button,
  .create,
  .muted,
  .error,
  .doc-meta {
    font-family: var(--font-ui);
  }

  .ghost {
    border: 1px solid var(--color-border);
    border-radius: 0.7rem;
    padding: 0.6rem 0.9rem;
    background: transparent;
    color: var(--color-muted);
    text-decoration: none;
    cursor: pointer;
  }

  .create {
    display: flex;
    gap: 0.6rem;
    margin-bottom: 2rem;
  }

  .create input {
    flex: 1;
    border: 1px solid var(--color-border);
    border-radius: 0.7rem;
    padding: 0.7rem 0.9rem;
    background: var(--color-surface);
    color: var(--color-ink);
  }

  .primary {
    border: 0;
    border-radius: 0.7rem;
    padding: 0.7rem 1rem;
    background: var(--color-ink);
    color: var(--color-surface);
    cursor: pointer;
  }

  .primary:disabled {
    opacity: 0.6;
  }

  .doc-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.6rem;
  }

  .doc {
    display: block;
    border: 1px solid var(--color-border);
    border-radius: 0.9rem;
    padding: 1rem 1.2rem;
    background: var(--color-surface);
    text-decoration: none;
    color: inherit;
  }

  .doc:hover {
    border-color: var(--color-border-strong);
  }

  .doc-title {
    font-family: var(--font-display);
    font-size: 1.25rem;
    margin-bottom: 0.4rem;
  }

  .doc-meta {
    display: flex;
    gap: 0.7rem;
    align-items: center;
    color: var(--color-muted);
    font-size: 0.82rem;
  }

  .status {
    padding: 0.1rem 0.5rem;
    border-radius: 999px;
    background: var(--color-surface-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 0.7rem;
  }

  .status.deployed {
    background: var(--color-suggest-bg);
    color: var(--color-suggest);
  }

  .id {
    font-family: var(--font-mono);
    color: var(--color-subtle);
  }

  .muted {
    color: var(--color-muted);
  }

  .error {
    color: var(--color-terracotta);
  }

  .link {
    background: none;
    border: 0;
    color: var(--color-terracotta);
    text-decoration: underline;
    cursor: pointer;
    padding: 0;
  }
</style>
