<script lang="ts">
  import { tick } from 'svelte';
  import { goto } from '$app/navigation';
  import {
    Search,
    FolderGit2,
    Plus,
    Upload,
    UserRound,
    KeyRound,
    PanelsTopLeft,
    Plug,
    Sun,
    Moon,
    SunMoon,
    Keyboard,
    FileText,
    X,
    CornerDownLeft,
    CircleDot
  } from '@lucide/svelte';
  import { workspaceRequest } from '$lib/workspace/api';
  import { theme } from '$lib/theme.svelte';
  import FormatIcon from './FormatIcon.svelte';

  let {
    open = $bindable(false),
    mode = $bindable<'all' | 'files'>('all'),
    projectId = '',
    canAdd = false,
    onAction
  }: {
    open?: boolean;
    mode?: 'all' | 'files';
    projectId?: string;
    canAdd?: boolean;
    onAction: (action: 'help' | 'add-file') => void;
  } = $props();

  type Entry = {
    id: string;
    group: string;
    label: string;
    hint?: string;
    icon: typeof Search;
    kind?: string;
    run: () => void | Promise<void>;
  };
  let dialog = $state<HTMLDialogElement | null>(null),
    input = $state<HTMLInputElement | null>(null),
    query = $state(''),
    cursor = $state(0),
    projects = $state<{ id: string; name: string; description: string }[]>([]),
    files = $state<{ id: string; title: string; kind: string }[]>([]),
    docs = $state<{ id: string; title: string }[]>([]),
    loading = $state(false);
  const filesMode = $derived(mode === 'files' && !!projectId);

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    else if (!open && dialog.open) dialog.close();
  });
  $effect(() => {
    if (!open) return;
    query = '';
    cursor = 0;
    void tick().then(() => input?.focus());
    loading = true;
    void Promise.allSettled([
      workspaceRequest<{ items: typeof projects }>('/projects').then((r) => (projects = r.items)),
      projectId
        ? workspaceRequest<{ items: typeof files }>(`/projects/${projectId}/files`).then(
            (r) => (files = r.items)
          )
        : Promise.resolve()
    ]).finally(() => (loading = false));
  });
  // Document search runs only for a query of two characters or more.
  $effect(() => {
    const q = query.trim();
    if (!open || filesMode || q.length < 2) {
      docs = [];
      return;
    }
    let current = true;
    const timer = setTimeout(async () => {
      try {
        const r = await workspaceRequest<{ items: { document_id: string; title: string }[] }>(
          `/search?q=${encodeURIComponent(q)}`
        );
        if (!current) return;
        const seen = new Set<string>();
        docs = r.items
          .filter((i) => !seen.has(i.document_id) && !!seen.add(i.document_id))
          .slice(0, 6)
          .map((i) => ({ id: i.document_id, title: i.title }));
      } catch {
        if (current) docs = [];
      }
    }, 250);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  });

  const go = (path: string) => () => goto(path);
  /**
   * The project page listens for this and shows its Issues tab. When no page handled it
   * (for example on a file page), open the project's Issues tab by address instead.
   */
  const issues = (view: 'list' | 'new') => () => {
    const event = new CustomEvent('dynodoc:issues', { detail: { view }, cancelable: true });
    if (window.dispatchEvent(event))
      void goto(`/projects/${projectId}?tab=issues${view === 'new' ? '&new=1' : ''}`);
  };
  const actions = $derived<Entry[]>([
    { id: 'a-new', group: 'Actions', label: 'New project', icon: Plus, run: go('/projects?new=1') },
    ...(canAdd
      ? [
          {
            id: 'a-add',
            group: 'Actions',
            label: 'Add file to this project',
            icon: Upload,
            run: () => onAction('add-file')
          }
        ]
      : []),
    ...(projectId
      ? [
          {
            id: 'a-issue-new',
            group: 'Actions',
            label: 'New issue',
            icon: CircleDot,
            run: issues('new')
          },
          {
            id: 'a-issues',
            group: 'Actions',
            label: 'Go to issues',
            icon: CircleDot,
            run: issues('list')
          }
        ]
      : []),
    {
      id: 'a-projects',
      group: 'Actions',
      label: 'Your projects',
      icon: FolderGit2,
      run: go('/projects')
    },
    {
      id: 'a-profile',
      group: 'Actions',
      label: 'Your profile',
      icon: UserRound,
      run: go('/profile')
    },
    {
      id: 'a-keys',
      group: 'Actions',
      label: 'Manage signing keys',
      icon: KeyRound,
      run: go('/profile#signing-keys')
    },
    {
      id: 'a-editors',
      group: 'Actions',
      label: 'Web editors',
      icon: PanelsTopLeft,
      run: go('/workspace')
    },
    {
      id: 'a-connect',
      group: 'Actions',
      label: 'Connect editors',
      icon: Plug,
      run: go('/connectors')
    },
    {
      id: 't-light',
      group: 'Theme',
      label: 'Light theme',
      icon: Sun,
      run: () => theme.set('light')
    },
    { id: 't-dark', group: 'Theme', label: 'Dark theme', icon: Moon, run: () => theme.set('dark') },
    {
      id: 't-system',
      group: 'Theme',
      label: 'Match system theme',
      icon: SunMoon,
      run: () => theme.set('system')
    },
    {
      id: 'a-help',
      group: 'Actions',
      label: 'Keyboard shortcuts',
      icon: Keyboard,
      run: () => onAction('help')
    }
  ]);
  function match(label: string, q: string) {
    const text = label.toLowerCase();
    const tokens = q.toLowerCase().split(/\s+/).filter(Boolean);
    if (!tokens.every((t) => text.includes(t))) return 0;
    if (!tokens.length) return 1;
    return text.startsWith(tokens[0])
      ? 3
      : text.split(/\s+/).some((w) => w.startsWith(tokens[0]))
        ? 2
        : 1;
  }
  async function openDocument(id: string) {
    try {
      const access = await workspaceRequest<{ project: { id: string } | null }>(
        `/documents/${id}/access`
      );
      await goto(
        access.project ? `/projects/${access.project.id}/files/${id}` : `/documents/${id}`
      );
    } catch {
      await goto(`/documents/${id}`);
    }
  }
  const results = $derived.by<Entry[]>(() => {
    const q = query.trim();
    const rank = (list: Entry[], limit: number) =>
      list
        .map((entry) => ({ entry, score: match(entry.label, q) }))
        .filter((x) => x.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, limit)
        .map((x) => x.entry);
    const fileEntries: Entry[] = files.map((f) => ({
      id: `f-${f.id}`,
      group: 'Files in this project',
      label: f.title,
      kind: f.kind,
      icon: FileText,
      run: go(`/projects/${projectId}/files/${f.id}`)
    }));
    if (filesMode) return rank(fileEntries, 30);
    const projectEntries: Entry[] = projects.map((p) => ({
      id: `p-${p.id}`,
      group: 'Projects',
      label: p.name,
      hint: p.description,
      icon: FolderGit2,
      run: go(`/projects/${p.id}`)
    }));
    const docEntries: Entry[] = docs.map((d) => ({
      id: `d-${d.id}`,
      group: 'Documents',
      label: d.title,
      icon: FileText,
      run: () => openDocument(d.id)
    }));
    const limit = q ? 8 : 5;
    return [
      ...rank(actions, q ? 6 : 14),
      ...rank(projectEntries, limit),
      ...rank(fileEntries, limit),
      ...docEntries
    ];
  });
  $effect(() => {
    void results.length;
    cursor = 0;
  });
  function choose(entry: Entry | undefined) {
    if (!entry) return;
    open = false;
    void entry.run();
  }
  function key(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (!results.length) return;
      cursor = (cursor + (e.key === 'ArrowDown' ? 1 : -1) + results.length) % results.length;
      void tick().then(() =>
        dialog
          ?.querySelector(`[data-entry="${CSS.escape(results[cursor].id)}"]`)
          ?.scrollIntoView({ block: 'nearest' })
      );
    } else if (e.key === 'Enter') {
      e.preventDefault();
      choose(results[cursor]);
    }
  }
