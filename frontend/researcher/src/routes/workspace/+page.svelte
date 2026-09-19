<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import {
    Archive,
    FileText,
    Table2,
    ClipboardList,
    Plus,
    Search,
    Upload,
    ArrowUpRight,
    Grid2X2,
    List,
    FolderOpen,
    Clock3,
    Users,
    LogOut,
    X,
    AlertCircle,
    ArrowRight,
    Check
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
    listView = $state(false),
    createOpen = $state(false),
    title = $state(''),
    kind = $state<Kind>('document'),
    fileInput: HTMLInputElement;
  let importResult = $state<ImportResult | null>(null),
    originalFile = $state<File | null>(null),
    progress = $state('');
  let sidebarOpen = $state(false);
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
    documents.filter(
      (doc) =>
        (doc.title.toLowerCase().includes(search.toLowerCase()) ||
          matches.some((m) => m.document_id === doc.id)) &&
        (trash || filter === 'archived'
          ? trash || doc.status === 'archived'
          : doc.status !== 'archived' && (filter === 'all' || documentKind(doc) === filter))
    )
  );
  onMount(refresh);
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
          fields: { label: 'Sheet 1', kind: 'sheet', columns: 12 }
        });
        for (let i = 0; i < 30; i++) ops.push(itemOp(sheet, i, {}));
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
    try {
      importResult = await importFile(file);
      originalFile = file;
      title = file.name.replace(/\.[^.]+$/, '');
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not read this file.';
    } finally {
      busy = false;
      progress = '';
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
<div class="workspace">
  <aside class="sidebar" class:open={sidebarOpen}>
    <button class="mobile-close" onclick={() => (sidebarOpen = false)}>Close navigation</button>
    <a class="brand" href="https://dynodoc.online"
      ><img src="/brand/mark.svg?v=2" alt="" width="32" height="32" /><b>dynodoc</b></a
    >
    <div class="team">
      <span class="team-icon">R</span>
      <div><strong>Research workspace</strong><small>Your team’s shared home</small></div>
    </div>
    <button
      class="new-button"
      onclick={() => {
        createOpen = true;
        title = '';
      }}><Plus size={18} /> New document</button
    >
    <nav aria-label="Workspace">
      <button class:active={filter === 'all'} onclick={() => (filter = 'all')}
        ><FolderOpen size={17} /> All documents <span>{documents.length}</span></button
      ><button class:active={filter === 'document'} onclick={() => (filter = 'document')}
        ><FileText size={17} /> Documents</button
      ><button class:active={filter === 'spreadsheet'} onclick={() => (filter = 'spreadsheet')}
        ><Table2 size={17} /> Spreadsheets</button
      ><button class:active={filter === 'questionnaire'} onclick={() => (filter = 'questionnaire')}
        ><ClipboardList size={17} /> Questionnaires</button
      >
      <button class:active={filter === 'archived'} onclick={() => (filter = 'archived')}
        ><Archive size={17} /> Archived</button
      >
      <button class:active={trash} onclick={() => void setTrash(!trash)}
        ><Archive size={17} />{trash ? 'Back to documents' : 'Trash'}</button
      >
    </nav>
    <SpaceManager
      {spaces}
      selected={selectedSpace}
      onSelect={(id) => void changeLocation(id)}
      onRefresh={refresh}
    />
    <button class="upload" onclick={() => (connectorOpen = true)} disabled={busy}
      >Import from research tool</button
    >
    <GoogleFormsConnector onImport={researchImport} />
    <div class="side-bottom">
      <div class="workspace-tip">
        <Users size={18} />
        <p>Better questions start<br />with more perspectives.</p>
        <small>Open a document and select Share to invite your team.</small>
      </div>
      <button onclick={logout}><LogOut size={16} /> Sign out</button>
    </div>
  </aside>
  <main>
    <header class="top">
      <button
        class="mobile-nav"
        aria-expanded={sidebarOpen}
        onclick={() => (sidebarOpen = !sidebarOpen)}>Workspaces</button
      >
      <div class="search">
        <Search size={17} /><input
          aria-label="Search documents"
          bind:value={search}
          placeholder="Find a document, table, or questionnaire…"
        /><kbd>Search</kbd>
      </div>
      <a href="https://dynodoc.online">About Dynodoc <ArrowUpRight size={14} /></a>
    </header>
    <div class="content">
      <div class="heading">
        <div>
          <p class="eyebrow">A LITTLE LESS FILE CHAOS</p>
          <h1>Your next finding starts here.</h1>
          <p>A shared place for drafts, questions, and the decisions in between.</p>
        </div>
        <button class="upload" onclick={() => fileInput.click()} disabled={busy}
          ><Upload size={16} /> Upload a file</button
        ><input
          class="file-input"
          bind:this={fileInput}
          type="file"
          accept=".docx,.doc,.xlsx,.csv,.txt,.md,.tex,.zip"
          onchange={chooseFile}
        />
      </div>
      <div class="starters">
        <button
          onclick={() => {
            kind = 'document';
            title = '';
            createOpen = true;
          }}
          ><div class="starter-icon blue"><FileText size={25} /></div>
          <div>
            <b>Start writing</b>
            <p>A guide, protocol, or field note</p>
          </div>
          <Plus size={16} /></button
        ><button
          onclick={() => {
            kind = 'spreadsheet';
            title = '';
            createOpen = true;
          }}
          ><div class="starter-icon green"><Table2 size={25} /></div>
          <div>
            <b>Make a spreadsheet</b>
            <p>Organize your questions and data</p>
          </div>
          <Plus size={16} /></button
        ><button
          onclick={() => {
            kind = 'questionnaire';
            title = '';
            createOpen = true;
          }}
          ><div class="starter-icon amber"><ClipboardList size={25} /></div>
          <div>
            <b>Build a questionnaire</b>
            <p>Questions, choices, and logic</p>
          </div>
          <Plus size={16} /></button
        >
      </div>
      <div class="example-links">
        <span>Try an example</span><button
          disabled={busy}
          onclick={() => void example('community-interview-guide.docx')}>Interview guide</button
        ><button disabled={busy} onclick={() => void example('fieldwork-tracker.xlsx')}
          >Fieldwork tracker</button
        ><button disabled={busy} onclick={() => void example('community-questionnaire.xlsx')}
          >Questionnaire</button
        >
      </div>
      <div class="list-heading">
        <h2>
          {trash
            ? 'Trash'
            : filter === 'all'
              ? 'Your documents'
              : filter === 'document'
                ? 'Documents'
                : filter === 'spreadsheet'
                  ? 'Spreadsheets'
                  : 'Questionnaires'} <span>{visible.length}</span>
        </h2>
        <div>
          <span>Recently updated</span><button
            aria-label="Grid view"
            class:chosen={!listView}
            onclick={() => (listView = false)}><Grid2X2 size={16} /></button
          ><button aria-label="List view" class:chosen={listView} onclick={() => (listView = true)}
            ><List size={16} /></button
          >
        </div>
      </div>
      {#if error}<div class="error" role="alert">
          <AlertCircle size={17} />{error}<button
            onclick={() => (error = '')}
            aria-label="Dismiss error"><X size={14} /></button
          >
        </div>{/if}
      {#if busy}<p class="progress" role="status">{progress || 'Saving…'}</p>{/if}
      {#if loading}<div class="empty">
          Loading your workspace…
        </div>{:else if visible.length === 0}<div class="empty">
          <div><FolderOpen size={35} /></div>
          <h3>{search ? 'No matching documents.' : 'A fresh page for your team.'}</h3>
          <p>
            {search
              ? 'Try a different title.'
              : 'Create a document or bring in a file to get started.'}
          </p>
          {#if !search}<button
              onclick={() => {
                title = '';
                createOpen = true;
              }}>Create your first document <ArrowRight size={15} /></button
            >{/if}
        </div>{:else}<div class:list={listView} class="document-grid">
          {#each visible as doc}<article class="doc-entry">
              <a class="doc-card" href={trash ? undefined : `/documents/${doc.id}`}
                ><div class="doc-preview" class:sheet={documentKind(doc) === 'spreadsheet'}>
                  <div class="preview-paper">
                    {#if documentKind(doc) === 'spreadsheet'}<div class="preview-grid">
                        {#each Array.from({ length: 35 }, (_, i) => i) as cell (cell)}<span
                          ></span>{/each}
                      </div>{:else}<div class="preview-line title"></div>
                      <div class="preview-line"></div>
                      <div class="preview-line"></div>
                      <div class="preview-line half"></div>
                      <div class="preview-gap"></div>
                      <div class="preview-line"></div>
                      <div class="preview-line"></div>
                      <div class="preview-line half"></div>{/if}
                  </div>
                  <span class="type-badge">{documentKind(doc)}</span>
                </div>
                <div class="doc-details">
                  <span class:green={documentKind(doc) === 'spreadsheet'} class="doc-icon"
                    >{#if documentKind(doc) === 'spreadsheet'}<Table2
                        size={19}
                      />{:else if documentKind(doc) === 'questionnaire'}<ClipboardList
                        size={19}
                      />{:else}<FileText size={19} />{/if}</span
                  >
                  <div>
                    <h3>{doc.title}</h3>
                    <p>
                      <Clock3 size={10} />
                      {relative(doc.updated_at)}<span>·</span>{doc.status === 'deployed'
                        ? 'Published version'
                        : 'Team document'}
                    </p>
                  </div>
                  <ArrowUpRight size={14} />
                </div></a
              >
              {#if doc.can_manage}<button
                  class="trash-action"
                  disabled={busy}
                  onclick={() => void changeTrash(doc)}
                  >{trash ? 'Restore document' : 'Move to Trash'}</button
                >{/if}
            </article>{/each}
        </div>{/if}
      <div class="workspace-footer">
        <span><Check size={12} /> Your document history stays with your work.</span><a
          href="https://dynodoc.online/#workflow">How Dynodoc works <ArrowUpRight size={12} /></a
        >
      </div>
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
            placeholder="e.g. Household wellbeing study"
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
  .doc-entry {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .doc-entry .doc-card {
    flex: 1;
  }
  .trash-action {
    background: #f7f8fa;
    color: #354f68;
    border: 1px solid #d4dfe8;
    border-top: 0;
    border-radius: 0 0 8px 8px;
    padding: 9px;
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .doc-entry:has(.trash-action) .doc-card {
    border-radius: 8px 8px 0 0;
  }
  .list .doc-entry {
    display: block;
  }
  .list .trash-action {
    width: 100%;
  }

  .example-links {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
    margin: 18px 0 32px;
    color: #46617a;
    font-size: 13px;
  }
  .example-links button {
    border: 0;
    background: none;
    color: #32659b;
    padding: 0;
    text-decoration: underline;
    text-underline-offset: 4px;
    font-size: 13px;
    cursor: pointer;
  }
  .workspace {
    display: flex;
    min-height: 100vh;
    background: #faf9f6;
    font-family: var(--font-ui);
    color: #293846;
    font-size: 13px;
  }
  .sidebar {
    width: 240px;
    position: fixed;
    inset: 0 auto 0 0;
    padding: 28px 19px 18px;
    background: #fff;
    border-right: 1px solid #e8ebef;
    display: flex;
    flex-direction: column;
    z-index: 10;
  }
  .brand {
    text-decoration: none;
    display: flex;
    align-items: baseline;
    font: bold 39px/1 var(--font-ui);
    color: #20559b;
    margin: 0 9px 34px;
  }
  .brand b {
    font: 600 23px var(--font-ui);
    letter-spacing: -1px;
    margin-left: 9px;
  }
  .team {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 13px 8px;
    margin-bottom: 20px;
  }
  .team-icon {
    background: #eaf1fb;
    color: #486b97;
    border: 1px solid #d1e0f3;
    border-radius: 8px;
    width: 33px;
    height: 33px;
    display: grid;
    place-items: center;
    font-weight: 600;
  }
  .team strong {
    display: block;
    font-size: 13px;
    font-weight: 500;
  }
  .team small {
    color: #46617a;
    font-size: 13px;
    display: block;
    margin-top: 3px;
  }
  .new-button {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 12px 14px;
    border: 0;
    border-radius: 7px;
    background: #205bb0;
    color: #fff;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    margin-bottom: 22px;
  }
  nav {
    display: grid;
    gap: 5px;
  }
  nav button,
  .side-bottom > button {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 11px 12px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: #46617a;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  nav button.active {
    background: #e8f1fc;
    color: #285ca4;
  }
  nav button > span {
    margin-left: auto;
    color: #46617a;
    font-size: 13px;
  }
  .side-bottom {
    margin-top: auto;
  }
  .workspace-tip {
    padding: 17px 13px 22px;
    margin-bottom: 12px;
    border-bottom: 1px solid #edf0f2;
    color: #46617a;
  }
  .workspace-tip p {
    font: 16px/1.5 var(--font-ui);
    color: #58718e;
    margin: 9px 0;
  }
  .workspace-tip small {
    display: block;
    font-size: 13px;
    line-height: 1.7;
    color: #46617a;
  }
  .side-bottom > button {
    width: 100%;
    font-size: 13px;
  }
  .workspace > main {
    margin-left: 240px;
    flex: 1;
    min-width: 0;
  }
  .top {
    height: 80px;
    background: #fff;
    border-bottom: 1px solid #e8ebef;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 42px;
    gap: 25px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 480px;
    width: 100%;
    color: #46617a;
  }
  .search input {
    background: none;
    border: 0;
    outline: none;
    font-size: 13px;
    flex: 1;
    min-width: 0;
    color: #4e6171;
  }
  .search kbd {
    font: 9px var(--font-ui);
    border: 1px solid #e6eaef;
    border-radius: 4px;
    padding: 2px 5px;
    color: #46617a;
  }
  .top > a {
    font-size: 13px;
    color: #46617a;
    display: flex;
    align-items: center;
    gap: 7px;
    text-decoration: none;
    white-space: nowrap;
  }
  .content {
    padding: 42px;
    max-width: 1500px;
    margin: auto;
  }
  .heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 30px;
  }
  .eyebrow {
    font-size: 13px;
    letter-spacing: 1.8px;
    color: #46617a;
    margin: 0 0 9px;
  }
  h1 {
    font: 550 33px/1.3 var(--font-ui);
    letter-spacing: -0.8px;
    margin: 0 0 9px;
    color: #29445f;
  }
  .heading p:not(.eyebrow) {
    font-size: 13px;
    color: #46617a;
    margin: 0;
  }
  .upload {
    display: flex;
    gap: 9px;
    align-items: center;
    background: white;
    border: 1px solid #dfe5e0;
    border-radius: 6px;
    padding: 10px 14px;
    font-size: 13px;
    color: #607263;
    cursor: pointer;
    white-space: nowrap;
  }
  .file-input {
    display: none;
  }
  .starters {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 16px;
    margin-bottom: 43px;
  }
  .starters > button {
    border: 1px solid #e5e9e6;
    border-radius: 9px;
    background: white;
    padding: 21px 17px;
    display: flex;
    align-items: center;
    gap: 12px;
    text-align: left;
    cursor: pointer;
  }
  .starters > button:hover {
    border-color: #b6cbbb;
    box-shadow: 0 5px 15px #28533005;
  }
  .starter-icon {
    width: 43px;
    height: 49px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    flex-shrink: 0;
  }
  .blue {
    background: #edf3fa;
    color: #46617a;
  }
  .green {
    background: #edf4ed !important;
    color: #719b71 !important;
  }
  .amber {
    background: #faf4e9;
    color: #46617a;
  }
  .starters b {
    font-size: 13px;
    font-weight: 500;
    color: #536957;
  }
  .starters p {
    font-size: 13px;
    color: #46617a;
    margin: 4px 0 0;
  }
  .starters > button > :global(svg) {
    margin-left: auto;
    color: #46617a;
    width: 14px;
  }
  .list-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 17px;
  }
  .list-heading h2 {
    font-size: 13px;
    font-weight: 500;
    color: #5a6b5b;
    margin: 0;
  }
  .list-heading h2 > span {
    margin-left: 7px;
    background: #e9eee8;
    color: #46617a;
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 13px;
  }
  .list-heading > div {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .list-heading > div > span {
    font-size: 13px;
    color: #46617a;
    margin-right: 14px;
  }
  .list-heading button {
    display: flex;
    background: none;
    border: 1px solid transparent;
    color: #46617a;
    padding: 6px;
    border-radius: 4px;
    cursor: pointer;
  }
  .list-heading button.chosen {
    background: white;
    border-color: #e0e6db;
    color: #46617a;
  }
  .document-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 20px;
  }
  .doc-card {
    text-decoration: none;
    color: inherit;
    border: 1px solid #e3e8df;
    border-radius: 9px;
    background: white;
    overflow: hidden;
    transition:
      box-shadow 0.2s,
      transform 0.2s;
  }
  .doc-card:hover {
    box-shadow: 0 8px 25px #2440270b;
    transform: translateY(-2px);
  }
  .doc-preview {
    background: #f1f3ef;
    height: 172px;
    padding: 24px 30px 0;
    overflow: hidden;
    position: relative;
  }
  .preview-paper {
    max-width: 220px;
    margin: auto;
    padding: 22px 20px;
    background: white;
    min-height: 170px;
    box-shadow: 0 0 0 1px #e5eae2;
    border-radius: 2px;
  }
  .preview-line {
    height: 3px;
    background: #e9ece5;
    margin: 8px 0;
    border-radius: 2px;
  }
  .preview-line.title {
    height: 5px;
    width: 70%;
    background: #c2cec0;
    margin-bottom: 15px;
  }
  .preview-line.half {
    width: 65%;
  }
  .preview-gap {
    height: 13px;
  }
  .type-badge {
    position: absolute;
    bottom: 10px;
    right: 10px;
    font-size: 13px;
    color: #46617a;
    background: #ffffffc9;
    border: 1px solid #e6eadf;
    border-radius: 4px;
    padding: 3px 6px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
  }
  .doc-details {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px 14px;
  }
  .doc-icon {
    display: grid;
    place-items: center;
    width: 29px;
    height: 33px;
    background: #eef3f9;
    color: #46617a;
    border-radius: 5px;
    flex-shrink: 0;
  }
  .doc-details > div {
    min-width: 0;
  }
  .doc-details h3 {
    font-size: 13px;
    font-weight: 500;
    color: #5b6d54;
    margin: 0 0 5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .doc-details p {
    display: flex;
    align-items: center;
    gap: 5px;
    color: #46617a;
    font-size: 13px;
    margin: 0;
  }
  .doc-details > :global(svg) {
    margin-left: auto;
    flex-shrink: 0;
    color: #46617a;
  }
  .doc-preview.sheet {
    background: #eef3ed;
  }
  .preview-grid {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
  }
  .preview-grid > span {
    height: 16px;
    border: 1px solid #e9eee6;
  }
  .preview-grid > span:nth-child(-n + 5) {
    background: #e7efdf;
  }
  .document-grid.list {
    grid-template-columns: 1fr;
    gap: 9px;
  }
  .document-grid.list .doc-preview {
    display: none;
  }
  .document-grid.list .doc-details {
    padding: 16px 18px;
  }
  .document-grid.list .doc-details p {
    font-size: 13px;
  }
  .document-grid.list .doc-details h3 {
    font-size: 13px;
  }
  .empty {
    border: 1px dashed #dce5d8;
    border-radius: 10px;
    background: #fafcf7;
    min-height: 280px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: #46617a;
    padding: 30px;
    text-align: center;
  }
  .empty > div {
    background: #edf3e7;
    width: 62px;
    height: 62px;
    border-radius: 14px;
    display: grid;
    place-items: center;
    color: #46617a;
  }
  .empty h3 {
    font: 25px var(--font-ui);
    color: #46617a;
    margin: 18px 0 10px;
  }
  .empty p {
    font-size: 13px;
    margin: 0;
  }
  .empty button {
    display: flex;
    align-items: center;
    gap: 10px;
    background: none;
    border: 0;
    color: #46617a;
    font-size: 13px;
    margin-top: 20px;
    cursor: pointer;
  }
  .workspace-footer {
    display: flex;
    justify-content: space-between;
    gap: 15px;
    padding-top: 40px;
    font-size: 13px;
    color: #46617a;
  }
  .workspace-footer > span,
  .workspace-footer > a {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .workspace-footer > a {
    color: #46617a;
    text-decoration: none;
  }
  .error {
    background: #fff2ef;
    color: #a55849;
    border: 1px solid #f0dcd5;
    padding: 12px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    margin-bottom: 16px;
  }
  .error > button {
    background: none;
    border: 0;
    color: inherit;
    display: flex;
    cursor: pointer;
    margin-left: auto;
  }
  .progress {
    color: #46617a;
    font-size: 13px;
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
    border-radius: 6px;
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
    border-radius: 7px;
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
  @media (max-width: 1100px) {
    .sidebar {
      width: 205px;
      padding: 24px 14px;
    }
    .workspace > main {
      margin-left: 205px;
    }
    .content {
      padding: 30px;
    }
    .top {
      padding: 0 30px;
    }
    .starters {
      gap: 10px;
    }
    .starters > button {
      padding: 16px 12px;
      gap: 8px;
    }
    .starter-icon {
      width: 32px;
      height: 40px;
    }
    .starter-icon > :global(svg) {
      width: 20px;
    }
    .starters b {
      font-size: 13px;
    }
    .starters p {
      font-size: 13px;
    }
    .starters > button > :global(svg) {
      display: none;
    }
    .document-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .heading h1 {
      font-size: 29px;
    }
  }
  @media (max-width: 760px) {
    .sidebar {
      width: 66px;
      padding: 22px 9px;
    }
    .brand {
      margin: 0 5px 24px;
      font-size: 35px;
    }
    .brand b,
    .team,
    .new-button,
    nav button > span,
    .workspace-tip,
    .side-bottom {
      display: none;
    }
    nav button {
      font-size: 0;
      padding: 12px 15px;
      gap: 0;
    }
    nav button > :global(svg) {
      flex-shrink: 0;
      width: 18px;
    }
    .workspace > main {
      margin-left: 66px;
    }
    .content {
      padding: 25px 20px;
    }
    .top {
      height: 66px;
      padding: 0 20px;
    }
    .top > a,
    .search kbd {
      display: none;
    }
    .search input {
      font-size: 13px;
    }
    .heading {
      display: block;
    }
    .heading h1 {
      font-size: 26px;
    }
    .heading > div > p:not(.eyebrow) {
      font-size: 13px;
    }
    .upload {
      margin-top: 18px;
      font-size: 13px;
    }
    .starters {
      grid-template-columns: 1fr;
      margin-bottom: 30px;
    }
    .starters > button {
      padding: 13px;
    }
    .starters p {
      font-size: 13px;
    }
    .starters b {
      font-size: 13px;
    }
    .starters > button > :global(svg) {
      display: block;
    }
    .document-grid {
      grid-template-columns: 1fr;
    }
    .doc-preview {
      height: 150px;
    }
    .list-heading > div > span {
      display: none;
    }
    .workspace-footer {
      flex-direction: column;
      gap: 12px;
      font-size: 13px;
    }
    .modal {
      padding: 25px;
    }
    .modal h2 {
      font-size: 25px;
    }
  }
  .sidebar {
    overflow-y: auto;
  }
  .sidebar > * {
    flex-shrink: 0;
  }
  .mobile-close,
  .mobile-nav {
    display: none;
  }
  @media (max-width: 760px) {
    .sidebar {
      display: none;
      width: 280px;
      padding: 22px;
      box-shadow: 6px 0 28px #12375b30;
    }
    .sidebar.open {
      display: flex;
    }
    .sidebar.open .brand b,
    .sidebar.open .team,
    .sidebar.open .new-button,
    .sidebar.open .side-bottom {
      display: flex;
    }
    .sidebar.open nav button {
      font-size: 14px;
      padding: 12px;
      gap: 10px;
    }
    .sidebar.open nav button > span {
      display: inline;
    }
    .mobile-nav,
    .mobile-close {
      display: block;
      padding: 9px 10px;
      border: 1px solid #b6cbe3;
      background: #e9f1fb;
      border-radius: 5px;
      color: #25548c;
      font-size: 13px;
      cursor: pointer;
    }
    .mobile-close {
      margin-bottom: 15px;
    }
    .workspace > main {
      margin-left: 0;
    }
    .top {
      padding: 12px 16px;
      height: auto;
      min-height: 76px;
      gap: 12px;
    }
    .top > a {
      display: none;
    }
    .top .search {
      min-width: 0;
      flex: 1;
    }
    .content {
      padding: 25px 18px;
    }
    .heading {
      flex-wrap: wrap;
      gap: 16px;
    }
    .starters {
      grid-template-columns: 1fr;
    }
    .starters p {
      font-size: 14px;
    }
    .starters b {
      font-size: 16px;
    }
    .document-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
