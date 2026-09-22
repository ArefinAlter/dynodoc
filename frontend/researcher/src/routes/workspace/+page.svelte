<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import {
    Archive,
    FileText,
    Table2,
    ClipboardList,
    Search,
    Upload,
    Grid2X2,
    List,
    FolderOpen,
    Home,
    Settings2,
    Trash2,
    RotateCcw,
    CircleHelp,
    LogOut,
    X,
    AlertCircle,
    ArrowRight
  } from '@lucide/svelte';
  import GoogleFormsConnector from '$lib/components/product/GoogleFormsConnector.svelte';
  import SpaceManager from '$lib/components/product/SpaceManager.svelte';
  import type { Space } from '$lib/workspace/access';
  import ResearchConnector from '$lib/components/workspace/ResearchConnector.svelte';
  import type { ResearchFile, ResearchSource } from '$lib/workspace/connectors';
  import { createDocument } from '$lib/api/client';
  import type { Document } from '$lib/api/types';
  import { documentKind, saveBatch, workspaceRequest, type Kind } from '$lib/workspace/api';
  import { importFile, rootOp, itemOp, paragraph, type ImportResult } from '$lib/workspace/files';
  import { ulid } from 'ulid';
  import { clearSession } from '$lib/stores/session.svelte';
  type WorkspaceDocument = Document & {
    deleted_at?: string;
    space_id?: string;
    can_manage?: boolean;
    role?: string;
  };
  let spaces = $state<Space[]>([]),
    selectedSpace = $state(''),
    trash = $state(false);
  let documents = $state<WorkspaceDocument[]>([]),
    loading = $state(true),
    search = $state(''),
    filter = $state('all'),
    error = $state(''),
    busy = $state(false),
    listView = $state(true),
    sort = $state('updated'),
    createOpen = $state(false),
    title = $state(''),
    kind = $state<Kind>('document'),
    fileInput: HTMLInputElement;
  let importResult = $state<ImportResult | null>(null),
    originalFile = $state<File | null>(null),
    progress = $state('');
  let sidebarOpen = $state(false),
    toolsOpen = $state(false);
  let searchInput: HTMLInputElement;
  let importAbort = $state<AbortController | null>(null);
  let connectorOpen = $state(false),
    importedSource = $state<ResearchSource | null>(null);
  async function researchImport(file: File, info: ResearchFile) {
    busy = true;
    error = '';
    try {
      const result = await importFile(file);
      result.warnings.push(...info.warnings);
      importResult = result;
      originalFile = file;
      importedSource = info.source;
      title = file.name.replace(/\.[^.]+$/, '').replaceAll('_', ' ');
    } finally {
      busy = false;
    }
  }
  let matches = $state<{ document_id: string; title: string; node_id: string; text: string }[]>([]);
  $effect(() => {
    const term = search.trim();
    if (term.length < 2) {
      matches = [];
      return;
    }
    let cancelled = false;
    const timer = setTimeout(async () => {
      try {
        const result = await workspaceRequest<{ items: typeof matches }>(
          `/search?q=${encodeURIComponent(term)}`
        );
        if (!cancelled) matches = result.items;
      } catch {
        if (!cancelled) matches = [];
      }
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });
  const visible = $derived(
    documents
      .filter(
        (doc) =>
          (doc.title.toLowerCase().includes(search.toLowerCase()) ||
            matches.some((m) => m.document_id === doc.id)) &&
          (trash || filter === 'archived'
            ? trash || doc.status === 'archived'
            : doc.status !== 'archived' && (filter === 'all' || documentKind(doc) === filter))
      )
      .sort((a, b) =>
        sort === 'title'
          ? a.title.localeCompare(b.title)
          : new Date(sort === 'created' ? b.created_at : b.updated_at).getTime() -
            new Date(sort === 'created' ? a.created_at : a.updated_at).getTime()
      )
  );
  onMount(refresh);
  onMount(() => {
    const focusSearch = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        searchInput?.focus();
      }
      if (event.key === 'Escape') sidebarOpen = false;
    };
    window.addEventListener('keydown', focusSearch);
    return () => window.removeEventListener('keydown', focusSearch);
  });
  async function refresh() {
    loading = true;
    try {
      const all: WorkspaceDocument[] = [];
      let offset = 0;
      for (;;) {
        const query = new URLSearchParams({ trash: String(trash), offset: String(offset) });
        if (selectedSpace) query.set('space_id', selectedSpace);
        const page = await workspaceRequest<{ items: WorkspaceDocument[]; more: boolean }>(
          `/workspace/index?${query}`
        );
        all.push(...page.items);
        if (!page.more) break;
        offset += page.items.length;
      }
      documents = all;
      spaces = (await workspaceRequest<{ items: Space[] }>('/spaces')).items;
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not load documents.';
    } finally {
      loading = false;
    }
  }
  async function setTrash(value: boolean) {
    trash = value;
    filter = 'all';
    await refresh();
  }
  async function changeLocation(id: string) {
    selectedSpace = id;
    await refresh();
  }
  async function changeTrash(doc: WorkspaceDocument) {
    if (
      !trash &&
      !confirm(
        `Move “${doc.title}” to Trash? Public links will stop working. You can restore the document later.`
      )
    )
      return;
    busy = true;
    try {
      await workspaceRequest(`/documents/${doc.id}/trash`, { deleted: !trash });
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not update document';
    } finally {
      busy = false;
    }
  }
  async function create() {
    if (!title.trim()) return;
    busy = true;
    error = '';
    try {
      const doc = await createDocument({ title: title.trim() });
      if (selectedSpace)
        await workspaceRequest(`/documents/${doc.id}/location`, { space_id: selectedSpace });
      await workspaceRequest(`/documents/${doc.id}/metadata`, { settings: { kind } });
      const root = ulid();
      let ops = [rootOp(root, title)];
      if (kind === 'document') ops.push(itemOp(root, 0, { label: '', content: paragraph('') }));
      if (kind === 'spreadsheet') {
        const sheet = ulid();
        ops.push({
          type: 'NodeCreated',
          node_id: sheet,
          node_type: 'section',
          parent_id: root,
          pos: '00000000',
          fields: { label: 'Sheet 1', kind: 'sheet', columns: 30 }
        });
        for (let i = 0; i < 100; i++) ops.push(itemOp(sheet, i, {}));
      }
      await saveBatch(doc.id, ops, 0);
      await goto(`/documents/${doc.id}`);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not create document.';
    } finally {
      busy = false;
    }
  }
  async function chooseFile(event: Event) {
    const file = (event.target as HTMLInputElement).files?.[0];
    if (!file) return;
    importedSource = null;
    busy = true;
    error = '';
    progress = 'Reading your file…';
    importAbort = new AbortController();
    try {
      importResult = await importFile(file, importAbort?.signal);
      originalFile = file;
      title = file.name.replace(/\.[^.]+$/, '');
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not read this file.';
    } finally {
      busy = false;
      progress = '';
      importAbort = null;
      if (fileInput) fileInput.value = '';
    }
  }
  async function example(filename: string) {
    importedSource = null;
    busy = true;
    error = '';
    try {
      const response = await fetch(`/examples/${filename}`);
      if (!response.ok) throw new Error('Example file unavailable');
      const file = new File([await response.blob()], filename);
      importResult = await importFile(file);
      originalFile = file;
      title = filename.replace(/\.[^.]+$/, '').replaceAll('-', ' ');
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not open example';
    } finally {
      busy = false;
    }
  }
  async function finishImport() {
    if (!importResult || !originalFile) return;
    busy = true;
    error = '';
    try {
      progress = 'Creating your document…';
      const doc = await createDocument({ title: title.trim() || importResult.filename });
      if (selectedSpace)
        await workspaceRequest(`/documents/${doc.id}/location`, { space_id: selectedSpace });
      await workspaceRequest(`/documents/${doc.id}/metadata`, {
        settings: {
          kind: importResult.kind,
          import_warnings: importResult.warnings,
          research_source: importedSource
        }
      });
      const bytes = new Uint8Array(await originalFile.arrayBuffer());
      let binary = '';
      for (let i = 0; i < bytes.length; i += 8192)
        binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
      await workspaceRequest(`/documents/${doc.id}/uploads`, {
        filename: originalFile.name,
        content_type: originalFile.type || 'application/octet-stream',
        content: btoa(binary),
        report: { warnings: importResult.warnings, research_source: importedSource }
      });
      progress = 'Importing into the workspace…';
      await saveBatch(doc.id, importResult.ops, 0);
      await goto(`/documents/${doc.id}`);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Import failed.';
    } finally {
      busy = false;
      progress = '';
    }
  }
  async function logout() {
    await clearSession();
    await goto('/login');
  }
  function relative(date: string) {
    const days = Math.floor((Date.now() - new Date(date).getTime()) / 86400000);
    return days < 1
      ? 'Today'
      : days === 1
        ? 'Yesterday'
        : new Date(date).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  }
