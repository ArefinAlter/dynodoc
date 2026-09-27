<script lang="ts">
  import { byPosition } from '$lib/workspace/api';
  import RichEditor from '$lib/components/workspace/RichEditor.svelte';
  import SheetEditor from '$lib/components/workspace/SheetEditor.svelte';
  import PresentationEditor from '$lib/components/workspace/PresentationEditor.svelte';
  import { richContent } from '$lib/workspace/changes';
  import type { PageData } from './$types';
  let { data }: { data: PageData } = $props();
  let selectedTab = $state('');
  const tabs = $derived(
    Object.values(data.state.nodes)
      .filter(
        (n) =>
          !n.deleted &&
          (n.type === 'form' || (n.type === 'section' && n.current_fields.kind === 'document_tab'))
      )
      .sort((a, b) => (a.type === 'form' ? -1 : b.type === 'form' ? 1 : byPosition(a, b)))
  );
  const activeTab = $derived(tabs.find((n) => n.id === selectedTab) || tabs[0]);
  const nodes = $derived(
    Object.values(data.state.nodes)
      .filter((n) => !n.deleted)
      .sort((a, b) => byPosition(a, b))
  );
</script>

<svelte:head
  ><title>{data.title} · Shared with Dynodoc</title><meta
    name="robots"
    content="noindex,nofollow"
  /><meta name="referrer" content="no-referrer" /></svelte:head
>
<header>
  <a href="https://dynodoc.online"
    ><img src="/brand/logo.svg?v=2" width="145" height="32" alt="Dynodoc" /></a
  ><span>Published copy · View only</span>
</header>
<main>
  <h1>{data.title}</h1>
  <p class="notice">This is a saved copy shared by its owner. It does not include later edits.</p>
  {#if data.kind === 'document'}
    {#if tabs.length > 1}<nav class="document-tabs" aria-label="Document tabs">
        {#each tabs as tab}<button
            aria-pressed={activeTab?.id === tab.id}
            onclick={() => (selectedTab = tab.id)}
            >{tab.type === 'form'
              ? 'Main document'
              : String(tab.current_fields.label || 'Untitled tab')}</button
          >{/each}
      </nav>{/if}
    <div class="readonly-rich">
      <RichEditor
        content={richContent(data.state, activeTab?.id)}
        pageSettings={tabs.find((n) => n.type === 'form')?.current_fields.page_setup}
        editable={false}
        revision={tabs.findIndex((n) => n.id === activeTab?.id)}
        onChange={() => {}}
      />
    </div>{:else if data.kind === 'spreadsheet'}<SheetEditor
      documentState={data.state}
      editable={false}
      onChange={() => {}}
      onSelect={() => {}}
      onUndo={() => {}}
      onRedo={() => {}}
    />{:else if data.kind === 'presentation'}<div class="readonly-deck">
      <PresentationEditor documentState={data.state} editable={false} onChange={() => {}} />
    </div>{:else}<div class="questions">
      {#each nodes as node}{#if node.type !== 'form' && node.type !== 'choice'}<article>
            <h2>{String(node.current_fields.label || node.current_fields.name || 'Untitled')}</h2>
            {#if node.current_fields.hint}<p>{String(node.current_fields.hint)}</p>{/if}<small
              >{String(node.current_fields.type || node.type)}</small
            >{#each nodes.filter((c) => c.parent_id === node.id && c.type === 'choice') as choice}<p
              >
                {String(choice.current_fields.label || '')}
              </p>{/each}
          </article>{/if}{/each}
    </div>{/if}
</main>

<style>
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    padding: 18px 5vw;
    border-bottom: 1px solid #d3dfeb;
    background: #faf9f6;
    color: #425f7b;
    font-size: 14px;
  }
  main {
    padding: 30px 3vw;
    background: #eef3f8;
    min-height: 100vh;
  }
  h1,
  .notice {
    max-width: 980px;
    margin: 0 auto 18px;
  }
  h1 {
    font-size: 30px;
  }
  .notice {
    color: #526a7f;
    line-height: 1.6;
  }
  .readonly-rich :global(.toolbar),
  .readonly-rich :global(.menu-row) {
    display: none;
  }
  .document-tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    max-width: 980px;
    margin: 0 auto 18px;
  }
  .document-tabs button {
    border: 1px solid #b8c9db;
    border-radius: 6px;
    padding: 10px 16px;
    background: white;
    color: #29455f;
    font: inherit;
    cursor: pointer;
  }
  .document-tabs button[aria-pressed='true'] {
    background: #dbeaff;
    border-color: #3267b0;
  }
  .readonly-deck {
    display: flex;
    flex-direction: column;
    height: min(760px, 82vh);
    max-width: 1180px;
    margin: auto;
    border: 1px solid #d3dfeb;
    border-radius: 8px;
    overflow: hidden;
  }
  .questions {
    max-width: 850px;
    margin: auto;
  }
  .questions article {
    padding: 22px;
    background: white;
    border: 1px solid #d3dfeb;
    border-radius: 8px;
    margin: 12px 0;
  }
  .questions h2 {
    font-size: 20px;
  }
  .questions small {
    color: #526a7f;
  }
</style>
