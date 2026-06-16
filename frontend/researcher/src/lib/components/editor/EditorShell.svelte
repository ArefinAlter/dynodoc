<script lang="ts">
  import type { EditorDocument, EditorItem, EditorMode } from '$lib/types/editor';
  import Topbar from './Topbar.svelte';
  import DocumentView from './DocumentView.svelte';
  import OutlineView from './OutlineView.svelte';
  import SpecView from './SpecView.svelte';
  import PreviewView from './PreviewView.svelte';
  import Inspector from './Inspector.svelte';
  import ActivityLog from './ActivityLog.svelte';
  import ConflictModal from './ConflictModal.svelte';

  interface Props {
    document: EditorDocument;
  }

  let { document }: Props = $props();

  let mode = $state<EditorMode>('document');
  let selectedId = $state('');
  let suggesting = $state(false);
  let activityOpen = $state(false);
  let conflictOpen = $state(false);
  let selectedConflictOptionId = $state('');

  $effect(() => {
    if (!selectedId) {
      selectedId = document.sections[0]?.items[0]?.id ?? '';
    }
  });

  function findSelectedItem(): EditorItem | undefined {
    return document.sections
      .flatMap((section) => section.items)
      .find((item) => item.id === selectedId);
  }

  function selectItem(item: EditorItem) {
    selectedId = item.id;
  }
</script>

<Topbar
  {document}
  {mode}
  {suggesting}
  {activityOpen}
  onModeChange={(next) => (mode = next)}
  onToggleSuggesting={() => (suggesting = !suggesting)}
  onToggleActivity={() => (activityOpen = !activityOpen)}
  onOpenConflict={() => (conflictOpen = true)}
/>

<div class="page">
  <main class:full={mode !== 'document'} class="main">
    {#if mode === 'document'}
      <DocumentView {document} {selectedId} {suggesting} onSelect={selectItem} />
    {:else if mode === 'outline'}
      <OutlineView {document} {selectedId} onSelect={selectItem} />
    {:else if mode === 'spec'}
      <SpecView {document} />
    {:else}
      <PreviewView {document} />
    {/if}
  </main>

  {#if mode === 'document'}
    <Inspector item={findSelectedItem()} />
  {/if}
</div>

<ActivityLog activity={document.activity} open={activityOpen} />
<ConflictModal
  conflict={document.conflict}
  open={conflictOpen}
  selectedOptionId={selectedConflictOptionId}
  onClose={() => {
    conflictOpen = false;
    selectedConflictOptionId = '';
  }}
  onSelect={(id) => (selectedConflictOptionId = id)}
/>

<style>
  .page {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 23rem;
    gap: 1.2rem;
    max-width: 96rem;
    margin: 0 auto;
    padding: 1.5rem;
  }

  .main {
    min-width: 0;
    background: transparent;
  }

  .main.full {
    grid-column: 1 / -1;
  }

  @media (max-width: 1100px) {
    .page {
      grid-template-columns: 1fr;
    }
  }
</style>
