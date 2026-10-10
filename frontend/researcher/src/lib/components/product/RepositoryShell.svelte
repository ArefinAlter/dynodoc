<script lang="ts">
  import type { Snippet } from 'svelte';
  import { FolderGit2, PanelsTopLeft, Plug, UserRound, LogOut } from '@lucide/svelte';
  import { goto } from '$app/navigation';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import NotificationBell from '$lib/components/workspace/NotificationBell.svelte';
  import { clearSession } from '$lib/stores/session.svelte';
  import './repository.css';
  let { children, active = 'projects' }: { children: Snippet; active?: string } = $props();
  let signOutError = $state('');
</script>

<div class="repo">
  <a class="repo-skip" href="#repository-content">Skip to content</a>
  <header class="repo-global">
    <a class="repo-brand" href="/projects"
      ><img src="/favicon.svg" alt="" width="28" height="28" />Dynodoc</a
    >
    <nav aria-label="Workspace">
      <a href="/projects" aria-current={active === 'projects' ? 'page' : undefined}
        ><FolderGit2 size={17} /> Projects</a
      >
      <a href="/workspace" aria-current={active === 'editors' ? 'page' : undefined}
        ><PanelsTopLeft size={17} /> Editors</a
      >
      <a href="/connectors"><Plug size={17} /> Connect editors</a>
    </nav>
    <div class="repo-account">
      <ThemeToggle /><NotificationBell /><a
        class="repo-icon"
        href="/profile"
        aria-label="Your profile"
        aria-current={active === 'profile' ? 'page' : undefined}><UserRound size={19} /></a
      ><button
        class="repo-icon"
        aria-label="Sign out"
        onclick={async () => {
          signOutError = '';
          try {
            await clearSession();
            await goto('/login');
          } catch {
            signOutError = 'Sign-out could not be completed. Please try again.';
          }
        }}><LogOut size={17} /></button
      >
    </div>
  </header>
  {#if signOutError}<p class="repo-content" role="alert">{signOutError}</p>{/if}
  <main id="repository-content" class="repo-content">{@render children()}</main>
  <footer class="repo-footer">
    <span>Dynodoc · Version control for documents</span><a href="/compatibility"
      >Supported formats</a
    ><span>Test uploads: 3 MB per file</span>
  </footer>
</div>
