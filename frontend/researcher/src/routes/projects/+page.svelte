<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import {
    FolderGit2,
    Plus,
    LockKeyhole,
    GitPullRequest,
    ArrowUpRight,
    Search,
    ChevronDown,
    Check
  } from '@lucide/svelte';
  import RepositoryShell from '$lib/components/product/RepositoryShell.svelte';
  import Avatar from '$lib/components/product/repo/Avatar.svelte';
  import FormatsBar from '$lib/components/product/repo/FormatsBar.svelte';
  import Menu from '$lib/components/product/repo/Menu.svelte';
  import ProfileNav from '$lib/components/product/repo/ProfileNav.svelte';
  import { workspaceRequest } from '$lib/workspace/api';
  import { mapPool, summarizeProject, type ProjectSummary } from '$lib/workspace/activity';
  import { projectRoleName, type Project } from '$lib/workspace/projects';
  import { loadProfile, type Profile } from '$lib/workspace/profile';
  import { plural, relativeTime } from '$lib/workspace/repo-ui';
  type Kind = 'all' | 'owned' | 'shared';
  type Sort = 'created' | 'updated' | 'name';
  const KINDS: [Kind, string][] = [
    ['all', 'All'],
    ['owned', 'Owned by me'],
    ['shared', 'Shared with me']
  ];
  const SORTS: [Sort, string][] = [
    ['created', 'Newest first'],
    ['updated', 'Last updated'],
    ['name', 'Name']
  ];
  let projects = $state<Project[]>([]),
    profile = $state<Profile | null>(null),
    summaries = $state<Record<string, ProjectSummary>>({});
  let name = $state(''),
    description = $state(''),
    search = $state(''),
    error = $state(''),
    kind = $state<Kind>('all'),
    sort = $state<Sort>('created'),
    limit = $state(20);
  let busy = $state(false),
    loaded = $state(false),
    creating = $state(false);
  const visible = $derived.by(() => {
    const q = search.toLowerCase();
    const list = projects.filter(
      (p) =>
        (kind === 'all' || (kind === 'owned') === (p.role === 'owner')) &&
        `${p.name} ${p.description}`.toLowerCase().includes(q)
    );
    if (sort === 'name') return [...list].sort((a, b) => a.name.localeCompare(b.name));
    if (sort === 'updated') {
      const at = (p: Project) => Date.parse(summaries[p.id]?.updated_at || '') || 0;
      return [...list].sort((a, b) => at(b) - at(a));
    }
    return list;
  });
  const shown = $derived(visible.slice(0, limit));
  // The header's New project link lands here, also when this page is already open.
  $effect(() => {
    if ($page.url.searchParams.get('new') === '1') creating = true;
  });
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
  // Formats, update times and open pull requests load quietly for the rows on screen.
  const requested = new Set<string>();
  $effect(() => {
    const wanted = shown.map((p) => p.id).filter((projectId) => !requested.has(projectId));
    if (!wanted.length) return;
    wanted.forEach((projectId) => requested.add(projectId));
    void mapPool(wanted, 4, async (projectId) => {
      summaries[projectId] = await summarizeProject(projectId);
    });
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
  <div class="rp-band slim">
    <div class="repo-wrap">
      <ProfileNav current="projects" count={loaded ? projects.length : null} />
    </div>
  </div>
  <div class="repo-wrap rp-body">
    <div class="rp-dashboard">
      <aside class="rp-card" aria-label="Your profile">
        <div class="rp-card-avatar">
          <Avatar name={profile?.name || ''} size={96} />
        </div>
        <h2>{profile?.name || 'Your profile'}</h2>
        <p class="repo-muted rp-bio">
          {profile?.bio || 'A place for your files, contributions and shared history.'}
        </p>
        <a class="repo-btn block" href="/profile"
          >{profile?.name ? 'Edit profile' : 'Set up your profile'}</a
        >
        <p class="repo-muted repo-small">
          Your profile is visible to people who share a project with you.
        </p>
        <hr />
        <p class="rp-fact">
          <FolderGit2 size={15} aria-hidden="true" />
          {projects.length} accessible {projects.length === 1 ? 'project' : 'projects'}
        </p>
        <a class="rp-fact" href="/connectors"
          ><span>Connect Word, Excel or PowerPoint<br />and Google Docs, Sheets or Slides</span>
          <ArrowUpRight size={14} aria-hidden="true" /></a
        >
      </aside>
      <section aria-labelledby="projects-heading">
        <h1 id="projects-heading" class="sr-only">Projects</h1>
        {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}
        {#if creating}
          <section class="repo-box" aria-labelledby="new-project">
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
                  <LockKeyhole size={14} aria-hidden="true" /> Private · Only invited people can access
                  this project.
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
        <div class="rp-filters">
          <label class="rp-search"
            ><span class="sr-only">Find a project</span><Search
              size={15}
              aria-hidden="true"
            /><input
              type="search"
              placeholder="Find a project…"
              aria-label="Find a project"
              bind:value={search}
            /></label
          >
          <Menu title="Filter by type">
            {#snippet trigger()}Type: {KINDS.find((k) => k[0] === kind)?.[1]}<ChevronDown
                size={13}
                aria-hidden="true"
              />{/snippet}
            {#each KINDS as [value, label]}<button
                type="button"
                role="menuitemradio"
                aria-checked={kind === value}
                onclick={() => (kind = value)}
                ><span class="menu-check-slot"
                  >{#if kind === value}<Check size={14} aria-hidden="true" />{/if}</span
                >{label}</button
              >{/each}
          </Menu>
          <Menu title="Sort projects" align="right">
            {#snippet trigger()}Sort: {SORTS.find((s) => s[0] === sort)?.[1]}<ChevronDown
                size={13}
                aria-hidden="true"
              />{/snippet}
            {#each SORTS as [value, label]}<button
                type="button"
                role="menuitemradio"
                aria-checked={sort === value}
                onclick={() => (sort = value)}
                ><span class="menu-check-slot"
                  >{#if sort === value}<Check size={14} aria-hidden="true" />{/if}</span
                >{label}</button
              >{/each}
          </Menu>
          <button class="repo-primary" onclick={() => (creating = !creating)}
            ><Plus size={16} aria-hidden="true" /> New project</button
          >
        </div>
        <div class="rp-list" aria-busy={!loaded}>
          {#if !loaded}
            {#each [0, 1, 2] as n (n)}<div class="rp-proj" aria-hidden="true">
                <span class="repo-skel" style:width="{26 + n * 9}%"></span>
                <span class="repo-skel" style:width="{55 - n * 8}%"></span>
                <span class="repo-skel" style:width="22%"></span>
              </div>{/each}
          {:else if !shown.length}<div class="repo-empty">
              <FolderGit2 size={32} aria-hidden="true" />
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
          {#each shown as p (p.id)}
            {@const s = summaries[p.id]}
            <article class="rp-proj">
              <div class="rp-proj-head">
                <FolderGit2 size={17} aria-hidden="true" />
                <h2><a href={`/projects/${p.id}`}>{p.name}</a></h2>
                <span class="repo-badge">Private</span>
              </div>
              <p class="repo-muted rp-proj-desc">
                {p.description || 'Files and changes shared with your team.'}
              </p>
              <div class="rp-proj-meta">
                {#if s?.formats.length}<FormatsBar variant="dots" shares={s.formats} />{/if}
                {#if s?.updated_at}<span>Updated {relativeTime(s.updated_at)}</span>{/if}
                {#if s}<span>{plural(s.files, 'file')}{s.filesMore ? '+' : ''}</span>{/if}
                <span class="repo-role">{projectRoleName(p.role)}</span>
                <a href={`/projects/${p.id}?tab=requests`}
                  ><GitPullRequest size={14} aria-hidden="true" />
                  {s?.open_requests ? `${s.open_requests} open` : 'Pull requests'}</a
                >
              </div>
            </article>
          {/each}
          {#if visible.length > limit}<div class="repo-pagination">
              <button onclick={() => (limit += 20)}>Show more projects</button>
            </div>{/if}
        </div>
        <p class="repo-muted repo-small">
          Looking for a file outside a project? <a href="/workspace"
            >Open the web editors and file library.</a
          >
        </p>
      </section>
    </div>
  </div>
</RepositoryShell>
