<script lang="ts">
  import type { DocumentState } from '$lib/api/types';
  import { byPosition } from '$lib/workspace/api';
  import { richContent } from '$lib/workspace/changes';
  import RichEditor from '$lib/components/workspace/RichEditor.svelte';
  import SheetEditor from '$lib/components/workspace/SheetEditor.svelte';
  import PresentationEditor from '$lib/components/workspace/PresentationEditor.svelte';
  let { documentState, kind }: { documentState: DocumentState; kind: string } = $props();
  let selected = $state('');
  const nodes = $derived(
    Object.values(documentState.nodes)
      .filter((n) => !n.deleted)
      .sort(byPosition)
  );
  const tabs = $derived(
    nodes.filter(
      (n) => n.type === 'form' || (n.type === 'section' && n.current_fields.kind === 'document_tab')
    )
  );
  const active = $derived(
    tabs.find((n) => n.id === selected) || tabs.find((n) => n.type === 'form') || tabs[0]
  );
</script>

<div class="file-preview" aria-label="Read-only file preview">
  {#if kind === 'document'}
    {#if tabs.length > 1}<nav class="repo-tabs" aria-label="Document tabs">
        {#each tabs as tab}<button
            aria-current={active?.id === tab.id ? 'page' : undefined}
            onclick={() => (selected = tab.id)}
            >{tab.type === 'form'
              ? 'Main document'
              : String(tab.current_fields.label || 'Untitled tab')}</button
          >{/each}
      </nav>{/if}
    <div class="rich-preview">
      <RichEditor
        content={richContent(documentState, active?.id)}
        pageSettings={tabs.find((n) => n.type === 'form')?.current_fields.page_setup}
        editable={false}
        revision={tabs.findIndex((n) => n.id === active?.id)}
        onChange={() => {}}
      />
    </div>
  {:else if kind === 'spreadsheet'}<SheetEditor
      {documentState}
      editable={false}
      onChange={() => {}}
      onSelect={() => {}}
      onUndo={() => {}}
      onRedo={() => {}}
    />
  {:else if kind === 'presentation'}<PresentationEditor
      {documentState}
      editable={false}
      onChange={() => {}}
    />
  {:else}{#each nodes.filter((n) => n.type !== 'form') as node}<section class="repo-row">
        <h3>{String(node.current_fields.label || node.current_fields.name || 'Untitled')}</h3>
        {#if node.current_fields.hint}<p>{String(node.current_fields.hint)}</p>{/if}
      </section>{/each}{/if}
</div>

<style>
  .file-preview {
    overflow: auto;
    max-height: 850px;
    min-height: 260px;
  }
  .rich-preview :global(.ribbon-tabs),
  .rich-preview :global(.office-panel),
  .rich-preview :global(.ruler) {
    display: none;
  }
</style>
