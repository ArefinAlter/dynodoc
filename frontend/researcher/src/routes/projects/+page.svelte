<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { FolderGit2, Plus, LockKeyhole, GitPullRequest, ArrowUpRight } from '@lucide/svelte';
  import RepositoryShell from '$lib/components/product/RepositoryShell.svelte';
  import { workspaceRequest } from '$lib/workspace/api';
  import { projectRoleName, type Project } from '$lib/workspace/projects';
  import { loadProfile, initials, type Profile } from '$lib/workspace/profile';
  let projects = $state<Project[]>([]),
    profile = $state<Profile | null>(null);
  let name = $state(''),
    description = $state(''),
    search = $state(''),
    error = $state('');
  let busy = $state(false),
    loaded = $state(false),
    creating = $state(false),
    owned = $state(false);
  const visible = $derived(
    projects.filter(
      (p) =>
        (!owned || p.role === 'owner') &&
        `${p.name} ${p.description}`.toLowerCase().includes(search.toLowerCase())
    )
  );
  onMount(async () => {
    search = $page.url.searchParams.get('q') || '';
    try {
      [projects, profile] = await Promise.all([
        workspaceRequest<{ items: Project[] }>('/projects').then((p) => p.items),
        loadProfile()
      ]);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not load your projects';
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

<svelte:head><title>Your projects · Dynodoc</title></svelte:head>
<RepositoryShell>
  <div class="repo-dashboard">
    <aside>
      <div class="repo-avatar" aria-hidden="true">{initials(profile?.name || '')}</div>
      <h1>{profile?.name || 'Your profile'}</h1>
      <p class="repo-muted">
        {profile?.bio || 'A place for your files, contributions and shared history.'}
      </p>
      <a class="repo-button" href="/profile"
        >{profile?.name ? 'Edit profile' : 'Set up your profile'}</a
      >
      <p class="repo-muted repo-small">
        Your profile is visible to people who share a project with you.
      </p>
      <hr />
      <p>
        <FolderGit2 size={15} />
        {projects.length} accessible {projects.length === 1 ? 'project' : 'projects'}
      </p>
      <a href="/connectors"
        >Connect Word, Excel or PowerPoint<br />and Google Docs, Sheets or Slides <ArrowUpRight
          size={14}
        /></a
      >
    </aside>
    <section aria-labelledby="projects-heading">
      <div class="repo-title">
        <h1 id="projects-heading">Projects</h1>
        <span class="repo-badge">{projects.length}</span>
        <div class="repo-actions">
          <button class="repo-primary" onclick={() => (creating = !creating)}
            ><Plus size={16} /> New project</button
          >
        </div>
      </div>
      <p class="repo-muted">
        Keep files together. Propose changes, review differences and merge into your team's shared
        version.
      </p>
      {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}
      {#if creating}
        <section class="repo-panel" aria-labelledby="new-project">
          <div class="repo-panel-body">
            <h2 id="new-project">New project</h2>
            <form
              onsubmit={(e) => {
                e.preventDefault();
                void create();
              }}
            >
              <label
                >Project name<input
                  bind:value={name}
                  required
                  maxlength="160"
                  placeholder="e.g. Annual report"
                /></label
              >
              <label
                >Description<textarea
                  bind:value={description}
                  maxlength="2000"
                  placeholder="What will your team work on?"
                ></textarea></label
              >
              <p class="repo-muted">
                <LockKeyhole size={14} /> Private · Only invited people can access this project.
              </p>
              <div class="repo-actions">
                <button class="repo-primary" disabled={busy}
                  >{busy ? 'Creating…' : 'Create project'}</button
                ><button type="button" onclick={() => (creating = false)}>Cancel</button>
              </div>
            </form>
          </div>
        </section>
      {/if}
      <div class="repo-toolbar">
        <input
          type="search"
          aria-label="Find a project"
          placeholder="Find a project…"
          bind:value={search}
        /><label><input type="checkbox" bind:checked={owned} /> Owned by me</label>
      </div>
      <div class="repo-panel">
        {#if !loaded}<div class="repo-empty" role="status">Loading projects…</div>
        {:else if !visible.length}<div class="repo-empty">
            <FolderGit2 size={32} />
            <h2>{projects.length ? 'No matching projects' : 'Your next project starts here'}</h2>
            <p>
              {projects.length
                ? 'Try another name or clear your filters.'
                : 'Create a project, add a file and invite your collaborators.'}
            </p>
            {#if !projects.length}<button onclick={() => (creating = true)}
                >Create your first project</button
              >{/if}
          </div>
        {/if}
        {#each visible as p}<article class="repo-row">
            <div class="repo-title" style="margin-bottom:0">
              <FolderGit2 size={19} />
              <h2><a href={`/projects/${p.id}`}>{p.name}</a></h2>
              <span class="repo-badge">Private</span>
            </div>
            <p class="repo-muted">{p.description || 'Files and changes shared with your team.'}</p>
            <div class="repo-actions repo-small">
              <span class="repo-muted">{projectRoleName(p.role)}</span><a
                href={`/projects/${p.id}?tab=requests`}
                ><GitPullRequest size={14} /> Pull requests</a
              >
            </div>
          </article>{/each}
      </div>
      <p class="repo-muted repo-small">
        Looking for a file outside a project? <a href="/workspace"
          >Open the web editors and file library.</a
        >
      </p>
    </section>
  </div>
</RepositoryShell>