</script>

<dialog
  bind:this={dialog}
  class="palette"
  aria-label="Command palette"
  onclose={() => (open = false)}
  onclick={(e) => {
    if (e.target === dialog) open = false;
  }}
>
  {#if open}
    <div class="field">
      <Search size={16} aria-hidden="true" />
      <input
        bind:this={input}
        bind:value={query}
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        aria-activedescendant={results[cursor] ? `opt-${results[cursor].id}` : undefined}
        aria-autocomplete="list"
        aria-label={filesMode ? 'Go to file' : 'Search projects, files and actions'}
        placeholder={filesMode ? 'Go to file…' : 'Search projects, files and actions…'}
        autocomplete="off"
        spellcheck="false"
        onkeydown={key}
      />
      {#if filesMode}<button
          type="button"
          class="chip"
          title="Search everything instead"
          onclick={() => {
            mode = 'all';
            input?.focus();
          }}>This project <X size={12} aria-hidden="true" /></button
        >{/if}
    </div>
    <ul id="palette-list" role="listbox" aria-label="Results" aria-busy={loading}>
      {#each results as entry, i (entry.id)}
        {#if i === 0 || results[i - 1].group !== entry.group}<li class="group" role="presentation">
            {entry.group}
          </li>{/if}
        <li
          id={`opt-${entry.id}`}
          data-entry={entry.id}
          role="option"
          aria-selected={i === cursor}
          class:active={i === cursor}
          onpointermove={() => (cursor = i)}
          onclick={() => choose(entry)}
          onkeydown={() => {}}
        >
          {#if entry.kind}<FormatIcon kind={entry.kind} size={16} />{:else}<entry.icon
              size={16}
              aria-hidden="true"
            />{/if}
          <span class="label">{entry.label}</span>
          {#if entry.hint}<span class="hint">{entry.hint}</span>{/if}
          {#if i === cursor}<CornerDownLeft size={14} aria-hidden="true" />{/if}
        </li>
      {/each}
      {#if !results.length}<li class="none" role="presentation">
          {loading ? 'Searching…' : 'Nothing matches. Try another word.'}
        </li>{/if}
    </ul>
    <p class="foot">
      <span><kbd>↑</kbd> <kbd>↓</kbd> to move</span><span><kbd>↵</kbd> to open</span><span
        ><kbd>esc</kbd> to close</span
      >
    </p>
  {/if}
</dialog>

<style>
  .palette {
    margin: 12vh auto auto;
    margin: 12dvh auto auto;
    width: min(640px, calc(100vw - 24px));
    max-height: 70vh;
    max-height: 70dvh;
    padding: 0;
    border: 1px solid var(--rp-border);
    border-radius: 12px;
    background: var(--rp-overlay);
    color: var(--rp-fg);
    box-shadow: var(--rp-shadow);
    overflow: hidden;
  }
  .palette[open] {
    display: flex;
    flex-direction: column;
    animation: rise 0.14s ease-out;
  }
  .palette::backdrop {
    background: rgb(1 4 9 / 0.5);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.985);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .palette[open] {
      animation: none;
    }
  }
  .field {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--rp-border);
    color: var(--rp-muted);
  }
  .field input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--rp-fg);
    font: inherit;
    font-size: 16px;
    padding: 4px 0;
  }
  .field:focus-within {
    box-shadow: inset 0 -2px 0 var(--rp-accent);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px;
    border: 1px solid var(--rp-border);
    border-radius: 999px;
    background: var(--rp-subtle);
    color: var(--rp-fg);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  ul {
    margin: 0;
    padding: 6px;
    list-style: none;
    overflow-y: auto;
    flex: 1;
  }
  .group {
    padding: 8px 10px 4px;
    font-size: 12px;
    font-weight: 600;
    color: var(--rp-muted);
  }
  [role='option'] {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: 6px;
    cursor: pointer;
    color: var(--rp-fg);
  }
  [role='option'].active {
    background: var(--rp-accent-bg);
    box-shadow: inset 2px 0 0 var(--rp-accent);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--rp-muted);
    font-size: 12px;
  }
  .none {
    padding: 18px 10px;
    color: var(--rp-muted);
    text-align: center;
  }
  .foot {
    display: flex;
    gap: 16px;
    margin: 0;
    padding: 8px 16px;
    border-top: 1px solid var(--rp-border);
    background: var(--rp-subtle);
    color: var(--rp-muted);
    font-size: 12px;
  }
  kbd {
    padding: 0 5px;
    border: 1px solid var(--rp-border);
    border-bottom-width: 2px;
    border-radius: 5px;
    background: var(--rp-canvas);
    font-family: var(--rp-mono);
    font-size: 11px;
  }
</style>
