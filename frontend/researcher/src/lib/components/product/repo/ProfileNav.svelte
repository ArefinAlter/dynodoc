<script lang="ts">
  import { BookOpen, FolderGit2 } from '@lucide/svelte';
  let { current, count = null }: { current: 'overview' | 'projects'; count?: number | null } =
    $props();
</script>

<nav class="profile-nav" aria-label="Profile sections">
  <a href="/profile" aria-current={current === 'overview' ? 'page' : undefined}
    ><BookOpen size={16} aria-hidden="true" />Overview</a
  >
  <a href="/projects" aria-current={current === 'projects' ? 'page' : undefined}
    ><FolderGit2 size={16} aria-hidden="true" />Projects{#if count !== null}<span
        class="count"
        aria-hidden="true">{count}</span
      >{/if}</a
  >
</nav>

<style>
  .profile-nav {
    display: flex;
    gap: 2px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .profile-nav a {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    margin-bottom: 4px;
    border-radius: 6px;
    color: var(--rp-fg);
    white-space: nowrap;
    transition: background-color 0.12s ease;
  }
  .profile-nav a:hover {
    background: var(--rp-btn-hover);
    text-decoration: none;
  }
  .profile-nav a :global(svg) {
    color: var(--rp-muted);
  }
  .profile-nav a[aria-current='page'] {
    font-weight: 600;
  }
  .profile-nav a[aria-current='page']::after {
    content: '';
    position: absolute;
    left: 8px;
    right: 8px;
    bottom: -5px;
    height: 2px;
    border-radius: 2px;
    background: var(--rp-underline);
  }
  .count {
    min-width: 20px;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--rp-neutral-bg);
    font-size: 12px;
    font-weight: 500;
    line-height: 18px;
    text-align: center;
  }
</style>
