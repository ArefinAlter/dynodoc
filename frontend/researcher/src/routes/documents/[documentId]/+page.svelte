<script lang="ts">
  import { onDestroy } from 'svelte';
  import { DocumentStore } from '$lib/stores/documentStore.svelte';
  import WorkspaceEditor from '$lib/components/workspace/WorkspaceEditor.svelte';

  let { params } = $props();

  let store = $state<DocumentStore | null>(null);

  // Recreate + load the store whenever the route's document id changes.
  $effect(() => {
    const id = params.documentId;
    const next = new DocumentStore(id);
    store = next;
    void next.load();
    return () => next.dispose();
  });

  onDestroy(() => store?.dispose());
</script>

{#if store}
  {#if store.loading}
    <div class="state">Loading document…</div>
  {:else if store.error}
    <div class="state error">
      {store.error}
      <a class="back" href="/workspace">← Back to documents</a>
    </div>
  {:else if store.detail}
    <WorkspaceEditor {store} />
  {/if}
{/if}

<style>
  .state {
    max-width: 40rem;
    margin: 6rem auto;
    padding: 0 1.5rem;
    font-family: var(--font-ui);
    color: var(--color-muted);
  }

  .state.error {
    color: var(--color-terracotta);
  }

  .back {
    display: inline-block;
    margin-top: 1rem;
  }
</style>
