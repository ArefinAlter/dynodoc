<script lang="ts">
  import type { EditorDocument, EditorMode } from '$lib/types/editor';
  import ModeToggle from './ModeToggle.svelte';

  interface Props {
    document: EditorDocument;
    mode: EditorMode;
    suggesting: boolean;
    activityOpen: boolean;
    onModeChange: (mode: EditorMode) => void;
    onToggleSuggesting: () => void;
    onToggleActivity: () => void;
    onOpenConflict: () => void;
    /** Number of live four-card conflicts; the conflict button is disabled at 0. */
    conflictCount?: number;
  }

  let {
    document,
    mode,
    suggesting,
    activityOpen,
    onModeChange,
    onToggleSuggesting,
    onToggleActivity,
    onOpenConflict,
    conflictCount = 0
  }: Props = $props();
</script>

<header class="topbar">
  <div class="brand">
    <div class="mark">d</div>
    <div>
      <div class="breadcrumb">dynodoc / instruments / <strong>{document.title}</strong></div>
      <div class="version">{document.version}</div>
    </div>
  </div>

  <ModeToggle {mode} onChange={onModeChange} />

  <div class="actions">
    <button type="button" class:suggesting onclick={onToggleSuggesting}>
      <span class="dot"></span>
      {suggesting ? 'Suggesting' : 'Editing'}
    </button>
    <button
      type="button"
      class:active={conflictCount > 0}
      disabled={conflictCount === 0}
      onclick={onOpenConflict}
    >
      Conflicts{conflictCount > 0 ? ` (${conflictCount})` : ''}
    </button>
    <button type="button" class:active={activityOpen} onclick={onToggleActivity}>Activity</button>
  </div>
</header>

<style>
  .topbar {
    position: sticky;
    top: 0;
    z-index: 20;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 1rem;
    padding: 1rem 1.5rem;
    background: color-mix(in srgb, var(--color-surface) 92%, white);
    border-bottom: 1px solid var(--color-border);
    backdrop-filter: blur(14px);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    min-width: 0;
  }

  .mark {
    display: grid;
    place-items: center;
    width: 2rem;
    height: 2rem;
    border-radius: 0.4rem;
    background: var(--color-ink);
    color: var(--color-surface);
    font-family: var(--font-display);
    font-style: italic;
    font-weight: 600;
  }

  .breadcrumb,
  .version,
  .actions {
    font-family: var(--font-ui);
  }

  .breadcrumb {
    color: var(--color-muted);
    font-size: 0.82rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .breadcrumb strong {
    color: var(--color-ink);
    font-weight: 600;
  }

  .version {
    color: var(--color-accent);
    font-size: 0.74rem;
    margin-top: 0.1rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.65rem;
  }

  .actions button {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    border: 1px solid var(--color-border);
    border-radius: 0.65rem;
    padding: 0.5rem 0.8rem;
    background: var(--color-surface);
    color: var(--color-muted);
    font: inherit;
  }

  .actions button.active,
  .actions button:hover {
    color: var(--color-ink);
    border-color: var(--color-border-strong);
  }

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .actions button.suggesting {
    background: var(--color-suggest-bg);
    color: var(--color-suggest);
    border-color: color-mix(in srgb, var(--color-suggest) 55%, white);
  }

  .dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 999px;
    background: currentColor;
  }

  @media (max-width: 960px) {
    .topbar {
      grid-template-columns: 1fr;
    }

    .actions {
      justify-content: flex-start;
      flex-wrap: wrap;
    }
  }
</style>
