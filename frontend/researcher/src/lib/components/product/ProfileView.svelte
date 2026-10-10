<script lang="ts">
  import { onMount } from 'svelte';
  import { workspaceRequest } from '$lib/workspace/api';
  import { loadProfile, initials, type Profile } from '$lib/workspace/profile';
  import RepositoryShell from './RepositoryShell.svelte';
  let { id }: { id?: string } = $props();
  let profile = $state<Profile | null>(null),
    name = $state(''),
    bio = $state(''),
    error = $state(''),
    notice = $state(''),
    busy = $state(false);
  onMount(async () => {
    try {
      profile = await loadProfile(id);
      name = profile.name;
      bio = profile.bio;
    } catch (e) {
      error = e instanceof Error ? e.message : 'This profile is not available';
    }
  });
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
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not save your profile';
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head><title>{profile?.name || 'Profile'} · Dynodoc</title></svelte:head>
<RepositoryShell active="profile">
  <div class="repo-title">
    <a href="/projects">Projects</a><span class="repo-muted">/</span>
    <h1>{id ? 'Team profile' : 'Your profile'}</h1>
  </div>
  {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}
  {#if notice}<p class="repo-notice" role="status">{notice}</p>{/if}
  {#if profile}<div class="repo-dashboard">
      <aside>
        <div class="repo-avatar" aria-hidden="true">{initials(profile.name)}</div>
        <h2>{profile.name || 'Your name'}</h2>
        <p style="white-space:pre-wrap">{profile.bio}</p>
        <span class="repo-badge">Project members only</span>
      </aside>
      {#if !id}<section class="repo-panel">
          <div class="repo-panel-body repo-form">
            <h2>Profile details</h2>
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
    </div>{:else if !error}<p role="status">Loading profile…</p>{/if}
</RepositoryShell>
