<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import {
    Archive,
    FileText,
    Table2,
    ClipboardList,
    Presentation,
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
    ArrowRight,
    LoaderCircle
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
  import { initialPresentationOps } from '$lib/workspace/presentation';
  import { ulid } from 'ulid';
  import { clearSession } from '$lib/stores/session.svelte';
  import { queuePush } from '$lib/workspace/pending-push';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import NotificationBell from '$lib/components/workspace/NotificationBell.svelte';
  import { RELATION_TEXT, percent, type SimilarDocument } from '$lib/workspace/collaboration';
  import { isCopy, isSameName, similarDocuments } from '$lib/workspace/upload-check';
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
    try {
      listView = localStorage.getItem('dynodoc.workspace-view') !== 'grid';
    } catch {
      /* The layout preference is optional. */
    }
  });
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
      // An expired session returns to sign-in instead of showing an empty workspace.
      if ((e as { status?: number }).status === 401) {
        await goto('/login?next=%2Fworkspace');
        return;
      }
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
      if (kind === 'presentation') ops = initialPresentationOps(title, root);
      await saveBatch(doc.id, ops, 0);
      await goto(`/documents/${doc.id}`);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not create document.';
    } finally {
      busy = false;
    }
  }
  // A file that resembles a document you can open is probably an edited copy: offer
  // to send it there as reviewable changes instead of creating a separate document.
  // The server compares content, so renamed files are recognised too.
  let similar = $state<SimilarDocument[]>([]),
    checking = $state(false);
  const PUSHABLE: Kind[] = ['document', 'spreadsheet', 'presentation'];
  const pushable = (m: SimilarDocument) =>
    m.can_push && m.kind === importResult?.kind && PUSHABLE.includes(m.kind as Kind);
  async function checkFile(file: File, result: ImportResult) {
    similar = [];
    checking = true;
    try {
      similar = await similarDocuments(file, result.ops);
    } catch {
      // The check only helps; importing still works, and owners are still told
      // about copies after the import.
      similar = [];
    } finally {
      checking = false;
    }
  }
  async function pushTo(id: string) {
    if (!originalFile) return;
    queuePush(id, originalFile);
    importResult = null;
    similar = [];
    await goto(`/documents/${id}`);
  }
  async function chooseFile(event: Event) {
    const file = (event.target as HTMLInputElement).files?.[0];
    if (file) await openFile(file);
  }
  // Dropping a file anywhere on Home opens the same import dialog as Upload.
  let dragging = $state(false);
  const hasFiles = (event: DragEvent) =>
    !!event.dataTransfer && Array.from(event.dataTransfer.types).includes('Files');
  function dragOver(event: DragEvent) {
    if (!hasFiles(event) || trash) return;
    event.preventDefault();
    dragging = true;
  }
  function dragLeave(event: DragEvent) {
    const next = event.relatedTarget as Node | null;
    if (!next || !(event.currentTarget as HTMLElement).contains(next)) dragging = false;
  }
  function drop(event: DragEvent) {
    if (!hasFiles(event) || trash) return;
    event.preventDefault();
    dragging = false;
    const file = event.dataTransfer?.files[0];
    if (file && !busy) void openFile(file);
  }
  async function openFile(file: File) {
    importedSource = null;
    busy = true;
    error = '';
    progress = 'Reading your file…';
    importAbort = new AbortController();
    try {
      similar = [];
      importResult = await importFile(file, importAbort?.signal);
      originalFile = file;
      title = file.name.replace(/\.[^.]+$/, '');
      void checkFile(file, importResult);
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
<div
  class="workspace"
  class:spreadsheet-home={filter === 'spreadsheet'}
  class:presentation-home={filter === 'presentation'}
>
  <header class="app-title">
    <a href="https://dynodoc.online"
      ><img src="/brand/mark.svg?v=2" alt="" width="21" height="21" />Dynodoc</a
    >
    <div>
      <NotificationBell size={17} />
      <ThemeToggle size={17} />
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
        ><Upload size={23} strokeWidth={1.3} /><span>Upload</span></button
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
    <div
      class="content"
      class:dragging
      role="region"
      aria-label={trash ? 'Trash' : 'Home'}
      ondragover={dragOver}
      ondragleave={dragLeave}
      ondrop={drop}
    >
      {#if dragging}<div class="drop-overlay" aria-hidden="true">
          <Upload size={34} strokeWidth={1.3} />
          <strong>Drop to upload</strong>
          <span>Word, Excel, PowerPoint, CSV, text or LaTeX files</span>
        </div>{/if}
      <h1>{trash ? 'Trash' : 'Home'}</h1>
      <input
        class="file-input"
        bind:this={fileInput}
        type="file"
        accept=".docx,.doc,.xlsx,.csv,.pptx,.txt,.md,.tex,.zip"
        onchange={chooseFile}
      />
      {#if !trash}<section class="new-section" aria-labelledby="new-title">
          <h2 id="new-title">New</h2>
          <div class="starters">
            <button class="upload-tile" disabled={busy} onclick={() => fileInput.click()}>
              <div class="template-paper upload-paper">
                <Upload size={30} strokeWidth={1.2} />
              </div>
              <span>Upload a file</span>
              <small>Word, Excel, PowerPoint, CSV… or drop it here</small>
            </button>
            {#each [{ kind: 'document', label: 'Blank document', icon: FileText }, { kind: 'spreadsheet', label: 'Blank workbook', icon: Table2 }, { kind: 'questionnaire', label: 'Blank questionnaire', icon: ClipboardList }, { kind: 'presentation', label: 'Blank presentation', icon: Presentation }] as item}
              <button
                class:sheet={item.kind === 'spreadsheet'}
                class:featured={item.kind === 'document'}
                disabled={busy}
                onclick={() => {
                  kind = item.kind as Kind;
                  title = '';
                  createOpen = true;
                }}
              >
                <div
                  class="template-paper"
                  class:workbook={item.kind === 'spreadsheet'}
                  class:deck={item.kind === 'presentation'}
                >
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
          {#each [{ value: 'all', label: 'Recent' }, { value: 'document', label: 'Documents' }, { value: 'spreadsheet', label: 'Spreadsheets' }, { value: 'questionnaire', label: 'Questionnaires' }, { value: 'presentation', label: 'Presentations' }] as item}<button
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
          onclick={() => {
            listView = !listView;
            try {
              localStorage.setItem('dynodoc.workspace-view', listView ? 'list' : 'grid');
            } catch {
              /* The layout preference is optional. */
            }
          }}
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
          {#if !search && !trash}<p>Upload a file, drop one here, or start from a blank page.</p>
            <div class="empty-actions">
              <button class="upload-now" onclick={() => fileInput.click()}
                ><Upload size={16} /> Upload a file</button
              ><button
                onclick={() => {
                  title = '';
                  createOpen = true;
                }}>Create a document</button
              >
            </div>{/if}
        </div>
      {:else}<div class:list={listView} class="document-grid">
          {#each visible as doc}<article class="doc-entry">
              <a class="doc-card" href={trash ? undefined : `/documents/${doc.id}`}>
                <span
                  class="doc-icon"
                  class:green={documentKind(doc) === 'spreadsheet'}
                  class:orange={documentKind(doc) === 'presentation'}
                  >{#if documentKind(doc) === 'spreadsheet'}<Table2
                      size={listView ? 26 : 38}
                      strokeWidth={1.3}
                    />{:else if documentKind(doc) === 'presentation'}<Presentation
                      size={listView ? 26 : 38}
                      strokeWidth={1.3}
                    />{:else if documentKind(doc) === 'questionnaire'}<ClipboardList
                      size={listView ? 26 : 38}
                      strokeWidth={1.3}
                    />{:else}<FileText size={listView ? 26 : 38} strokeWidth={1.3} />{/if}</span
                >
                <div class="doc-details">
                  <h3 title={doc.title}>{doc.title}</h3>
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
          similar = [];
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
              ><option value="questionnaire">Questionnaire</option><option value="presentation"
                >Presentation</option
              ></select
            ></label
          >{:else}{#if checking}<p class="checking" role="status">
              <LoaderCircle size={14} /> Checking for documents like this file…
            </p>{/if}{#if similar.length}{@const top = similar[0]}
            <div class="existing-copy" role="note">
              <strong
                >{isCopy(top)
                  ? `This file looks like ${RELATION_TEXT[top.comparison.relation].toLowerCase()} of “${top.title}”.`
                  : isSameName(top)
                    ? `A document named “${top.title}” already exists, but its content is different.`
                    : `Parts of this file already appear in “${top.title}”.`}</strong
              >
              <p>
                {pushable(top)
                  ? isSameName(top)
                    ? 'If you upload it as changes, the owner chooses, paragraph by paragraph, between their content and yours. Nothing changes until then.'
                    : 'Send your edits to that document for review instead of creating a separate copy. Nothing changes there until someone merges them.'
                  : 'You can open it, but your role there doesn’t allow sending changes. You can still import a separate copy.'}
              </p>
              <ul class="matches">
                {#each similar.slice(0, 3) as m (m.document_id)}<li>
                    <span
                      ><b>{m.title}</b><small
                        >{RELATION_TEXT[m.comparison.relation]} · {percent(m.comparison.score)} match
                        · {m.owner.is_you
                          ? 'yours'
                          : `owner ${m.owner.name || m.owner.email}`}</small
                      ></span
                    >{#if pushable(m)}<button
                        type="button"
                        disabled={busy}
                        onclick={() => void pushTo(m.document_id)}>Upload as changes</button
                      >{:else}<a href={`/documents/${m.document_id}`}>Open</a>{/if}
                  </li>{/each}
              </ul>
            </div>{/if}
          <p class="privacy-note">
            Uploads are compared with existing documents by content. If this file closely matches a
            document you can’t open, its owner is told who uploaded it and the file name — never its
            content.
          </p>
          <div class="import-report">
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
  /* Filled brand controls keep the day brand shade in both themes so on-accent text stays readable. */
  .workspace {
    --workspace-accent: var(--color-word);
    --workspace-accent-solid: #185abd;
  }
  .spreadsheet-home {
    --workspace-accent: var(--color-excel);
    --workspace-accent-solid: #107c41;
  }
  .presentation-home {
    --workspace-accent: var(--color-powerpoint);
    --workspace-accent-solid: #c43e1c;
  }

  .workspace {
    min-height: 100dvh;
    background: var(--color-parchment);
    color: var(--color-ink-strong);
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
    outline: 2px solid var(--workspace-accent);
    outline-offset: 3px;
  }
  .app-title {
    height: 46px;
    background: var(--color-surface-sunken);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 18px;
    border-bottom: 1px solid var(--color-border-soft);
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
    border-right: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    padding: 5px 4px 12px;
    background: var(--color-surface-sunken);
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
    color: var(--color-ink);
    font-size: 12px;
    min-height: 57px;
    width: 100%;
  }
  .sidebar button.active {
    border-left-color: var(--workspace-accent);
    color: var(--workspace-accent);
  }
  .sidebar button:hover {
    background: var(--color-surface-hover);
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
  .starters > button.featured {
    border-color: var(--color-ink-soft);
    background: var(--color-surface-hover);
  }
  .starters > button.upload-tile {
    gap: 8px;
    border: 1px dashed var(--color-accent-border);
    border-radius: 8px;
    background: var(--color-accent-bg);
  }
  .upload-tile small {
    max-width: 150px;
    font-size: 11px;
    line-height: 1.35;
    color: var(--color-ink-soft);
    text-align: center;
  }
  .template-paper.upload-paper {
    width: 58px;
    height: 58px;
    border-radius: 50%;
    border-color: var(--color-accent-border);
    color: var(--color-accent);
  }
  .content {
    position: relative;
  }
  .drop-overlay {
    position: absolute;
    inset: 8px;
    z-index: 5;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    border: 2px dashed var(--color-accent);
    border-radius: 16px;
    background: color-mix(in srgb, var(--color-accent-bg) 92%, transparent);
    color: var(--color-accent-strong);
    pointer-events: none;
  }
  .drop-overlay span {
    color: var(--color-ink-soft);
    font-size: 14px;
  }
  .starters > button:hover {
    background: var(--color-surface-hover);
    border-color: var(--color-muted);
  }
  .template-paper {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    width: 71px;
    height: 92px;
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border-strong);
    color: var(--workspace-accent);
  }
  .template-paper.workbook {
    --workbook-line: color-mix(in srgb, var(--color-excel) 15%, var(--color-surface-raised));
    color: var(--color-excel);
    background:
      repeating-linear-gradient(0deg, transparent 0, transparent 8px, var(--workbook-line) 9px),
      repeating-linear-gradient(
        90deg,
        var(--color-surface-raised) 0,
        var(--color-surface-raised) 14px,
        var(--workbook-line) 15px
      );
  }
  .template-paper.deck {
    color: var(--color-powerpoint);
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
    background: var(--color-border-strong);
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
    border: 1px solid var(--color-subtle);
    border-radius: 18px;
    padding: 7px 13px;
    background: var(--color-surface-raised);
    color: inherit;
  }
  .filters button.active {
    background: var(--workspace-accent-solid);
    border-color: var(--workspace-accent-solid);
    color: var(--color-on-accent);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    width: min(450px, 40%);
    margin-left: auto;
    border: 1px solid var(--color-subtle);
    background: var(--color-surface-raised);
    padding: 6px 10px;
    border-radius: 3px;
  }
  .search input {
    border: 0;
    outline: none;
    min-width: 0;
    width: 100%;
    font-size: 12px;
    background: transparent;
    color: inherit;
  }
  .search:focus-within {
    outline: 2px solid var(--workspace-accent);
  }
  kbd {
    color: var(--color-muted);
    font-size: 10px;
    white-space: nowrap;
  }
  .view-button {
    background: transparent;
    border: 0;
    color: var(--color-ink-soft);
    padding: 6px;
  }
  .column-head {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    padding: 8px 35px 8px 0;
    border-bottom: 1px solid var(--color-border-strong);
  }
  .column-head select {
    border: 0;
    background: transparent;
    color: inherit;
    width: 185px;
  }
  .document-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
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
    border: 1px solid var(--color-border-strong);
    padding: 12px;
    background: var(--color-surface-sunken);
    min-width: 0;
  }
  .list .doc-entry {
    border: 0;
    border-bottom: 1px solid var(--color-border-strong);
    background: transparent;
    padding: 0 0 0 8px;
  }
  .doc-entry:hover {
    background: var(--color-surface-hover);
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
    color: var(--workspace-accent);
    flex-shrink: 0;
    display: flex;
  }
  .doc-icon.green {
    color: var(--color-excel);
  }
  .doc-icon.orange {
    color: var(--color-powerpoint);
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
    color: var(--color-ink-soft);
    background: transparent;
  }
  .trash-action:hover {
    background: var(--color-border);
  }
  /* Grid tiles stack preview, name and details so a long name can never squeeze out. */
  .document-grid:not(.list) .doc-entry {
    flex-direction: column;
    align-items: stretch;
    padding: 0;
    background: var(--color-surface-raised);
    border-radius: 4px;
    overflow: hidden;
  }
  .document-grid:not(.list) .doc-entry:hover {
    border-color: var(--color-subtle);
    background: var(--color-surface-raised);
  }
  .document-grid:not(.list) .doc-card {
    flex-direction: column;
    align-items: stretch;
    flex-wrap: nowrap;
    gap: 0;
    padding: 0;
  }
  .document-grid:not(.list) .doc-icon {
    height: 112px;
    align-items: center;
    justify-content: center;
    background: var(--color-word-bg);
    border-bottom: 1px solid var(--color-border);
  }
  .document-grid:not(.list) .doc-icon.green {
    background: var(--color-excel-bg);
  }
  .document-grid:not(.list) .doc-icon.orange {
    background: var(--color-powerpoint-bg);
  }
  .document-grid:not(.list) .doc-details {
    padding: 11px 12px 3px;
  }
  .document-grid:not(.list) .doc-details h3 {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    white-space: normal;
    overflow-wrap: anywhere;
    line-height: 1.35;
    min-height: 2.7em;
  }
  .document-grid:not(.list) .doc-details p {
    color: var(--color-ink-soft);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .document-grid:not(.list) time {
    width: auto;
    padding: 0 12px 11px;
    font-size: 11px;
    color: var(--color-ink-soft);
  }
  .document-grid:not(.list) .trash-action {
    position: absolute;
    top: 6px;
    right: 6px;
    margin: 0;
    background: color-mix(in srgb, var(--color-surface-raised) 85%, transparent);
  }
  .workspace-tools {
    display: flex;
    gap: 35px;
    padding: 20px 48px;
    background: var(--color-surface-raised);
    border-bottom: 1px solid var(--color-border);
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
    border: 1px solid var(--color-border-strong);
    background: var(--color-surface-sunken);
  }
  .empty {
    padding: 55px 10px;
    color: var(--color-ink-soft);
    text-align: center;
  }
  .empty-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 10px;
  }
  .empty .upload-now {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--color-accent-solid);
    border-color: var(--color-accent-solid);
    color: var(--color-on-accent);
  }
  .empty h3 {
    font-size: 16px;
    font-weight: 400;
  }
  .empty button {
    border: 1px solid var(--color-accent-border);
    color: var(--workspace-accent);
    padding: 8px 12px;
    background: var(--color-surface-raised);
    border-radius: 3px;
  }
  .error {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px;
    background: var(--color-danger-bg);
    color: var(--color-danger);
  }
  .error button {
    margin-left: auto;
    border: 0;
    background: transparent;
  }
  .progress {
    color: var(--workspace-accent);
  }
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
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
    background: var(--color-surface-raised);
    border-radius: 14px;
    padding: 30px;
    position: relative;
    max-height: 90vh;
    overflow: auto;
    box-shadow: var(--shadow-dialog);
  }
  .close {
    position: absolute;
    top: 18px;
    right: 18px;
    border: 0;
    background: none;
    color: var(--color-ink-soft);
    cursor: pointer;
  }
  .modal-icon {
    background: var(--color-success-bg);
    color: var(--color-ink-soft);
    width: 46px;
    height: 46px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    margin-bottom: 20px;
  }
  .modal h2 {
    font: 29px var(--font-ui);
    color: var(--color-ink-soft);
    margin: 0 0 10px;
  }
  .modal > p {
    font-size: 13px;
    color: var(--color-ink-soft);
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
    color: var(--color-ink-soft);
  }
  .modal input,
  .modal select {
    padding: 11px;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    color: var(--color-ink-soft);
    background: var(--color-surface-raised);
    font-size: 13px;
    width: 100%;
  }
  .confirm {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    background: #285440;
    color: var(--color-on-accent);
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
  .checking,
  .privacy-note {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 0;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--color-ink-soft);
  }
  .matches {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  .matches li {
    display: flex;
    gap: 10px;
    align-items: center;
    justify-content: space-between;
  }
  .matches span {
    display: grid;
    min-width: 0;
  }
  .matches b {
    overflow-wrap: anywhere;
  }
  .matches small {
    color: var(--color-ink-soft);
  }
  .existing-copy {
    display: grid;
    gap: 6px;
    background: var(--color-accent-bg);
    border: 1px solid var(--color-accent-border);
    border-radius: 8px;
    padding: 12px 14px;
    font-size: 13px;
    color: var(--color-accent-strong);
  }
  .existing-copy p {
    margin: 0;
    line-height: 1.6;
  }
  .existing-copy button {
    justify-self: start;
    border: 0;
    border-radius: 8px;
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
    padding: 8px 12px;
    font: inherit;
    cursor: pointer;
  }
  .import-report {
    background: var(--color-surface-sunken);
    border: 1px solid var(--color-border-soft);
    padding: 14px;
    border-radius: 8px;
    font-size: 13px;
    color: var(--color-ink-soft);
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
    color: var(--color-ink-soft);
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
