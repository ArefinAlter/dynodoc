<script lang="ts">
  import { onMount } from 'svelte';
  import { FolderGit2, LockKeyhole, GitPullRequest, CalendarDays } from '@lucide/svelte';
  import { workspaceRequest } from '$lib/workspace/api';
  import { loadProfile, type Profile } from '$lib/workspace/profile';
  import { projectRoleName, type Project } from '$lib/workspace/projects';
  import {
    loadActivity,
    loadProfileActivity,
    mapPool,
    summarizeProject,
    type ActivityDay,
    type ActivityItem,
    type MemberLite,
    type ProjectSummary
  } from '$lib/workspace/activity';
  import { plural } from '$lib/workspace/repo-ui';
  import RepositoryShell from './RepositoryShell.svelte';
  import DeviceKeys from './DeviceKeys.svelte';
  import Avatar from './repo/Avatar.svelte';
  import ActivityFeed from './repo/ActivityFeed.svelte';
  import ContributionGraph from './repo/ContributionGraph.svelte';
  import FormatsBar from './repo/FormatsBar.svelte';
  import ProfileNav from './repo/ProfileNav.svelte';
  let { id }: { id?: string } = $props();
  let storage = $state<{
    bytes_used: number;
    byte_limit: number;
    objects_used: number;
    object_limit: number;
  } | null>(null);
  const thisYear = new Date().getFullYear();
  let profile = $state<Profile | null>(null),
    name = $state(''),
    bio = $state(''),
    error = $state(''),
    notice = $state(''),
    busy = $state(false),
    projects = $state<Project[]>([]),
    projectsLoaded = $state(false),
    summaries = $state<Record<string, ProjectSummary>>({}),
    year = $state(thisYear),
    days = $state<ActivityDay[]>([]),
    graphLoading = $state(true),
    graphDerived = $state(false),
    recent = $state<(ActivityItem & { project_id: string; project_name: string })[]>([]);
  const years = [thisYear, thisYear - 1, thisYear - 2];
  const pinned = $derived(
    [...projects]
      .sort((a, b) => Number(b.role === 'owner') - Number(a.role === 'owner'))
      .slice(0, 6)
  );
  const total = $derived(days.reduce((n, d) => n + d.count, 0));
  onMount(async () => {
    try {
      profile = await loadProfile(id);
      name = profile.name;
      bio = profile.bio;
      if (!id) storage = await workspaceRequest('/profile/storage');
    } catch (e) {
      error = e instanceof Error ? e.message : 'This profile is not available';
      return;
    }
    void quietExtras();
  });
  /** Projects, the graph and recent activity fill in after the profile itself. */
  async function quietExtras() {
    let all: Project[] = [];
    try {
      all = (await workspaceRequest<{ items: Project[] }>('/projects')).items;
    } catch {
      /* The overview simply shows no projects. */
    }
    if (id) {
      // Another person's profile lists only the projects both of you can open.
      const shared = await mapPool(all.slice(0, 24), 4, async (p) => {
        const m = await workspaceRequest<{ items: MemberLite[] }>(`/spaces/${p.id}/members`);
        return m.items.some((x) => x.id === id) ? p : null;
      });
      all = shared.filter((p): p is Project => !!p);
    }
    projects = all;
    projectsLoaded = true;
    void loadGraph();
    void mapPool(pinned, 4, async (p) => {
      summaries[p.id] = await summarizeProject(p.id);
    });
    const pages = await mapPool(all.slice(0, 8), 3, (p) =>
      loadActivity(p.id, [], new Map(), null, 30)
    );
    const mine = profile?.name || '';
    recent = pages
      .flatMap((page, i) =>
        (page?.items || []).map((item) => ({
          ...item,
          project_id: all[i].id,
          project_name: all[i].name
        }))
      )
      .filter((item) => !!mine && item.actor_name === mine)
      .sort((a, b) => Date.parse(b.at) - Date.parse(a.at))
      .slice(0, 20);
  }
  async function loadGraph(chosen = year) {
    if (!profile) return;
    graphLoading = true;
    try {
      const result = await loadProfileActivity(
        profile.id,
        chosen,
        projects.map((p) => p.id),
        profile.name
      );
      if (chosen === year) {
        days = result.days;
        graphDerived = result.derived;
      }
    } finally {
      graphLoading = false;
    }
  }
  async function save() {
    if (!profile) return;
    busy = true;
    error = '';
    notice = '';
    try {
      profile = await workspaceRequest<Profile>('/profile', {
        name,
        bio,
        revision: profile.revision
      });
      name = profile.name;
      bio = profile.bio;
      notice = 'Profile saved';
      window.dispatchEvent(new CustomEvent('dynodoc:profile', { detail: profile.name }));
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not save your profile';
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head><title>{profile?.name || 'Profile'} · Dynodoc</title></svelte:head>
<RepositoryShell
  active="profile"
  crumbs={[
    { label: 'Projects', href: '/projects' },
    { label: id ? 'Team profile' : 'Your profile' }
  ]}
>
  <div class="rp-band slim">
    <div class="repo-wrap">
      {#if !id}<ProfileNav current="overview" count={projectsLoaded ? projects.length : null} />
      {:else}<p class="rp-bandnote">
          <LockKeyhole size={14} aria-hidden="true" /> Visible to people who share a project with you
        </p>{/if}
    </div>
  </div>
  <div class="repo-wrap rp-body">
    {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}
    {#if notice}<p class="repo-notice" role="status">{notice}</p>{/if}
    {#if profile}
      <div class="rp-dashboard">
        <aside class="rp-card" aria-label="Profile">
          <div class="rp-card-avatar"><Avatar name={profile.name} size={96} /></div>
          <h1>{profile.name || 'Your name'}</h1>
          {#if profile.bio}<p class="rp-bio" style="white-space:pre-wrap">{profile.bio}</p>{/if}
          <span class="repo-badge">Project members only</span>
          <ul class="rp-facts">
            <li>
              <FolderGit2 size={15} aria-hidden="true" />
              {projectsLoaded
                ? plural(projects.length, id ? 'shared project' : 'project')
                : 'Projects'}
            </li>
          </ul>
        </aside>
        <div class="rp-profile-main">
          {#if !id}<section class="repo-box" aria-labelledby="profile-details">
              <div class="repo-panel-body repo-form">
                <h2 id="profile-details">Profile details</h2>
                <p class="repo-muted">
                  Help collaborators recognise your contributions. Your email is not shown on your
                  profile.
                </p>
                <form
                  onsubmit={(e) => {
                    e.preventDefault();
                    void save();
                  }}
                >
                  <label
                    >Display name<input
                      bind:value={name}
                      required
                      maxlength="80"
                      autocomplete="name"
                    /></label
                  ><label
                    >Bio<textarea
                      bind:value={bio}
                      maxlength="280"
                      placeholder="A little about your work"
                    ></textarea></label
                  >
                  <p class="repo-muted repo-small">{bio.length}/280 characters</p>
                  <button class="repo-primary" disabled={busy}
                    >{busy ? 'Saving…' : 'Save profile'}</button
                  >
                </form>
              </div>
            </section>{/if}
          <section aria-labelledby="pinned-title">
            <div class="rp-section-head">
              <h2 id="pinned-title">{id ? 'Shared projects' : 'Projects'}</h2>
              {#if !id}<a href="/projects">All projects</a>{/if}
            </div>
            {#if !projectsLoaded}
              <div class="rp-pinned" aria-busy="true">
                {#each [0, 1] as n (n)}<div class="rp-pin" aria-hidden="true">
                    <span class="repo-skel" style:width="{40 + n * 15}%"></span><span
                      class="repo-skel"
                      style:width="70%"
                    ></span>
                  </div>{/each}
              </div>
            {:else if !pinned.length}<p class="repo-muted">
                {id ? 'You do not share a project with this person yet.' : 'No projects yet.'}
              </p>
            {:else}
              <div class="rp-pinned">
                {#each pinned as p (p.id)}
                  {@const s = summaries[p.id]}
                  <article class="rp-pin">
                    <div class="rp-proj-head">
                      <FolderGit2 size={16} aria-hidden="true" />
                      <h3><a href={`/projects/${p.id}`}>{p.name}</a></h3>
                      <span class="repo-badge">Private</span>
                    </div>
                    <p class="repo-muted rp-proj-desc">
                      {p.description || 'Files and changes shared with your team.'}
                    </p>
                    <div class="rp-proj-meta">
                      {#if s?.formats.length}<FormatsBar variant="dots" shares={s.formats} />{/if}
                      <span class="repo-role">{projectRoleName(p.role)}</span>
                      {#if s?.open_requests}<a href={`/projects/${p.id}?tab=requests`}
                          ><GitPullRequest size={14} aria-hidden="true" /> {s.open_requests} open</a
                        >{/if}
                    </div>
                  </article>
                {/each}
              </div>
            {/if}
          </section>
          <section class="repo-box rp-graph" aria-labelledby="graph-title">
            <div class="repo-panel-body">
              <h2 id="graph-title">
                {total ? plural(total, 'recorded change') : 'No recorded activity yet'}
                {total ? `in ${year}` : ''}
              </h2>
              <ContributionGraph
                {year}
                {years}
                {days}
                loading={graphLoading}
                onYear={(y) => {
                  year = y;
                  void loadGraph(y);
                }}
              />
              <p class="repo-muted repo-small">
                {graphDerived
                  ? 'Based on recent activity in projects you can open.'
                  : 'Recorded changes in projects you can open.'}
              </p>
            </div>
          </section>
          <section aria-labelledby="recent-title">
            <h2 id="recent-title"><CalendarDays size={16} aria-hidden="true" /> Recent activity</h2>
            {#if recent.length}<ActivityFeed items={recent} showActor={false} />
            {:else}<p class="repo-muted">
                {projectsLoaded ? 'No recorded activity yet.' : 'Loading recent activity…'}
              </p>{/if}
          </section>
          {#if !id && storage}
            <section class="repo-box" aria-labelledby="shared-storage-title">
              <div class="repo-panel-body">
                <h2 id="shared-storage-title">Shared history storage</h2>
                <p>
                  {(storage.bytes_used / 1048576).toLocaleString(undefined, {
                    maximumFractionDigits: 2
                  })} MiB of {(storage.byte_limit / 1048576).toLocaleString()} MiB used · {storage.objects_used.toLocaleString()}
                  of {storage.object_limit.toLocaleString()} content objects.
                </p>
                <div
                  class="rp-meter"
                  role="meter"
                  aria-label="Shared history storage used"
                  aria-valuemin="0"
                  aria-valuemax={storage.byte_limit}
                  aria-valuenow={storage.bytes_used}
                >
                  <span
                    style:width="{Math.min(
                      100,
                      (storage.bytes_used / (storage.byte_limit || 1)) * 100
                    )}%"
                  ></span>
                </div>
                <p class="repo-muted">
                  Retained shared history for files you created. Original uploads and older snapshot
                  storage are counted separately. Contact the workspace operator if you need more
                  space.
                </p>
              </div>
            </section>
          {/if}
          {#if !id}<DeviceKeys />{/if}
        </div>
      </div>
    {:else if !error}
      <div class="repo-skel-stack" aria-busy="true">
        {#each [0, 1, 2] as n (n)}<div class="repo-skel-row" aria-hidden="true">
            <span class="repo-skel" style:width="{30 + n * 12}%"></span>
          </div>{/each}
      </div>
    {/if}
  </div>
</RepositoryShell>
