<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { workspaceRequest } from '$lib/workspace/api';
  import { projectRoleName, type Project } from '$lib/workspace/projects';
  let projects = $state<Project[]>([]),
    name = $state(''),
    description = $state(''),
    error = $state(''),
    busy = $state(false),
    loaded = $state(false);
  onMount(async () => {
    try {
      projects = (await workspaceRequest<{ items: Project[] }>('/projects')).items;
    } catch (e) {
      error = String(e);
    } finally {
      loaded = true;
    }
  });
  async function create() {
    busy = true;
    error = '';
    try {
      const p = await workspaceRequest<{ id: string }>('/projects', { name, description });
      await goto(`/projects/${p.id}`);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not create the project';
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head><title>Projects · Dynodoc</title><meta name="robots" content="noindex" /></svelte:head>
<main>
  <header>
    <a href="/workspace">Dynodoc · Home</a>
    <h1>Projects</h1>
    <p>Edit in your own tools. Collect changes, review them and keep a shared history here.</p>
  </header>
  {#if error}<p role="alert">{error}</p>{/if}
  <section aria-labelledby="new-project">
    <h2 id="new-project">New project</h2>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void create();
      }}
    >
      <label>Project name<input bind:value={name} required maxlength="160" /></label>
      <label>Description<textarea bind:value={description} maxlength="2000"></textarea></label>
      <p>
        Only invited people can access this project. Editors propose changes through review by
        default.
      </p>
      <button disabled={busy}>{busy ? 'Creating…' : 'Create project'}</button>
    </form>
  </section>
  <section aria-labelledby="your-projects">
    <h2 id="your-projects">Your projects</h2>
    {#if !loaded}<p>Loading…</p>{:else if !projects.length}<p>
        No projects yet. Create one to start collecting your team's files and changes.
      </p>{/if}
    {#each projects as p}<article>
        <a href={`/projects/${p.id}`}>{p.name}</a><span>{projectRoleName(p.role)}</span>
        <p>{p.description}</p>
      </article>{/each}
  </section>
</main>

<style>
  main {
    max-width: 1000px;
    margin: auto;
    padding: 32px;
    color: var(--color-ink);
  }
  section {
    margin-top: 28px;
    padding: 24px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 12px;
  }
  label {
    display: grid;
    gap: 8px;
    margin: 12px 0;
  }
  input,
  textarea,
  button {
    font: inherit;
    padding: 10px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-surface);
    color: inherit;
  }
  button {
    cursor: pointer;
  }
  article {
    padding: 16px 0;
    border-top: 1px solid var(--color-border-soft);
  }
  article a {
    font-size: 20px;
  }
  span {
    margin-left: 16px;
  }
  a {
    color: var(--workspace-accent);
  }
</style>
