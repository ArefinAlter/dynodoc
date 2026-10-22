<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import {
    Plus,
    ChevronDown,
    Search,
    LogOut,
    FolderGit2,
    PanelsTopLeft,
    Plug,
    UserRound,
    Sun,
    Moon,
    SunMoon,
    Keyboard,
    Check,
    Upload
  } from '@lucide/svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import OrgNotice from './OrgNotice.svelte';
  import NotificationBell from '$lib/components/workspace/NotificationBell.svelte';
  import { clearSession } from '$lib/stores/session.svelte';
  import { theme, type ThemePreference } from '$lib/theme.svelte';
  import { loadProfile } from '$lib/workspace/profile';
  import Avatar from './repo/Avatar.svelte';
  import Menu from './repo/Menu.svelte';
  import CommandPalette from './repo/CommandPalette.svelte';
  import ShortcutHelp from './repo/ShortcutHelp.svelte';
  import './repository.css';
  type Crumb = { label: string; href?: string };
  let {
    children,
    active = 'projects',
    crumbs = [],
    projectId = '',
    canAdd = false
  }: {
    children: Snippet;
    active?: string;
    crumbs?: Crumb[];
    /** Set on a project's pages so the palette and `t` can find its files. */
    projectId?: string;
    canAdd?: boolean;
  } = $props();
  let signOutError = $state(''),
    me = $state(''),
    paletteOpen = $state(false),
    paletteMode = $state<'all' | 'files'>('all'),
    helpOpen = $state(false),
    pendingG = false,
    gTimer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => {
    theme.watch();
    void loadProfile()
      .then((p) => (me = p.name))
      .catch(() => {});
    const rename = (e: Event) => {
      const name = (e as CustomEvent<string>).detail;
      if (typeof name === 'string') me = name;
    };
    const palette = (e: Event) => {
      paletteMode = (e as CustomEvent<{ mode?: 'all' | 'files' }>).detail?.mode || 'all';
      paletteOpen = true;
    };
    window.addEventListener('dynodoc:profile', rename);
    window.addEventListener('dynodoc:palette', palette);
    return () => {
      window.removeEventListener('dynodoc:profile', rename);
      window.removeEventListener('dynodoc:palette', palette);
      clearTimeout(gTimer);
    };
  });
  function typing(target: EventTarget | null) {
    if (!(target instanceof HTMLElement)) return false;
    return (
      target.isContentEditable ||
      ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName) ||
      !!target.closest('[contenteditable="true"],[contenteditable=""]')
    );
  }
  function openPalette(mode: 'all' | 'files' = 'all') {
    paletteMode = mode;
    paletteOpen = true;
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.defaultPrevented || e.isComposing) return;
    if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      openPalette();
      return;
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (typing(e.target) || document.querySelector('dialog[open]')) {
      pendingG = false;
      return;
    }
    if (pendingG) {
      pendingG = false;
      clearTimeout(gTimer);
      const next = { p: '/projects', e: '/workspace', c: '/connectors' }[e.key.toLowerCase()];
      if (next) {
        e.preventDefault();
        void goto(next);
      }
      return;
    }
    if (e.key === '/') {
      e.preventDefault();
      openPalette();
    } else if (e.key === '?') {
      e.preventDefault();
      helpOpen = true;
    } else if (e.key === 't') {
      e.preventDefault();
      openPalette(projectId ? 'files' : 'all');
    } else if (e.key === 'g') {
      pendingG = true;
      gTimer = setTimeout(() => (pendingG = false), 1200);
    }
  }
  async function signOut() {
    signOutError = '';
    try {
      await clearSession();
      await goto('/login');
    } catch {
      signOutError = 'Sign-out could not be completed. Please try again.';
    }
  }
  const themes: [ThemePreference, string, typeof Sun][] = [
    ['light', 'Light', Sun],
    ['dark', 'Dark', Moon],
    ['system', 'Match system', SunMoon]
  ];
</script>

