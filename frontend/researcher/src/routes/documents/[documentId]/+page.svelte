<script lang="ts">
  import { onDestroy } from 'svelte';
  import { DocumentStore } from '$lib/stores/documentStore.svelte';
  import WorkspaceEditor from '$lib/components/workspace/WorkspaceEditor.svelte';
  import { workspaceRequest } from '$lib/workspace/api';

  let { params } = $props();

  let store = $state<DocumentStore | null>(null);
  let project = $state<{ id: string; name: string } | null>(null);

  // Recreate + load the store whenever the route's document id changes.
  $effect(() => {
    const id = params.documentId;
    const next = new DocumentStore(id);
    store = next;
    project = null;
    let active = true;
    void workspaceRequest<{ project?: { id: string; name: string } | null }>(
      `/documents/${id}/access`
    )
      .then((access) => {
        if (active) project = access.project || null;
      })
      .catch(() => {});
    void next.load();
    return () => {
      active = false;
      next.dispose();
    };
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
    {#if project}<nav class="project-trail" aria-label="Project">
        <a href={`/projects/${project.id}`}>← {project.name}</a><span> / {store.detail.title}</span>
      </nav>{/if}
    <WorkspaceEditor {store} />
  {/if}
{/if}

<style>
  .project-trail {
    padding: 6px 18px;
    border-bottom: 1px solid var(--color-border);
    color: var(--color-ink);
    background: var(--color-surface-sunken);
    font-size: 13px;
  }
  .project-trail a {
    color: var(--workspace-accent);
  }
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