</script>

<svelte:head
  ><title>Your workspace · Dynodoc</title><meta name="robots" content="noindex" /></svelte:head
>
<div class="workspace" class:spreadsheet-home={filter === 'spreadsheet'}>
  <header class="app-title">
    <a href="https://dynodoc.online"
      ><img src="/brand/mark.svg?v=2" alt="" width="21" height="21" />Dynodoc</a
    >
    <div>
      <button
        aria-label="Help & tutorials"
        title="Help & tutorials"
        onclick={() => window.dispatchEvent(new Event('dynodoc:help'))}
        ><CircleHelp size={17} /></button
      ><button title="Sign out" aria-label="Sign out" onclick={logout}><LogOut size={17} /></button>
    </div>
  </header>
  <aside class="sidebar" class:open={sidebarOpen}>
    <nav aria-label="Workspace">
      <button
        class:active={!trash && filter === 'all'}
        onclick={() => {
          filter = 'all';
          void setTrash(false);
        }}><Home size={23} strokeWidth={1.3} /><span>Home</span></button
      >
      <button
        aria-label="New document"
        onclick={() => {
          createOpen = true;
          title = '';
          kind = 'document';
        }}><FileText size={23} strokeWidth={1.3} /><span>New</span></button
      >
      <button aria-label="Upload a file" disabled={busy} onclick={() => fileInput.click()}
        ><FolderOpen size={24} strokeWidth={1.3} /><span>Open</span></button
      >
      <button class:active={trash} onclick={() => void setTrash(!trash)}
        ><Trash2 size={22} strokeWidth={1.3} /><span>Trash</span></button
      >
    </nav>
    <button
      class="settings"
      title="Workspace options"
      aria-label="Workspace options"
      aria-expanded={toolsOpen}
      onclick={() => (toolsOpen = !toolsOpen)}
      ><Settings2 size={22} strokeWidth={1.3} /><span>Options</span></button
    >
  </aside>
  <main>
    {#if toolsOpen}<section class="workspace-tools" aria-label="Workspace options">
        <SpaceManager
          {spaces}
          selected={selectedSpace}
          onSelect={(id) => void changeLocation(id)}
          onRefresh={refresh}
        />
        <div class="import-tools">
          <button onclick={() => (connectorOpen = true)} disabled={busy}
            ><Upload size={15} />Import from research tool</button
          ><GoogleFormsConnector onImport={researchImport} /><button
            onclick={() => (filter = 'archived')}><Archive size={15} />Archived documents</button
          >
        </div>
      </section>{/if}
    <div class="content">
      <h1>{trash ? 'Trash' : 'Home'}</h1>
      <input
        class="file-input"
        bind:this={fileInput}
        type="file"
        accept=".docx,.doc,.xlsx,.csv,.txt,.md,.tex,.zip"
        onchange={chooseFile}
      />
      {#if !trash}<section class="new-section" aria-labelledby="new-title">
          <h2 id="new-title">New</h2>
          <div class="starters">
            {#each [{ kind: 'document', label: 'Blank document', icon: FileText }, { kind: 'spreadsheet', label: 'Blank workbook', icon: Table2 }, { kind: 'questionnaire', label: 'Blank questionnaire', icon: ClipboardList }] as item}
              <button
                class:sheet={item.kind === 'spreadsheet'}
                disabled={busy}
                onclick={() => {
                  kind = item.kind as Kind;
                  title = '';
                  createOpen = true;
                }}
              >
                <div class="template-paper" class:workbook={item.kind === 'spreadsheet'}>
                  {#if item.kind !== 'document'}<item.icon size={33} strokeWidth={1} />{/if}
                </div>
                <span>{item.label}</span>
              </button>
            {/each}
            {#each [{ file: 'community-interview-guide.docx', label: 'Interview guide', kind: 'document' }, { file: 'fieldwork-tracker.xlsx', label: 'Fieldwork tracker', kind: 'spreadsheet' }, { file: 'community-questionnaire.xlsx', label: 'Questionnaire', kind: 'questionnaire' }] as template}
              <button
                aria-label={template.label}
                disabled={busy}
                onclick={() => void example(template.file)}
                ><div
                  class="template-paper sample"
                  class:workbook={template.kind === 'spreadsheet'}
                >
                  <strong>{template.label}</strong>{#each [1, 2, 3, 4, 5] as line}<i
                      style:width={line === 5 ? '60%' : '100%'}
                    ></i>{/each}
                </div>
                <span>{template.label}</span></button
              >
            {/each}
          </div>
        </section>{/if}
      <div class="list-heading">
        <div class="filters" aria-label="Document types">
          {#each [{ value: 'all', label: 'Recent' }, { value: 'document', label: 'Documents' }, { value: 'spreadsheet', label: 'Spreadsheets' }, { value: 'questionnaire', label: 'Questionnaires' }] as item}<button
              class:active={filter === item.value}
              aria-pressed={filter === item.value}
              onclick={() => (filter = item.value)}>{item.label}</button
            >{/each}
        </div>
        <div class="search">
          <Search size={17} /><input
            bind:this={searchInput}
            aria-label="Search documents"
            bind:value={search}
            placeholder="Search for a file"
          /><kbd>Ctrl K</kbd>
        </div>
        <button
          class="view-button"
          aria-label={listView ? 'Grid view' : 'List view'}
          title={listView ? 'Grid view' : 'List view'}
          onclick={() => (listView = !listView)}
          >{#if listView}<Grid2X2 size={17} />{:else}<List size={17} />{/if}</button
        >
      </div>
      <div class="column-head">
        <span>Name</span><select aria-label="Sort documents" bind:value={sort}
          ><option value="updated">Date modified</option><option value="created"
            >Date created</option
          ><option value="title">Name A–Z</option></select
        >
      </div>
      {#if error}<div class="error" role="alert">
          <AlertCircle size={17} />{error}<button
            aria-label="Dismiss error"
            onclick={() => (error = '')}><X size={14} /></button
          >
        </div>{/if}
      {#if busy}<p class="progress" role="status">
          {progress || 'Saving…'}{#if importAbort}<button onclick={() => importAbort?.abort()}
              >Cancel import</button
            >{/if}
        </p>{/if}
      {#if loading}<p class="empty">Loading…</p>
      {:else if !visible.length}<div class="empty">
          <FolderOpen size={30} strokeWidth={1} />
          <h3>{search ? 'No matching files' : trash ? 'Trash is empty' : 'No recent files'}</h3>
          {#if !search && !trash}<button
              onclick={() => {
                title = '';
                createOpen = true;
              }}>Create a document</button
            >{/if}
        </div>
      {:else}<div class:list={listView} class="document-grid">
          {#each visible as doc}<article class="doc-entry">
              <a class="doc-card" href={trash ? undefined : `/documents/${doc.id}`}>
                <span class="doc-icon" class:green={documentKind(doc) === 'spreadsheet'}
                  >{#if documentKind(doc) === 'spreadsheet'}<Table2
                      size={26}
                      strokeWidth={1.3}
                    />{:else if documentKind(doc) === 'questionnaire'}<ClipboardList
                      size={26}
                      strokeWidth={1.3}
                    />{:else}<FileText size={26} strokeWidth={1.3} />{/if}</span
                >
                <div class="doc-details">
                  <h3>{doc.title}</h3>
                  <p>{spaces.find((space) => space.id === doc.space_id)?.name || 'My workspace'}</p>
                </div>
                <time datetime={doc.updated_at} title={new Date(doc.updated_at).toLocaleString()}
                  >{relative(doc.updated_at)}</time
                >
              </a>
              {#if doc.can_manage}<button
                  class="trash-action"
                  disabled={busy}
                  title={trash ? 'Restore document' : 'Move to Trash'}
                  aria-label={trash ? 'Restore document' : 'Move to Trash'}
                  onclick={() => void changeTrash(doc)}
                  >{#if trash}<RotateCcw size={16} />{:else}<Trash2 size={16} />{/if}</button
                >{/if}
            </article>{/each}
        </div>{/if}
    </div>
  </main>
</div>
{#if connectorOpen}<ResearchConnector
    onImport={researchImport}
    onClose={() => (connectorOpen = false)}
  />{/if}
{#if createOpen || importResult}<div class="modal-backdrop" role="presentation">
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="create-title" tabindex="-1">
      <button
        class="close"
        aria-label="Close"
        disabled={busy}
        onclick={() => {
          createOpen = false;
          importResult = null;
          originalFile = null;
        }}><X size={20} /></button
      >
      <div class="modal-icon">
        {#if importResult}<Upload size={24} />{:else}<FileText size={24} />{/if}
      </div>
      <h2 id="create-title">{importResult ? 'Review your import' : 'A new place to think.'}</h2>
      <p>
        {importResult
          ? 'Review what will transfer before creating your editable copy.'
          : 'Give your work a name. You can change it later.'}
      </p>
      <form
        onsubmit={(e) => {
          e.preventDefault();
          void (importResult ? finishImport() : create());
        }}
      >
        <label
          >Document name<input
            bind:value={title}
            placeholder="e.g. Our next chapter"
            required
            maxlength="200"
          /></label
        >{#if !importResult}<label
            >Document type<select aria-label="Document type" bind:value={kind}
              ><option value="document">Document</option><option value="spreadsheet"
                >Spreadsheet</option
              ><option value="questionnaire">Questionnaire</option></select
            ></label
          >{:else}<div class="import-report">
            <b>{importResult.filename}</b>
            <p>{importResult.ops.length - 1} blocks prepared. Your original file will be kept.</p>
            {#each importResult.warnings as warning}<p class="warning">
                <AlertCircle size={14} />{warning}
              </p>{/each}
          </div>{/if}{#if error}<p class="error" role="alert">{error}</p>{/if}<button
          class="confirm"
          disabled={busy}
          >{busy
            ? progress || 'Creating…'
            : importResult
              ? 'Import into workspace'
              : 'Create document'}<ArrowRight size={16} /></button
        >
      </form>
    </div>
  </div>{/if}

<style>
  .spreadsheet-home {
    --workspace-accent: #107c41;
  }

  .workspace {
    min-height: 100dvh;
    background: #f5f5f5;
    color: #292929;
    font:
      13px 'Segoe UI',
      Arial,
      sans-serif;
  }
  button,
  input,
  select {
    font: inherit;
  }
  button {
    cursor: pointer;
  }
  button:focus-visible,
  a:focus-visible {
    outline: 2px solid var(--workspace-accent, #185abd);
    outline-offset: 3px;
  }
  .app-title {
    height: 46px;
    background: #eeeeee;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 18px;
    border-bottom: 1px solid #e8e8e8;
  }
  .app-title a {
    display: inline-flex;
    align-items: center;
    gap: 12px;
    text-decoration: none;
    color: inherit;
  }
  .app-title > div {
    display: flex;
    gap: 25px;
    align-items: center;
  }
  .app-title button {
    border: 0;
    padding: 4px;
    background: transparent;
  }
  .sidebar {
    position: fixed;
    inset: 46px auto 0 0;
    width: 68px;
    border-right: 1px solid #e3e3e3;
    display: flex;
    flex-direction: column;
    padding: 5px 4px 12px;
    background: #f0f0f0;
  }
  .sidebar nav {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .sidebar button {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 3px;
    border: 0;
    border-left: 2px solid transparent;
    background: transparent;
    color: #333;
    font-size: 12px;
    min-height: 57px;
    width: 100%;
  }
  .sidebar button.active {
    border-left-color: var(--workspace-accent, #185abd);
    color: var(--workspace-accent, #185abd);
  }
  .sidebar button:hover {
    background: #e4e4e4;
  }
  .sidebar .settings {
    margin-top: auto;
  }
  main {
    margin-left: 68px;
  }
  .content {
    max-width: 1740px;
    padding: 18px 48px 40px;
    margin: auto;
  }
  h1 {
    font-size: 23px;
    font-weight: 600;
    margin: 0 0 23px;
  }
  h2 {
    font-size: 18px;
    font-weight: 600;
    margin: 0 0 13px;
  }
  .file-input {
    display: none;
  }
  .starters {
    display: flex;
    gap: 20px;
    overflow-x: auto;
    padding: 2px 0 8px;
    margin-bottom: 34px;
  }
  .starters > button {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 15px;
    min-width: 158px;
    width: 180px;
    height: 154px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 2px;
    color: inherit;
    flex-shrink: 0;
  }
  .starters > button:first-child {
    border-color: #444;
    background: #ececec;
  }
  .starters > button:hover {
    background: #e9e9e9;
    border-color: #777;
  }
  .template-paper {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    width: 71px;
    height: 92px;
    background: white;
    border: 1px solid #c9c9c9;
    color: var(--workspace-accent, #185abd);
  }
  .template-paper.workbook {
    color: #107c41;
    background:
      repeating-linear-gradient(0deg, transparent 0, transparent 8px, #dbe7df 9px),
      repeating-linear-gradient(90deg, white 0, white 14px, #dbe7df 15px);
  }
  .template-paper.sample {
    align-items: stretch;
    padding: 9px 7px;
    justify-content: start;
    gap: 6px;
  }
  .template-paper strong {
    font-size: 6px;
    margin: 0 0 7px;
  }
  .template-paper i {
    display: block;
    height: 1px;
    background: #ccd2d8;
  }
  .template-paper i:last-child {
    width: 60%;
  }
  .list-heading {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    margin-bottom: 6px;
  }
  .filters {
    display: flex;
    gap: 9px;
  }
  .filters button {
    border: 1px solid #999;
    border-radius: 18px;
    padding: 7px 13px;
    background: white;
  }
  .filters button.active {
    background: var(--workspace-accent, #185abd);
    border-color: var(--workspace-accent, #185abd);
    color: white;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    width: min(450px, 40%);
    margin-left: auto;
    border: 1px solid #999;
    background: white;
    padding: 6px 10px;
    border-radius: 3px;
  }
  .search input {
    border: 0;
    outline: none;
    min-width: 0;
    width: 100%;
    font-size: 12px;
  }
  .search:focus-within {
    outline: 2px solid var(--workspace-accent, #185abd);
  }
  kbd {
    color: #666;
    font-size: 10px;
    white-space: nowrap;
  }
  .view-button {
    background: transparent;
    border: 0;
    color: #444;
    padding: 6px;
  }
  .column-head {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    padding: 8px 35px 8px 0;
    border-bottom: 1px solid #ccc;
  }
  .column-head select {
    border: 0;
    background: transparent;
    color: inherit;
    width: 185px;
  }
  .document-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 14px;
    padding: 14px 0;
  }
  .document-grid.list {
    display: block;
    padding: 0;
  }
  .doc-entry {
    display: flex;
    align-items: center;
    position: relative;
    border: 1px solid #d0d0d0;
    padding: 12px;
    background: #fafafa;
    min-width: 0;
  }
  .list .doc-entry {
    border: 0;
    border-bottom: 1px solid #cfcfcf;
    background: transparent;
    padding: 0 0 0 8px;
  }
  .doc-entry:hover {
    background: #eaeaea;
  }
  .doc-card {
    display: flex;
    align-items: center;
    min-width: 0;
    flex: 1;
    color: inherit;
    text-decoration: none;
    gap: 16px;
    padding: 12px 0;
    flex-wrap: wrap;
  }
  .list .doc-card {
    flex-wrap: nowrap;
  }
  .doc-icon {
    color: var(--workspace-accent, #185abd);
    flex-shrink: 0;
    display: flex;
  }
  .doc-icon.green {
    color: #107c41;
  }
  .doc-details {
    min-width: 0;
    flex: 1;
  }
  .doc-details h3 {
    font-size: 14px;
    font-weight: 400;
    margin: 0 0 3px;
    text-overflow: ellipsis;
    overflow: hidden;
    white-space: nowrap;
  }
  .doc-details p {
    margin: 0;
    font-size: 11px;
  }
  time {
    font-size: 12px;
    width: 185px;
    flex-shrink: 0;
  }
  .trash-action {
    display: grid;
    place-items: center;
    margin: 0 8px;
    border: 0;
    padding: 7px;
    color: #555;
    background: transparent;
  }
  .trash-action:hover {
    background: #d6d6d6;
  }
  .workspace-tools {
    display: flex;
    gap: 35px;
    padding: 20px 48px;
    background: white;
    border-bottom: 1px solid #ddd;
  }
  .import-tools {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    align-items: start;
  }
  .import-tools > button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px;
    border: 1px solid #ccc;
    background: #fafafa;
  }
  .empty {
    padding: 55px 10px;
    color: #666;
    text-align: center;
  }
  .empty h3 {
    font-size: 16px;
    font-weight: 400;
  }
  .empty button {
    border: 1px solid #b7c9e3;
    color: var(--workspace-accent, #185abd);
    padding: 8px 12px;
    background: white;
    border-radius: 3px;
  }
  .error {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px;
    background: #fff0ed;
    color: #933b2a;
  }
  .error button {
    margin-left: auto;
    border: 0;
    background: transparent;
  }
  .progress {
    color: var(--workspace-accent, #185abd);
  }
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: #16251d55;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 25px;
    backdrop-filter: blur(4px);
    font-family: var(--font-ui);
  }
  .modal {
    max-width: 500px;
    width: 100%;
    background: white;
    border-radius: 14px;
    padding: 30px;
    position: relative;
    max-height: 90vh;
    overflow: auto;
    box-shadow: 0 20px 100px #12251822;
  }
  .close {
    position: absolute;
    top: 18px;
    right: 18px;
    border: 0;
    background: none;
    color: #46617a;
    cursor: pointer;
  }
  .modal-icon {
    background: #edf4e9;
    color: #46617a;
    width: 46px;
    height: 46px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    margin-bottom: 20px;
  }
  .modal h2 {
    font: 29px var(--font-ui);
    color: #48603e;
    margin: 0 0 10px;
  }
  .modal > p {
    font-size: 13px;
    color: #46617a;
    line-height: 1.7;
  }
  .modal form {
    display: grid;
    gap: 16px;
    margin-top: 24px;
  }
  .modal label {
    display: grid;
    gap: 7px;
    font-size: 13px;
    color: #46617a;
  }
  .modal input,
  .modal select {
    padding: 11px;
    border: 1px solid #dde5d6;
    border-radius: 10px;
    color: #54684a;
    background: white;
    font-size: 13px;
    width: 100%;
  }
  .confirm {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    background: #285440;
    color: white;
    border: 0;
    border-radius: 12px;
    font-size: 13px;
    cursor: pointer;
    margin-top: 6px;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .import-report {
    background: #f7f9f3;
    border: 1px solid #e6ecdd;
    padding: 14px;
    border-radius: 8px;
    font-size: 13px;
    color: #46617a;
  }
  .import-report b {
    font-size: 13px;
  }
  .import-report p {
    font-size: 13px;
    line-height: 1.7;
  }
  .warning {
    display: flex;
    gap: 6px;
    color: #46617a;
  }
  .warning > :global(svg) {
    flex-shrink: 0;
    margin-top: 3px;
  }
  @media (max-width: 900px) {
    .content {
      padding: 20px 24px;
    }
    .search {
      width: 100%;
      order: 3;
    }
    .filters {
      flex-wrap: wrap;
    }
    time {
      width: 125px;
    }
    .column-head select {
      width: 130px;
    }
    .workspace-tools {
      padding: 16px;
      flex-direction: column;
    }
  }
  @media (max-width: 600px) {
    .content {
      padding: 18px 14px;
    }
    .starters {
      gap: 8px;
    }
    .starters > button {
      min-width: 135px;
    }
    .list .doc-card {
      gap: 9px;
    }
    .list time {
      display: none;
    }
    .filters button {
      font-size: 11px;
      padding: 5px 8px;
    }
  }
</style>