<svelte:window onkeydown={onKeydown} />
<div class="repo" data-active={active}>
  <a class="repo-skip" href="#repository-content">Skip to content</a>
  <header class="repo-global">
    <a class="repo-brand" href="/projects"
      ><img src="/favicon.svg" alt="" width="26" height="26" /><span>Dynodoc</span></a
    >
    {#if crumbs.length}<nav class="repo-crumbs" aria-label="Breadcrumb">
        <ol>
          {#each crumbs as crumb, i}<li>
              <span class="sep" aria-hidden="true">/</span
              >{#if crumb.href && i < crumbs.length - 1}<a href={crumb.href}>{crumb.label}</a
                >{:else}<span aria-current={i === crumbs.length - 1 ? 'page' : undefined}
                  >{crumb.label}</span
                >{/if}
            </li>{/each}
        </ol>
      </nav>{/if}
    <div class="repo-account">
      <button type="button" class="repo-search" onclick={() => openPalette()}
        ><Search size={15} aria-hidden="true" /><span class="repo-search-text"
          >Type <kbd>/</kbd> to search</span
        ></button
      >
      <Menu ariaLabel="Create new" align="right" triggerClass="repo-icon">
        {#snippet trigger()}<Plus size={17} aria-hidden="true" /><ChevronDown
            size={12}
            aria-hidden="true"
          />{/snippet}
        <a role="menuitem" href="/projects?new=1"
          ><FolderGit2 size={16} aria-hidden="true" />New project</a
        >
        {#if canAdd}<button
            type="button"
            role="menuitem"
            onclick={() => window.dispatchEvent(new CustomEvent('dynodoc:add-file'))}
            ><Upload size={16} aria-hidden="true" />Add file</button
          >{/if}
      </Menu>
      <NotificationBell />
      <Menu ariaLabel="Your account" align="right" triggerClass="repo-avatar-btn">
        {#snippet trigger()}<Avatar name={me} size={28} />{/snippet}
        {#if me}<p class="menu-head" role="presentation">Signed in as <strong>{me}</strong></p>{/if}
        <a role="menuitem" href="/profile"><UserRound size={16} aria-hidden="true" />Your profile</a
        >
        <a role="menuitem" href="/projects"
          ><FolderGit2 size={16} aria-hidden="true" />Your projects</a
        >
        <a role="menuitem" href="/workspace"
          ><PanelsTopLeft size={16} aria-hidden="true" />Web editors</a
        >
        <a role="menuitem" href="/connectors"
          ><Plug size={16} aria-hidden="true" />Connect editors</a
        >
        <hr />
        <p class="menu-head" role="presentation">Theme</p>
        {#each themes as [value, name, Icon]}<button
            type="button"
            role="menuitemradio"
            aria-checked={theme.preference === value}
            onclick={() => theme.set(value)}
            ><Icon size={16} aria-hidden="true" />{name}{#if theme.preference === value}<Check
                size={14}
                class="menu-check"
                aria-hidden="true"
              />{/if}</button
          >{/each}
        <hr />
        <button type="button" role="menuitem" onclick={() => (helpOpen = true)}
          ><Keyboard size={16} aria-hidden="true" />Keyboard shortcuts</button
        >
        <button type="button" role="menuitem" onclick={() => void signOut()}
          ><LogOut size={16} aria-hidden="true" />Sign out</button
        >
      </Menu>
      <button
        type="button"
        class="repo-icon"
        aria-label="Sign out"
        title="Sign out"
        onclick={() => void signOut()}><LogOut size={17} aria-hidden="true" /></button
      >
    </div>
  </header>
  {#if signOutError}<p class="repo-alert repo-wrap" role="alert">{signOutError}</p>{/if}
  <main id="repository-content" class="repo-main">{@render children()}</main>
  <footer class="repo-footer">
    <span>{$page.data.site.brand} · Version control for documents</span>
    {#if $page.data.site.mode === 'hosted'}
      <a href="/compatibility">Supported formats</a>
      <a href="/privacy">Privacy</a>
      <a href="/terms">Terms</a>
    {:else}
      <OrgNotice compact />
    {/if}
    <span>Test uploads: 3 MB per file</span>
  </footer>
  <CommandPalette
    bind:open={paletteOpen}
    bind:mode={paletteMode}
    {projectId}
    {canAdd}
    onAction={(action) => {
      if (action === 'help') helpOpen = true;
      else window.dispatchEvent(new CustomEvent('dynodoc:add-file'));
    }}
  />
  <ShortcutHelp bind:open={helpOpen} />
</div>
