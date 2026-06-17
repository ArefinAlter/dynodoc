<script lang="ts">
  import type { EditorItem, EditorMode } from '$lib/types/editor';
  import type { DocumentStore } from '$lib/stores/documentStore.svelte';
  import { toEditorDocument } from '$lib/adapter';

  import Topbar from './Topbar.svelte';
  import DocumentView from './DocumentView.svelte';
  import OutlineView from './OutlineView.svelte';
  import SpecView from './SpecView.svelte';
  import PreviewView from './PreviewView.svelte';
  import InspectorLive from './InspectorLive.svelte';
  import ProposalsPanel from './ProposalsPanel.svelte';
  import HistoryPanel from './HistoryPanel.svelte';

  interface Props {
    store: DocumentStore;
  }

  let { store }: Props = $props();

  // The view model is derived live from the store's folded state. Every SSE event or
  // optimistic op mutates `store.state`, which re-derives this and re-renders.
  const document = $derived(toEditorDocument(store.detail!));

  let mode = $state<EditorMode>('document');
  let selectedId = $state('');
  let suggesting = $state(false);
  let activityOpen = $state(false);
  let proposalsOpen = $state(false);
  let historyOpen = $state(false);

  // Focus the main region on mode switch (keyboard/focus management).
  let mainEl: HTMLElement | undefined = $state();
  function switchMode(next: EditorMode) {
    mode = next;
    queueMicrotask(() => mainEl?.focus());
  }

  $effect(() => {
    if (!selectedId) {
      selectedId = document.sections[0]?.items[0]?.id ?? '';
    }
  });

  const selectedItem = $derived(
    document.sections.flatMap((s) => s.items).find((i) => i.id === selectedId)
  );

  function selectItem(item: EditorItem) {
    selectedId = item.id;
  }

  async function editField(field: string, value: string) {
    if (!selectedId) return;
    await store.editField(selectedId, field, value, suggesting ? 'suggest' : 'direct');
  }

  // Mutually exclusive side panels (only one open at a time).
  function openProposals() {
    proposalsOpen = true;
    historyOpen = false;
    activityOpen = false;
  }
  function openHistory() {
    historyOpen = true;
    proposalsOpen = false;
    activityOpen = false;
  }
</script>

<Topbar
  {document}
  {mode}
  {suggesting}
  {activityOpen}
  onModeChange={switchMode}
  onToggleSuggesting={() => (suggesting = !suggesting)}
  onToggleActivity={openHistory}
  onOpenConflict={openProposals}
/>

<div class="connection {store.connection}">
  <span class="dot"></span>
  {store.connection === 'connected'
    ? 'Live'
    : store.connection === 'reconnecting'
      ? 'Reconnecting…'
      : 'Offline'}
  · seq {store.seq}
  <button type="button" class="link" onclick={openProposals}>Proposals</button>
  <button type="button" class="link" onclick={openHistory}>History</button>
</div>

<div class="page">
  <main
    bind:this={mainEl}
    tabindex="-1"
    class:full={mode !== 'document'}
    class="main"
    aria-label="{mode} mode"
  >
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
    <InspectorLive item={selectedItem} {suggesting} onEdit={editField} />
  {/if}
</div>

<ProposalsPanel {store} open={proposalsOpen} onClose={() => (proposalsOpen = false)} />
<HistoryPanel
  documentId={store.documentId}
  seq={store.seq}
  open={historyOpen}
  onClose={() => (historyOpen = false)}
/>

<style>
  .connection {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    max-width: 96rem;
    margin: 0 auto;
    padding: 0.5rem 1.5rem 0;
    font-family: var(--font-ui);
    font-size: 0.78rem;
    color: var(--color-muted);
  }

  .connection .dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 999px;
    background: var(--color-subtle);
  }

  .connection.connected .dot {
    background: var(--color-suggest);
  }

  .connection.reconnecting .dot {
    background: var(--color-warn);
  }

  .connection.offline .dot {
    background: var(--color-terracotta);
  }

  .link {
    margin-left: 0.4rem;
    border: 0;
    background: none;
    color: var(--color-terracotta);
    text-decoration: underline;
    cursor: pointer;
    font: inherit;
  }

  .page {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 23rem;
    gap: 1.2rem;
    max-width: 96rem;
    margin: 0 auto;
    padding: 1rem 1.5rem 1.5rem;
  }

  .main {
    min-width: 0;
    background: transparent;
    outline: none;
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
