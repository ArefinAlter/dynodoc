<script lang="ts">
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import {
    recoveryCopies,
    saveRecovery,
    removeRecovery,
    recoveryPreference,
    setRecoveryPreference,
    clearRecovery,
    type RecoveryCopy
  } from '$lib/workspace/recovery';
  import { describeRichTextPatch } from '$lib/workspace/richtext';
  import { documentFonts, safeFont, type PageSetup } from '$lib/workspace/office';
  import { onMount, untrack } from 'svelte';
  import { beforeNavigate, goto } from '$app/navigation';
  import {
    ArrowLeft,
    FileText,
    Table2,
    ClipboardList,
    Presentation,
    Check,
    Cloud,
    Users,
    History,
    MessageSquare,
    GitBranch,
    X,
    Plus,
    Download,
    Copy,
    AlertCircle,
    UploadCloud,
    RefreshCw,
    ShieldCheck,
    LoaderCircle,
    HelpCircle,
    GitPullRequest,
    FileUp
  } from '@lucide/svelte';
  import { ulid } from 'ulid';
  import type { DocumentStore } from '$lib/stores/documentStore.svelte';
  import type { DocumentState, EventPayload, EventRow } from '$lib/api/types';
  import { applyPayload } from '$lib/materializer';
  import {
    documentKind,
    workspaceRequest as request,
    saveBatch,
    type Member,
    type Version,
    type Draft,
    byPosition
  } from '$lib/workspace/api';
  import {
    cloneState,
    applyLocalChanges,
    richContent,
    richChanges,
    stateChanges,
    between
  } from '$lib/workspace/changes';
  import {
    download,
    exportDocument,
    exportSpreadsheet,
    exportPresentation,
    textOf,
    type TextBlock
  } from '$lib/workspace/files';
  import { createDocument, listEvents } from '$lib/api/client';
  import PublicLinkPanel from '$lib/components/product/PublicLinkPanel.svelte';
  import RichEditor from './RichEditor.svelte';
  import DiscussionThreads from './DiscussionThreads.svelte';
  import SheetEditor from './SheetEditor.svelte';
  import QuestionnaireEditor from './QuestionnaireEditor.svelte';
  import PushFileDialog from './PushFileDialog.svelte';
  import ChangeRequestReview from './ChangeRequestReview.svelte';
  import RebaseDialog from './RebaseDialog.svelte';
  import { listRequests, submitDraft, type ChangeRequest } from '$lib/workspace/reviews';
  import { takePush } from '$lib/workspace/pending-push';
  import { dismissable } from '$lib/actions/dismissable';
  import PresentationEditor from './PresentationEditor.svelte';
  import type { JSONContent } from '@tiptap/core';
  let { store }: { store: DocumentStore } = $props();
  const kind = $derived(documentKind(store.detail!));
  let canManage = $state(false);
  let identityId = $state('');
  let recoveryLoading = $state(true),
    sessionEnded = $state(false);
  let recoveryEnabled = $state(false),
    recoveryOffer = $state<RecoveryCopy | null>(null),
    recoveryMessage = $state('');
  let recoveryOwner = '',
    recoveryKey = '',
    recoveryTimer: ReturnType<typeof setTimeout>;
  let recoveryWrites = Promise.resolve();
  let composing = $state(false);
  const author = $derived(store.detail?.role === 'author');
  let local = $state<DocumentState>(untrack(() => cloneState(store.state))),
    base = $state<DocumentState>(untrack(() => cloneState(store.state))),
    baseSeq = $state(untrack(() => store.seq)),
    revision = $state(0),
    dirty = $state(false),
    saving = $state(false),
    saved = $state('All changes saved'),
    error = $state(''),
    blocked = $state(false);
  let title = $state(untrack(() => store.detail?.title || '')),
    panel = $state(''),
    menu = $state(false),
    selected = $state<string | null>(null),
    busy = $state(false),
    email = $state(''),
    role = $state('author'),
    members = $state<Member[]>([]),
    versions = $state<Version[]>([]),
    drafts = $state<Draft[]>([]),
    events = $state<EventRow[]>([]),
    uploads = $state<
      {
        id: string;
        filename: string;
        report: {
          warnings?: string[];
          research_source?: import('$lib/workspace/connectors').ResearchSource;
        };
      }[]
    >([]),
    comment = $state(''),
    name = $state(''),
    note = $state(''),
    historyQuery = $state('');
  let draftId = $state(''),
    draftName = $state(''),
    draftRevision = $state(0),
    sync = $state<SyncPlan | null>(null),
    resolutions = $state<Record<string, string>>({}),
    included = $state<string[]>([]),
    compare = $state<DocumentState | null>(null),
    compareName = $state(''),
    undoStack = $state<DocumentState[]>([]),
    redoStack = $state<DocumentState[]>([]);
  let rememberSelection = $state(true);
  // Change requests: pushed files and drafts submitted for review.
  let requests = $state<ChangeRequest[]>([]),
    reviewing = $state<string | null>(null),
    pushOpen = $state(false),
    pushFile = $state<File | null>(null),
    rebaseOpen = $state(false),
    reviewNote = $state(''),
    fileButton = $state<HTMLButtonElement | null>(null);
  const openRequests = $derived(requests.filter((r) => r.status === 'open'));
  const draftRequest = $derived(requests.find((r) => r.id === draftId));
  async function refreshRequests() {
    try {
      requests = (await listRequests(store.documentId)).items;
    } catch {
      requests = [];
    }
  }
  function menuKey(event: KeyboardEvent) {
    const items = [
      ...(event.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>(
        'button:not(:disabled)'
      )
    ];
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    const next =
      event.key === 'ArrowDown'
        ? (at + 1) % items.length
        : event.key === 'ArrowUp'
          ? (at - 1 + items.length) % items.length
          : event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? items.length - 1
              : -1;
    if (next >= 0) {
      event.preventDefault();
      items[next]?.focus();
    }
  }
  async function pushed(result: { id: string; submitted: boolean }) {
    pushOpen = false;
    pushFile = null;
    await refreshRequests();
    if (result.submitted) {
      saved = 'Changes sent for review';
      panel = 'requests';
    } else await switchDraft(result.id);
  }
  async function askReview() {
    if (!draftId) return;
    await save();
    if (dirty) return;
    await run(async () => {
      await submitDraft(store.documentId, draftId, reviewNote.trim());
      reviewNote = '';
      await refreshRequests();
      saved = 'Draft sent for review';
    });
  }
  const reviewerName = $derived(
    members.find((m) => m.id === identityId)?.name ||
      members.find((m) => m.id === identityId)?.email ||
      'Editor'
  );
  let suggestionValue = $state(''),
    suggestionField = $state(
      untrack(() => (documentKind(store.detail!) === 'spreadsheet' ? 'cell_0' : 'label'))
    );
  const suggestions = $derived(
    Object.entries(store.state.suggestions).filter(
      ([, s]) => !s.accepted && s.rejected_reason === null
    )
  );
  async function propose() {
    if (!selected) return;
    await run(async () => {
      let field = suggestionField,
        value: unknown = suggestionValue;
      if (kind === 'document') {
        field = 'content';
        const original = local.nodes[selected!]?.current_fields.content as JSONContent | undefined;
        if (!original || !['paragraph', 'heading'].includes(original.type || ''))
          throw new Error('Select a paragraph or heading to suggest replacement text.');
        value = {
          ...original,
          content: suggestionValue ? [{ type: 'text', text: suggestionValue }] : []
        };
      }
      const result = await request<{ events: EventRow[] }>(`${prefix}/batch`, {
        base_seq: store.seq,
        ops: [
          {
            type: 'SuggestionProposed',
            suggestion_id: ulid(),
            target_node_id: selected,
            detail: { type: 'FieldEdited', node_id: selected, field, value }
          }
        ]
      });
      store.receive(result.events);
      suggestionValue = '';
    });
  }
  let timer: ReturnType<typeof setTimeout> | undefined;
  interface SyncPlan {
    excluded_nodes?: string[];
    team_seq: number;
    revision: number;
    ops: EventPayload[];
    conflicts: {
      key: string;
      node_id: string;
      field: string;
      ancestor: unknown;
      team: unknown;
      draft: unknown;
    }[];
  }
  const editable = $derived(
    !recoveryLoading &&
      !sessionEnded &&
      !recoveryOffer &&
      (author || (!!draftId && store.detail?.role !== 'auditor'))
  );
  let activeTab = $state('');
  const textRoot = $derived(
    Object.values(local.nodes).find((n) => !n.deleted && n.type === 'form')
  );
  const textTabs = $derived(
    Object.values(local.nodes)
      .filter((n) => !n.deleted && n.type === 'section' && n.current_fields.kind === 'document_tab')
      .sort((a, b) => byPosition(a, b))
  );
  const textParent = $derived(textTabs.some((t) => t.id === activeTab) ? activeTab : textRoot?.id);
  const content = $derived(richContent(local, textParent));
  function changePage(settings: PageSetup) {
    if (textRoot)
      change([{ type: 'FieldEdited', node_id: textRoot.id, field: 'page_setup', value: settings }]);
  }
  async function importFont(file: File, family: string) {
    if (!author || !textRoot) throw new Error('An editor must upload shared document fonts.');
    if (
      safeFont(family) !== family ||
      !/\.(woff2?|ttf|otf)$/i.test(file.name) ||
      file.size > 2 * 1024 * 1024
    )
      throw new Error('Choose a WOFF2, WOFF, TTF or OTF font under 2 MB with a plain family name.');
    const fonts = documentFonts(textRoot.current_fields.fonts);
    if (fonts.length >= 16) throw new Error('A document supports up to 16 imported font families.');
    if (fonts.some((f) => f.family === family))
      throw new Error('That font family is already available in this document.');
    const buffer = await file.arrayBuffer();
    await new FontFace(family, buffer).load();
    const bytes = new Uint8Array(buffer);
    let binary = '';
    for (let i = 0; i < bytes.length; i += 8192)
      binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
    const uploaded = await request<{ id: string }>(`${prefix}/uploads`, {
      filename: file.name,
      content_type: file.type || 'application/octet-stream',
      content: btoa(binary),
      report: { kind: 'font', family }
    });
    change([
      {
        type: 'FieldEdited',
        node_id: textRoot.id,
        field: 'fonts',
        value: [...fonts, { family, uploadId: uploaded.id }]
      }
    ]);
  }
  function addTextTab() {
    if (!textRoot) return;
    const name = prompt('Name this document tab', 'New tab')?.trim();
    if (!name) return;
    const id = ulid();
    change([
      {
        type: 'NodeCreated',
        node_id: id,
        node_type: 'section',
        parent_id: textRoot.id,
        pos: between(textTabs.at(-1)?.pos || ''),
        fields: { label: name.slice(0, 160), kind: 'document_tab' }
      },
      {
        type: 'NodeCreated',
        node_id: ulid(),
        node_type: 'item',
        parent_id: id,
        pos: 'a',
        fields: { label: '', content: { type: 'paragraph', content: [] } }
      }
    ]);
    activeTab = id;
    revision++;
  }
  function renameTextTab() {
    if (!activeTab) return;
    const name = prompt(
      'Rename tab',
      String(local.nodes[activeTab]?.current_fields.label || '')
    )?.trim();
    if (name)
      change([
        { type: 'FieldEdited', node_id: activeTab, field: 'label', value: name.slice(0, 160) }
      ]);
  }
  function removeTextTab() {
    if (
      !activeTab ||
      !confirm('Remove this tab and its contents? You can restore it from version history.')
    )
      return;
    change([
      ...Object.values(local.nodes)
        .filter((n) => !n.deleted && n.parent_id === activeTab)
        .map((n) => ({ type: 'NodeDeleted', node_id: n.id }) as EventPayload),
      { type: 'NodeDeleted', node_id: activeTab }
    ]);
    activeTab = '';
    revision++;
  }
  const prefix = $derived(`/documents/${store.documentId}`);
  const delta = $derived(compare ? stateChanges(compare, local) : []);
  $effect(() => {
    const seq = store.seq;
    untrack(() => {
      if (!draftId && !dirty && !saving && !composing) {
        base = cloneState(store.state);
        local = cloneState(store.state);
        baseSeq = seq;
        revision++;
      }
    });
  });
  $effect(() => {
    if (
      dirty &&
      !saving &&
      !blocked &&
      !composing &&
      !recoveryOffer &&
      !recoveryLoading &&
      !sessionEnded
    ) {
      clearTimeout(timer);
      timer = setTimeout(() => void save(), 1300);
    }
    return () => clearTimeout(timer);
  });
  beforeNavigate(({ cancel }) => {
    if (dirty || composing) {
      cancel();
      error = 'Save these changes or put them in a personal draft before leaving.';
    }
  });
  onMount(() => {
    recoveryEnabled = recoveryPreference();
    recoveryOwner = crypto.randomUUID();
    const unload = (e: BeforeUnloadEvent) => {
      if (dirty || composing) {
        e.preventDefault();
        e.returnValue = '';
      }
    };
    const stopRecovery = () => {
      recoveryEnabled = false;
      recoveryOffer = null;
      sessionEnded = true;
      blocked = true;
      clearTimeout(recoveryTimer);
      clearTimeout(timer);
      error = 'You signed out in this browser. Sign in again to continue editing.';
    };
    const storageChanged = (event: StorageEvent) => {
      if (event.key === 'dynodoc.session-ended') stopRecovery();
      if (event.key === 'dynodoc.browser-recovery') {
        recoveryEnabled = recoveryPreference();
        if (!recoveryEnabled) clearTimeout(recoveryTimer);
      }
    };
    window.addEventListener('storage', storageChanged);
    window.addEventListener('dynodoc:signout', stopRecovery);
    window.addEventListener('beforeunload', unload);
    void refreshPeople();
    void refreshRequests();
    const queued = takePush(store.documentId);
    if (queued) {
      pushFile = queued;
      pushOpen = true;
    }
    void fetch('/auth/session')
      .then((r) => r.json())
      .then(async (s) => {
        identityId = s.identity_id || '';
        if (identityId && recoveryPreference())
          recoveryOffer = (await recoveryCopies(identityId, store.documentId))[0] || null;
      })
      .catch(() => {})
      .finally(() => {
        recoveryLoading = false;
      });
    void request<{ can_manage: boolean }>(`${prefix}/access`)
      .then((a) => {
        canManage = a.can_manage;
      })
      .catch(() => {});
    return () => {
      clearTimeout(timer);
      clearTimeout(recoveryTimer);
      window.removeEventListener('beforeunload', unload);
      window.removeEventListener('storage', storageChanged);
      window.removeEventListener('dynodoc:signout', stopRecovery);
    };
  });
  $effect(() => {
    const changed = local,
      unsaved = dirty,
      owner = identityId,
      enabled = recoveryEnabled;
    const currentBase = base,
      seq = baseSeq,
      draft = draftId,
      draftRev = draftRevision,
      draftLabel = draftName;
    clearTimeout(recoveryTimer);
    if (!owner || !enabled || recoveryOffer || sessionEnded) return;
    if (unsaved) {
      recoveryTimer = setTimeout(() => {
        const key = `${owner}:${store.documentId}:${draft || 'team'}:${recoveryOwner}`;
        const previousKey = recoveryKey;
        recoveryKey = key;
        const copy: RecoveryCopy = {
          version: 1,
          key,
          identityId: owner,
          documentId: store.documentId,
          draftId: draft,
          draftName: draftLabel,
          draftRevision: draftRev,
          baseSeq: seq,
          base: cloneState(currentBase),
          state: cloneState(changed),
          updatedAt: Date.now()
        };
        recoveryWrites = recoveryWrites
          .then(() => saveRecovery(copy))
          .then(async () => {
            if (previousKey && previousKey !== key) await removeRecovery(previousKey);
            recoveryMessage = 'A recovery copy is kept on this browser.';
          })
          .catch((e) => {
            recoveryMessage = message(e);
          });
      }, 700);
    } else if (recoveryKey) {
      const key = recoveryKey;
      recoveryKey = '';
      recoveryWrites = recoveryWrites.then(() => removeRecovery(key)).catch(() => {});
    }
    return () => clearTimeout(recoveryTimer);
  });
  async function toggleRecovery() {
    try {
      recoveryEnabled = !recoveryEnabled;
      setRecoveryPreference(recoveryEnabled);
      if (!recoveryEnabled) {
        clearTimeout(recoveryTimer);
        await recoveryWrites;
        await clearRecovery();
        recoveryOffer = null;
      }
      recoveryMessage = recoveryEnabled
        ? 'Browser recovery enabled for this device. Copies expire after seven days and clear on sign-out.'
        : 'Browser recovery disabled; local copies cleared.';
      menu = false;
    } catch (e) {
      error = message(e);
    }
  }
  async function recover() {
    if (!recoveryOffer || dirty) return;
    const copy = recoveryOffer;
    // Access to a saved private draft must still be granted by the server.
    if (copy.draftId) {
      try {
        await request(`${prefix}/drafts/${copy.draftId}`);
      } catch (e) {
        error = message(e);
        return;
      }
    }
    base = cloneState(copy.base);
    local = cloneState(copy.state);
    baseSeq = copy.baseSeq;
    draftId = copy.draftId;
    draftName = copy.draftName;
    draftRevision = copy.draftRevision;
    dirty = true;
    blocked = true;
    revision++;
    recoveryKey = copy.key;
    recoveryOffer = null;
    saved = 'Recovered changes — review before saving';
    error =
      'Your unsaved work is recovered. Review it, then retry saving or keep it as a personal draft. Newer team edits will be checked for conflicts.';
  }
  async function discardRecovery() {
    if (!recoveryOffer) return;
    try {
      await removeRecovery(recoveryOffer.key);
      recoveryOffer = null;
    } catch (e) {
      error = message(e);
    }
  }
  function change(ops: EventPayload[], record = true, reapply = true) {
    if (!ops.length) return;
    if (ops.length > 5000) {
      error = 'Change up to 5,000 fields at a time. Select a smaller range.';
      return;
    }
    if (record) {
      undoStack.push(local);
      if (undoStack.length > 50) undoStack.shift();
      redoStack = [];
    }
    local = applyLocalChanges(local, ops);
    dirty = true;
    saved = dirty ? 'Unsaved changes' : 'All changes saved';
    if (reapply) revision++;
  }
  function changeRich(value: JSONContent, changedIds?: Set<string>) {
    // ProseMirror owns rich-text undo; do not keep a duplicate document snapshot per keystroke.
    change(richChanges(local, value, textParent, changedIds), false, false);
  }
  function undo() {
    const prev = undoStack.pop();
    if (prev) {
      redoStack.push(cloneState(local));
      local = prev;
      dirty = stateChanges(base, local).length > 0;
      revision++;
    }
  }
  function redo() {
    const next = redoStack.pop();
    if (next) {
      undoStack.push(cloneState(local));
      local = next;
      dirty = stateChanges(base, local).length > 0;
      revision++;
    }
  }
  async function save() {
    if (saving || !dirty || composing || recoveryOffer || recoveryLoading || sessionEnded) return;
    const sent = cloneState(local),
      ops = stateChanges(base, sent);
    if (!ops.length) {
      dirty = false;
      saved = draftId ? 'Personal draft saved' : 'All changes saved';
      return;
    }
    saving = true;
    error = '';
    saved = 'Saving…';
    try {
      let canonical: DocumentState;
      if (draftId) {
        const result = await request<{ state: DocumentState; revision: number }>(
          `${prefix}/drafts/${draftId}`,
          { ops, revision: draftRevision }
        );
        canonical = result.state;
        draftRevision = result.revision;
      } else {
        const result = await saveBatch(store.documentId, ops, baseSeq);
        store.receive(result.events);
        canonical = cloneState(store.state);
        baseSeq = store.seq;
      }
      const later = stateChanges(sent, local);
      base = cloneState(canonical);
      local = cloneState(canonical);
      for (const op of later) applyPayload(local, op);
      dirty = later.length > 0;
      revision++;
      blocked = false;
      saved = dirty ? 'Unsaved changes' : draftId ? 'Personal draft saved' : 'All changes saved';
    } catch (e) {
      error = message(e);
      blocked = true;
      saved = 'Changes need attention';
    } finally {
      saving = false;
    }
  }
  const message = (e: unknown) =>
    e instanceof Error ? e.message : 'Something went wrong. Please try again.';
  async function run(fn: () => Promise<void>) {
    busy = true;
    error = '';
    try {
      await fn();
    } catch (e) {
      error = message(e);
    } finally {
      busy = false;
    }
  }
  async function refreshPeople() {
    try {
      members = (await request<{ items: Member[] }>(`${prefix}/members`)).items;
    } catch (e) {
      error = message(e);
    }
  }
  async function openPanel(next: string) {
    panel = panel === next ? '' : next;
    menu = false;
    if (!panel) return;
    await run(async () => {
      if (panel === 'share') await refreshPeople();
      if (panel === 'versions') {
        versions = (await request<{ items: Version[] }>(`${prefix}/versions`)).items;
      }
      if (panel === 'drafts')
        drafts = (await request<{ items: Draft[] }>(`${prefix}/drafts`)).items;
      if (panel === 'activity')
        events = (
          await listEvents(store.documentId, { from_seq: Math.max(1, store.seq - 499), limit: 500 })
        ).items;
      if (panel === 'files')
        uploads = (await request<{ items: typeof uploads }>(`${prefix}/uploads`)).items;
      if (panel === 'requests' || panel === 'drafts') await refreshRequests();
    });
  }
  async function saveTitle() {
    if (!author || title === store.detail?.title) return;
    await run(async () => {
      await request(`${prefix}/metadata`, { title });
      store.detail!.title = title;
    });
  }
  async function share() {
    await run(async () => {
      await request(`${prefix}/members`, { email, role });
      email = '';
      await refreshPeople();
    });
  }
  async function version() {
    await save();
    if (dirty || draftId) return;
    await run(async () => {
      await request(`${prefix}/versions`, { name, note });
      name = '';
      note = '';
      versions = (await request<{ items: Version[] }>(`${prefix}/versions`)).items;
    });
  }
  async function compareVersion(v: Version) {
    await run(async () => {
      const snapshot = await request<{ state: DocumentState }>(
        `${prefix}/snapshot/${v.snapshot_id}`
      );
      compare = snapshot.state;
      compareName = v.name;
    });
  }
  async function restore(v: Version) {
    if (dirty || draftId) {
      error = 'Save your changes and switch to the team version before restoring.';
      return;
    }
    await run(async () => {
      const result = await request<{ events: EventRow[] }>(`${prefix}/versions/${v.id}/restore`, {
        base_seq: store.seq
      });
      store.receive(result.events);
      compare = null;
    });
  }
  async function createDraft() {
    await run(async () => {
      const result = await request<{ id: string }>(`${prefix}/drafts`, {
        name: name || 'My working draft',
        base_seq: baseSeq
      });
      draftId = result.id;
      draftName = name || 'My working draft';
      draftRevision = 0;
      name = '';
      blocked = false;
      if (dirty) await save();
      drafts = (await request<{ items: Draft[] }>(`${prefix}/drafts`)).items;
    });
  }
  async function switchDraft(id: string) {
    await save();
    if (dirty) return;
    await run(async () => {
      sync = null;
      compare = null;
      undoStack = [];
      redoStack = [];
      if (!id) {
        draftId = '';
        draftName = '';
        base = cloneState(store.state);
        local = cloneState(store.state);
        baseSeq = store.seq;
        saved = 'All changes saved';
      } else {
        const result = await request<{ draft: Draft & { revision: number }; state: DocumentState }>(
          `${prefix}/drafts/${id}`
        );
        draftId = id;
        draftName = result.draft.name;
        draftRevision = result.draft.revision;
        base = cloneState(result.state);
        local = result.state;
        baseSeq = result.draft.base_seq;
        saved = 'Personal draft saved';
      }
      revision++;
    });
  }
  async function previewSync() {
    await save();
    if (dirty) return;
    await run(async () => {
      sync = await request<SyncPlan>(`${prefix}/drafts/${draftId}/sync`, { action: 'preview' });
      resolutions = {};
      included = [
        ...new Set([
          ...sync.ops.flatMap((op) => ('node_id' in op && op.node_id ? [op.node_id] : [])),
          ...sync.conflicts.map((c) => c.node_id)
        ])
      ].filter((id) => !sync?.excluded_nodes?.includes(id));
    });
  }
  async function synchronize(action: 'pull' | 'share') {
    if (!sync) return;
    await run(async () => {
      const result = await request<{
        state: DocumentState;
        events: EventRow[];
        revision: number;
        completed: boolean;
        team_seq: number;
      }>(`${prefix}/drafts/${draftId}/sync`, {
        action,
        team_seq: sync!.team_seq,
        revision: sync!.revision,
        resolutions,
        included_nodes: action === 'share' ? included : undefined,
        remember_selection: action === 'share' && rememberSelection
      });
      store.receive(result.events);
      draftRevision = result.revision;
      local = result.state;
      base = cloneState(result.state);
      baseSeq = result.team_seq;
      revision++;
      sync = null;
      if (result.completed) await switchDraft('');
      else saved = action === 'share' ? 'Selected changes shared' : 'Latest team changes added';
    });
  }
  async function postComment() {
    await run(async () => {
      const result = await request<{ events: EventRow[] }>(`${prefix}/batch`, {
        ops: [{ type: 'CommentAdded', node_id: selected, body: comment }],
        base_seq: store.seq
      });
      store.receive(result.events);
      comment = '';
    });
  }
  async function archive() {
    await save();
    if (dirty) return;
    await run(async () => {
      const archived = store.detail?.status !== 'archived';
      await request(`${prefix}/metadata`, { archived });
      await goto('/workspace');
    });
  }
  async function makeCopy() {
    await save();
    if (dirty) return;
    await run(async () => {
      const created = await createDocument({ title: `${title} — copy`, languages: ['en'] });
      await request(`/documents/${created.id}/metadata`, { settings: { kind } });
      const copiedFonts = new Map<string, string>();
      for (const font of documentFonts(textRoot?.current_fields.fonts)) {
        const response = await fetch(`/api/engine${prefix}/uploads/${font.uploadId}`);
        if (!response.ok) throw new Error('The document font could not be copied.');
        const bytes = new Uint8Array(await response.arrayBuffer());
        let binary = '';
        for (let i = 0; i < bytes.length; i += 8192)
          binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
        const uploaded = await request<{ id: string }>(`/documents/${created.id}/uploads`, {
          filename: `${font.family}.font`,
          content_type: response.headers.get('content-type') || 'application/octet-stream',
          content: btoa(binary),
          report: { kind: 'font', family: font.family }
        });
        copiedFonts.set(font.uploadId, uploaded.id);
      }
      const ids = new Map(Object.keys(local.nodes).map((id) => [id, ulid()]));
      const empty = { nodes: {}, comments: [], suggestions: {}, removed_choices: [] };
      const ops = stateChanges(empty, local).map((op) => {
        if (op.type !== 'NodeCreated') return op;
        const fields = JSON.parse(JSON.stringify(op.fields || {}));
        if (fields.fonts)
          fields.fonts = documentFonts(fields.fonts).map((f) => ({
            ...f,
            uploadId: copiedFonts.get(f.uploadId) || f.uploadId
          }));
        if (fields.sheet_layout?.merges)
          fields.sheet_layout.merges = fields.sheet_layout.merges.map(
            (m: { fromRow: string; toRow: string }) => ({
              ...m,
              fromRow: ids.get(m.fromRow),
              toRow: ids.get(m.toRow)
            })
          );
        if (fields.content)
          fields.content.attrs = { ...fields.content.attrs, blockId: ids.get(op.node_id) };
        return {
          ...op,
          node_id: ids.get(op.node_id)!,
          parent_id: op.parent_id ? ids.get(op.parent_id) : null,
          fields
        };
      });
      await saveBatch(created.id, ops, 0);
      await goto(`/documents/${created.id}`);
    });
  }
  async function exportFile(format: string) {
    menu = false;
    await run(async () => {
      if (format === 'tex' || format === 'overleaf') {
        const { exportLatex } = await import('$lib/workspace/latex');
        await exportLatex(local, title, format === 'overleaf');
      } else if (format === 'docx') {
        // Team-version downloads remember their revision so edits can be uploaded back.
        await save();
        const origin =
          !draftId && !dirty ? { documentId: store.documentId, seq: store.seq } : undefined;
        await exportDocument(origin ? store.state : local, title, origin);
      } else if (format === 'csv' || format === 'xlsx')
        await exportSpreadsheet(local, title, format);
      else if (format === 'pptx') await exportPresentation(local, title);
      else if (format === 'json')
        download(
          JSON.stringify({ format: 'dynodoc-content-v1', title, kind, state: local }, null, 2),
          title + '.dynodoc.json',
          'application/json'
        );
      else if (format === 'history') {
        const all: EventRow[] = [];
        let seq = 1;
        for (;;) {
          const page = await listEvents(store.documentId, { from_seq: seq, limit: 1000 });
          all.push(...page.items);
          if (page.items.length < 1000) break;
          seq = page.items.at(-1)!.seq + 1;
        }
        download(
          JSON.stringify({ document_id: store.documentId, events: all }, null, 2),
          title + '.history.json',
          'application/json'
        );
      }
    });
  }
  function describe(op: EventPayload) {
    if (op.type === 'RichTextPatched') return describeRichTextPatch(op.patch);
    const n =
      'node_id' in op && op.node_id ? local.nodes[op.node_id] || compare?.nodes[op.node_id] : null;
    const label = String(n?.current_fields.label || n?.var_name || 'Block').slice(0, 70);
    if (op.type === 'FieldEdited')
      return `${label} · ${op.field.startsWith('cell_') ? 'cell ' + (Number(op.field.slice(5)) + 1) : op.field}`;
    return `${op.type === 'NodeCreated' ? 'Add' : op.type === 'NodeDeleted' ? 'Remove' : op.type === 'NodeRestored' ? 'Restore' : 'Move'} ${label}`;
  }
  function readable(v: unknown): string {
    if (typeof v === 'string') return v;
    if (v && typeof v === 'object' && 'type' in v)
      return textOf(v as TextBlock) || JSON.stringify(v);
    return JSON.stringify(v) ?? 'Empty';
  }
  function person(id: string) {
    const m = members.find((m) => m.id === id);
    return m?.name || m?.email || 'Collaborator';
  }
</script>

<svelte:head><title>{title} · Dynodoc</title></svelte:head>
<div
  class="workspace-editor"
  class:sheet-theme={kind === 'spreadsheet'}
  class:slides-theme={kind === 'presentation'}
>
  <header>
    <a class="back" href="/workspace" aria-label="Back to workspace"><ArrowLeft size={18} /></a>
    <div
      class="file-icon"
      class:green={kind === 'spreadsheet'}
      class:orange={kind === 'presentation'}
    >
      {#if kind === 'spreadsheet'}<Table2
          size={26}
        />{:else if kind === 'presentation'}<Presentation
          size={26}
        />{:else if kind === 'questionnaire'}<ClipboardList size={26} />{:else}<FileText
          size={26}
        />{/if}
    </div>
    <div class="heading">
      <div class="title-row">
        <input
          aria-label="Document title"
          bind:value={title}
          disabled={!author}
          onblur={saveTitle}
          onkeydown={(e) => {
            if (e.key === 'Enter') e.currentTarget.blur();
          }}
        /><span class="cloud" title={saved}
          >{#if saving}<LoaderCircle size={14} />{:else if blocked}<AlertCircle
              size={14}
            />{:else}<Cloud size={15} />{/if}</span
        >
      </div>
      <nav>
        <div
          class="file-menu"
          use:dismissable={{
            open: menu,
            onDismiss: () => (menu = false),
            returnFocus: () => fileButton
          }}
        >
          <button
            bind:this={fileButton}
            aria-haspopup="menu"
            aria-expanded={menu}
            onclick={() => (menu = !menu)}
            onkeydown={(e) => {
              if (e.key === 'ArrowDown' && !menu) {
                e.preventDefault();
                menu = true;
                setTimeout(() =>
                  document.querySelector<HTMLButtonElement>('.dropdown button')?.focus()
                );
              }
            }}>File</button
          >{#if menu}<!-- Choosing any command closes the menu, like desktop applications. -->
            <div
              class="dropdown"
              role="menu"
              tabindex="-1"
              aria-label="File"
              onkeydown={menuKey}
              onclick={(e) => {
                if ((e.target as HTMLElement).closest('button')) menu = false;
              }}
            >
              <button onclick={() => void makeCopy()}><Copy size={14} /> Make a copy</button
              >{#if kind === 'document'}<button onclick={() => void exportFile('docx')}
                  ><Download size={14} /> Download Word (.docx)</button
                >{#if store.detail?.role !== 'auditor'}<button
                    onclick={() => {
                      pushFile = null;
                      pushOpen = true;
                    }}><FileUp size={14} /> Upload edited Word file…</button
                  >{/if}<button onclick={() => void exportFile('tex')}>Download LaTeX (.tex)</button
                ><button onclick={() => void exportFile('overleaf')}
                  >Download Overleaf project (.zip)</button
                >
              {/if}{#if kind === 'spreadsheet'}<button onclick={() => void exportFile('xlsx')}
                  ><Download size={14} /> Download Excel (.xlsx)</button
                ><button onclick={() => void exportFile('csv')}
                  ><Download size={14} /> Download first sheet (.csv)</button
                >{/if}{#if kind === 'presentation'}<button onclick={() => void exportFile('pptx')}
                  ><Download size={14} /> Download PowerPoint (.pptx)</button
                >{/if}<button onclick={() => void exportFile('json')}
                ><Download size={14} /> Download Dynodoc content</button
              ><button onclick={() => void exportFile('history')}
                ><ShieldCheck size={14} /> Download audit history</button
              ><button onclick={() => void openPanel('files')}
                >Original uploads & import report</button
              ><button onclick={() => void toggleRecovery()}
                >{recoveryEnabled
                  ? 'Turn off browser recovery'
                  : 'Keep recovery copies on this browser'}</button
              ><button
                onclick={() =>
                  kind === 'questionnaire'
                    ? window.print()
                    : window.dispatchEvent(new Event('dynodoc-print'))}>Print</button
              >{#if author}<button onclick={() => void archive()}
                  >{store.detail?.status === 'archived'
                    ? 'Restore to workspace'
                    : 'Move to archive'}</button
                >{/if}
            </div>{/if}
        </div>
        <button
          title="Personal drafts"
          aria-label="Personal drafts"
          onclick={() => void openPanel('drafts')}><GitBranch size={16} /></button
        >
        <button
          class="requests-button"
          title="Change requests"
          aria-label={openRequests.length
            ? `Change requests, ${openRequests.length} open`
            : 'Change requests'}
          onclick={() => void openPanel('requests')}
          ><GitPullRequest size={16} />{#if openRequests.length}<span class="count"
              >{openRequests.length}</span
            >{/if}</button
        >
        <button
          title="Who changed this?"
          aria-label="Who changed this?"
          onclick={() => void openPanel('activity')}><ShieldCheck size={16} /></button
        >
        <button
          title="Suggestions"
          aria-label="Suggestions"
          onclick={() => void openPanel('suggestions')}
          ><MessageSquare size={16} />{suggestions.length || ''}</button
        ><span class="save-state" aria-live="polite">{saved}</span>
      </nav>
    </div>
    <div class="actions">
      <ThemeToggle />
      <button
        class="round"
        title="Help & tutorials"
        aria-label="Help & tutorials"
        onclick={() => window.dispatchEvent(new Event('dynodoc:help'))}
        ><HelpCircle size={18} /></button
      >
      <button
        class="round"
        title="Comments"
        aria-label="Comments"
        onclick={() => void openPanel('comments')}><MessageSquare size={19} /></button
      ><button
        class="round"
        title="Version history"
        aria-label="Version history"
        onclick={() => void openPanel('versions')}><History size={20} /></button
      ><button class="share" onclick={() => void openPanel('share')}
        ><Users size={15} /> Share</button
      >
    </div>
  </header>
  {#if recoveryOffer}<div class="recovery-banner" role="status">
      <History size={18} /><span
        ><strong>Unsaved work is available.</strong> A browser copy from {new Date(
          recoveryOffer.updatedAt
        ).toLocaleString()}.</span
      ><button disabled={dirty} onclick={() => void recover()}>Review recovered work</button><button
        onclick={() => void discardRecovery()}>Discard copy</button
      >
    </div>{/if}
  {#if recoveryMessage}<div class="recovery-note" role="status">
      {recoveryMessage}<button
        aria-label="Dismiss recovery message"
        onclick={() => (recoveryMessage = '')}><X size={12} /></button
      >
    </div>{/if}
  {#if draftId}<div class="draft-banner">
      <GitBranch size={15} /><strong>{draftName}</strong><span
        >{draftRequest?.status === 'open'
          ? 'Sent for review: your team can see these changes'
          : 'Only you can see this draft'}{store.seq > baseSeq
          ? ` · the team version has ${store.seq - baseSeq} newer change${store.seq - baseSeq === 1 ? '' : 's'}`
          : ''}</span
      >{#if store.seq > baseSeq}<button
          disabled={busy || saving}
          onclick={async () => {
            await save();
            if (!dirty) rebaseOpen = true;
          }}>Rebase onto latest</button
        >{/if}<button disabled={busy || saving} onclick={() => void previewSync()}
        >Review & share changes</button
      ><button disabled={busy || saving} onclick={() => void switchDraft('')}>Team version</button>
    </div>{/if}
  {#if error}<div class="error" role="alert">
      <AlertCircle size={16} /><span>{error}</span>{#if dirty}<button
          disabled={saving}
          onclick={() => {
            blocked = false;
            void save();
          }}>Retry save</button
        ><button disabled={busy || saving || !!draftId} onclick={() => void createDraft()}
          >Keep as personal draft</button
        >{/if}<button aria-label="Dismiss message" onclick={() => (error = '')}
        ><X size={14} /></button
      >
    </div>{/if}
  <div class="body">
    <main>
      {#if kind === 'document'}<nav class="document-tabs" aria-label="Document tabs">
          <button
            class:active={!activeTab}
            onclick={() => {
              activeTab = '';
              revision++;
            }}>Main document</button
          >{#each textTabs as tab}<button
              class:active={activeTab === tab.id}
              onclick={() => {
                activeTab = tab.id;
                revision++;
              }}>{String(tab.current_fields.label)}</button
            >{/each}{#if editable}<button onclick={addTextTab} aria-label="Add tab" title="Add tab"
              ><Plus size={13} /></button
            >{#if activeTab}<button onclick={renameTextTab}>Rename tab</button><button
                onclick={removeTextTab}>Remove tab</button
              >{/if}{/if}
        </nav>
        <RichEditor
          {content}
          {editable}
          {reviewerName}
          onCompositionChange={(active) => {
            composing = active;
            if (!active && !dirty && !saving && !draftId) {
              base = cloneState(store.state);
              local = cloneState(store.state);
              baseSeq = store.seq;
              revision++;
            }
          }}
          documentId={store.documentId}
          pageSettings={textRoot?.current_fields.page_setup}
          fonts={textRoot?.current_fields.fonts}
          onPageChange={changePage}
          onFont={author ? importFont : undefined}
          onComment={author || store.detail?.role === 'reviewer'
            ? () => {
                void openPanel('comments');
              }
            : undefined}
          onChange={changeRich}
          onSelect={(id) => (selected = id)}
          {revision}
        />{:else if kind === 'spreadsheet'}<SheetEditor
          documentState={local}
          {editable}
          onEditingChange={(active) => {
            composing = active;
            if (!active && !dirty && !saving && !draftId) {
              base = cloneState(store.state);
              local = cloneState(store.state);
              baseSeq = store.seq;
              revision++;
            }
          }}
          onChange={change}
          onSelect={(id) => (selected = id)}
          onComment={author || store.detail?.role === 'reviewer'
            ? () => {
                void openPanel('comments');
              }
            : undefined}
          onUndo={undo}
          onRedo={redo}
        />{:else if kind === 'presentation'}<PresentationEditor
          documentState={local}
          {editable}
          onChange={change}
          onSelect={(id) => (selected = id)}
          onComment={author || store.detail?.role === 'reviewer'
            ? () => {
                void openPanel('comments');
              }
            : undefined}
          onUndo={undo}
          onRedo={redo}
        />{:else}<QuestionnaireEditor
          documentState={local}
          {editable}
          onChange={change}
          onSelect={(id) => (selected = id)}
        />{/if}
    </main>
    {#if panel}<aside>
        <div class="panel-title">
          <h2>
            {(
              {
                share: 'Share with your team',
                versions: 'Version history',
                drafts: 'Personal drafts',
                comments: 'Comments',
                activity: 'Who changed this?',
                files: 'Original files',
                suggestions: 'Suggested changes',
                requests: 'Change requests'
              } as Record<string, string>
            )[panel]}
          </h2>
          <button
            aria-label="Close panel"
            onclick={() => {
              panel = '';
              compare = null;
            }}><X size={18} /></button
          >
        </div>
        {#if busy}<div class="loading">Working…</div>{/if}
        {#if panel === 'share'}<p>
            Direct invitations and inherited workspace roles control access. Owners and workspace
            managers can invite people or publish a read-only copy.
          </p>
          {#if canManage}<form
              onsubmit={(e) => {
                e.preventDefault();
                void share();
              }}
            >
              <label
                >Email address<input
                  type="email"
                  placeholder="colleague@example.com"
                  bind:value={email}
                  required
                /></label
              ><label
                >Access<select aria-label="Access" bind:value={role}
                  ><option value="author">Can edit</option><option value="reviewer"
                    >Can comment & suggest</option
                  ><option value="auditor">Can view</option><option value="remove"
                    >Remove direct access</option
                  ></select
                ></label
              ><button class="primary" disabled={busy}>Add person</button>
            </form>{/if}
          <div class="people">
            {#each members as m}<div>
                <span class="avatar">{(m.name || m.email).slice(0, 1).toUpperCase()}</span>
                <div>
                  <strong>{m.name || m.email.split('@')[0]}</strong><small>{m.email}</small>
                </div>
                <em
                  >{m.role === 'author'
                    ? 'Editor'
                    : m.role === 'reviewer'
                      ? 'Reviewer'
                      : 'Viewer'}</em
                >
              </div>{/each}
          </div>
          <button
            class="secondary"
            onclick={() =>
              void run(async () => {
                await navigator.clipboard.writeText(location.href);
                saved = 'Link copied';
              })}><Copy size={14} /> Copy document link</button
          >
          <PublicLinkPanel
            documentId={store.documentId}
            prepare={async () => {
              await save();
              if (dirty || saving || draftId)
                throw new Error(
                  'Save the shared document before publishing. Personal drafts stay private.'
                );
              return baseSeq;
            }}
          />
        {:else if panel === 'versions'}<p>
            Name a milestone so everyone can find it later. Restoring adds new changes and keeps the
            complete history.
          </p>
          {#if author && !draftId}<form
              onsubmit={(e) => {
                e.preventDefault();
                void version();
              }}
            >
              <label
                >Version name<input
                  bind:value={name}
                  placeholder="Approved for release"
                  required
                  maxlength="200"
                /></label
              ><label
                >What changed?<textarea bind:value={note} rows="2" placeholder="Optional note"
                ></textarea></label
              ><button class="primary" disabled={busy || saving}>Save named version</button>
            </form>{/if}{#if compare}<div class="comparison">
              <strong>Since {compareName}</strong><span>{delta.length} changes</span
              >{#each delta.slice(0, 100) as op}<p>
                  {describe(op)}{#if op.type === 'FieldEdited'}<small
                      >{readable(op.value).slice(0, 200)}</small
                    >{/if}
                </p>{/each}
            </div>{/if}{#each versions as v}<div class="version">
              <span class="dot"></span><strong>{v.name}</strong><small
                >{new Date(v.created_at).toLocaleString()} · {v.author || 'Collaborator'}</small
              >{#if v.note}<p>{v.note}</p>{/if}
              <div>
                <button onclick={() => void compareVersion(v)}>Compare</button
                >{#if author && !draftId}<button onclick={() => void restore(v)}
                    >Restore as new changes</button
                  >{/if}
              </div>
            </div>{:else}<div class="empty">No named versions yet.</div>{/each}
        {:else if panel === 'drafts'}<p>
            Try a different direction without changing the team’s work. Compare and combine your
            changes when you are ready.
          </p>
          {#if !draftId && store.detail?.role !== 'auditor'}<form
              onsubmit={(e) => {
                e.preventDefault();
                void createDraft();
              }}
            >
              <label
                >Draft name<input
                  bind:value={name}
                  placeholder="Alternative interview guide"
                  required
                  maxlength="200"
                /></label
              ><button class="primary" disabled={busy || saving}
                ><Plus size={14} /> Create personal draft</button
              >
            </form>{/if}<button class="draft-card" onclick={() => void switchDraft('')}
            ><Cloud size={18} /><span>Team version<small>Shared with everyone listed</small></span
            >{#if !draftId}<Check size={15} />{/if}</button
          >{#each drafts.filter((d) => !d.merged_at) as d}<button
              class="draft-card"
              onclick={() => void switchDraft(d.id)}
              ><GitBranch size={17} /><span
                >{d.name}<small
                  >{new Date(d.created_at).toLocaleDateString()}{requests.some(
                    (r) => r.id === d.id && r.status === 'open'
                  )
                    ? ' · in review'
                    : ''}</small
                ></span
              >{#if draftId === d.id}<Check size={15} />{/if}</button
            >{/each}{#if draftId}<button
              class="primary"
              onclick={() => void previewSync()}
              disabled={busy || saving}>Compare with team version</button
            >{#if store.seq > baseSeq}<button
                class="secondary rebase"
                disabled={busy || saving}
                onclick={async () => {
                  await save();
                  if (!dirty) rebaseOpen = true;
                }}><GitBranch size={14} /> Rebase onto latest team version</button
              >{/if}
            <form
              onsubmit={(e) => {
                e.preventDefault();
                void askReview();
              }}
            >
              <label
                >{draftRequest?.status === 'open'
                  ? 'Update the note on your review request'
                  : 'Ask your team to review this draft'}<textarea
                  bind:value={reviewNote}
                  rows="2"
                  maxlength="4000"
                  placeholder="What did you change and why?"
                ></textarea></label
              ><button class="primary" disabled={busy || saving}
                ><GitPullRequest size={14} />
                {draftRequest?.status === 'open' ? 'Update request' : 'Send for review'}</button
              >
            </form>{/if}
        {:else if panel === 'comments'}<p>
            {selected ? 'Comments on the selected block or row.' : 'Document discussion.'}
            <button class="text-button" onclick={() => (selected = null)}
              >Show document comments</button
            >
          </p>
          {#if store.detail?.role !== 'auditor'}<form
              onsubmit={(e) => {
                e.preventDefault();
                void postComment();
              }}
            >
              <textarea
                aria-label="Comment"
                bind:value={comment}
                maxlength="10000"
                placeholder="Leave a note for your team…"
                required
                rows="3"
              ></textarea><button class="primary" disabled={busy || !comment.trim()}
                >Post comment</button
              >
            </form>{/if}
          <DiscussionThreads
            documentId={store.documentId}
            refreshSeq={store.seq}
            {selected}
            {author}
            canReply={store.detail?.role !== 'auditor'}
            onEvents={(rows) => store.receive(rows)}
          />
        {:else if panel === 'activity'}<p>
            {selected ? 'History for the selected block or row.' : 'Recent document changes.'}
            <button class="text-button" onclick={() => (selected = null)}>Show all</button>
          </p>
          <input
            class="history-search"
            aria-label="Search history"
            bind:value={historyQuery}
            placeholder="Find an edit or collaborator"
          />{#each events
            .filter((e) => (!selected || e.target_node_id === selected) && (!historyQuery || `${JSON.stringify(e.payload)} ${person(e.actor_id)}`
                    .toLowerCase()
                    .includes(historyQuery.toLowerCase())))
            .slice()
            .reverse() as e}<div class="activity">
              <strong>{person(e.actor_id)}</strong><small
                >{new Date(e.created_at).toLocaleString()}</small
              >
              <p>{describe(e.payload)}</p>
              {#if e.payload.type === 'FieldEdited'}<blockquote>
                  {readable(e.payload.value).slice(0, 250)}
                </blockquote>{/if}
            </div>{:else}<div class="empty">No changes match this selection.</div>{/each}<small
            class="footnote"
            >Showing up to the latest 500 events. Download the audit history for the full record.</small
          >
        {:else if panel === 'suggestions'}
          <p>
            Propose a change for an editor to review. Select a paragraph, question or spreadsheet
            row first.
          </p>
          {#if store.detail?.role !== 'auditor' && selected}<form
              onsubmit={(e) => {
                e.preventDefault();
                void propose();
              }}
            >
              {#if kind !== 'document'}<label
                  >Field<select bind:value={suggestionField}
                    >{#if kind === 'spreadsheet'}{#each Array.from({ length: Math.max(12, ...Object.keys(local.nodes[selected]?.current_fields || {})
                                .filter((k) => k.startsWith('cell_'))
                                .map((k) => Number(k.slice(5)) + 1)) }, (_, i) => i) as c}<option
                          value={`cell_${c}`}>Column {c + 1}</option
                        >{/each}{:else}<option value="label">Question label</option><option
                        value="hint">Hint</option
                      ><option value="relevance">Relevance</option><option value="constraint"
                        >Validation</option
                      >{/if}</select
                  ></label
                >{/if}
              <label
                >{kind === 'document' ? 'Suggested paragraph text' : 'Suggested value'}<textarea
                  rows="4"
                  bind:value={suggestionValue}
                  required
                ></textarea></label
              ><button class="primary" disabled={busy}>Propose change</button>
            </form>{/if}
          {#each suggestions as [id, suggestion]}<div class="version">
              <strong>{describe(suggestion.detail as EventPayload)}</strong>
              <p>{readable((suggestion.detail as { value: unknown }).value)}</p>
              {#if author}<div>
                  <button onclick={() => void store.accept(id)}>Accept change</button><button
                    onclick={() => void store.reject(id, 'Declined by editor')}>Decline</button
                  >
                </div>{/if}
            </div>{:else}<div class="empty">No suggestions awaiting review.</div>{/each}
        {:else if panel === 'requests'}<p>
            Like pull requests: teammates upload an edited copy or send a draft, and editors review
            and merge the changes. Nothing reaches the team version until it is merged.
          </p>
          {#if kind === 'document' && store.detail?.role !== 'auditor'}<button
              class="primary"
              onclick={() => {
                pushFile = null;
                pushOpen = true;
              }}><FileUp size={14} /> Upload edited Word file</button
            >{/if}
          {#each requests as r (r.id)}<button
              class="request-card"
              onclick={() => (reviewing = r.id)}
            >
              <span class={`status ${r.status}`}>{r.status}</span>
              <strong>{r.name}</strong>
              <small
                >{r.author.name || r.author.email.split('@')[0]} · {new Date(
                  r.submitted_at || r.created_at
                ).toLocaleDateString()}{r.source.filename ? ` · ${r.source.filename}` : ''}</small
              >
              {#if r.source.summary}<small class="summary"
                  >{r.source.summary.edited || 0} edited · {r.source.summary.added || 0} added · {r
                    .source.summary.removed || 0} removed</small
                >{/if}
            </button>{:else}<div class="empty">
              No change requests yet. Download the Word file, edit it anywhere, then upload it here.
            </div>{/each}
        {:else if panel === 'files'}<p>
            Original files are kept alongside the editable copy. Review conversion notes before
            using an imported instrument.
          </p>
          {#each uploads as file}<div class="version">
              <strong>{file.filename}</strong><a
                class="secondary"
                href={`/api/engine${prefix}/uploads/${file.id}`}
                download>Download original</a
              >{#if file.report?.research_source}<p>
                  Fetched from {file.report.research_source.provider === 'odk'
                    ? 'ODK Central'
                    : 'KoboToolbox'} on {new Date(
                    file.report.research_source.fetched_at
                  ).toLocaleString()}.<br />{file.report.research_source.origin} · {file.report
                    .research_source.form}
                </p>{/if}{#each file.report?.warnings || [] as warning}<p>{warning}</p>{/each}
            </div>{:else}<div class="empty">This document was created in Dynodoc.</div>{/each}{/if}
      </aside>{/if}
  </div>
  <footer>
    <span class:offline={store.connection !== 'connected'} class="connection-dot"
    ></span>{store.connection === 'connected'
      ? 'Connected to team'
      : store.connection === 'reconnecting'
        ? 'Reconnecting…'
        : 'Connection paused'}<span class="foot-right"
      >{draftId
        ? 'Personal draft'
        : store.detail?.role === 'auditor'
          ? 'Viewing'
          : store.detail?.role === 'reviewer'
            ? 'Reviewing'
            : 'Editing'} · {kind === 'document'
        ? 'Document'
        : kind === 'spreadsheet'
          ? 'Spreadsheet'
          : kind === 'presentation'
            ? 'Presentation'
            : 'Questionnaire'}</span
    >
  </footer>
</div>
{#if pushOpen}<PushFileDialog
    documentId={store.documentId}
    {title}
    initialFile={pushFile}
    onClose={() => {
      pushOpen = false;
      pushFile = null;
    }}
    onCreated={(result) => void pushed(result)}
  />{/if}
{#if reviewing}<ChangeRequestReview
    documentId={store.documentId}
    requestId={reviewing}
    team={store.state}
    teamSeq={store.seq}
    canMerge={author}
    onClose={() => (reviewing = null)}
    onChanged={() => void refreshRequests()}
    onOpenDraft={(id) => {
      reviewing = null;
      void switchDraft(id);
    }}
  />{/if}
{#if rebaseOpen && draftId}<RebaseDialog
    documentId={store.documentId}
    {draftId}
    draft={local}
    team={store.state}
    onClose={() => (rebaseOpen = false)}
    onDone={(result) => {
      rebaseOpen = false;
      draftRevision = result.revision;
      local = result.state;
      base = cloneState(result.state);
      baseSeq = result.team_seq;
      undoStack = [];
      redoStack = [];
      revision++;
      saved = 'Draft rebased onto the latest team version';
    }}
  />{/if}
{#if sync}<div class="modal-shade">
    <div class="sync-modal" role="dialog" aria-modal="true" aria-label="Review draft changes">
      <div class="panel-title">
        <h2>Bring your work together</h2>
        <button aria-label="Close review" onclick={() => (sync = null)}><X size={20} /></button>
      </div>
      <p>Choose which overlapping changes to keep. Include the blocks or rows you want to share.</p>
      {#each sync.conflicts as conflict}<div class="conflict">
          <strong>{conflict.field === '$node' ? 'Block changed or removed' : conflict.field}</strong
          >
          <div class="ancestor">
            <small>Before either edit</small>{readable(conflict.ancestor).slice(0, 350)}
          </div>
          <div class="choices">
            {#each ['team', 'draft'] as side}<label
                class:chosen={resolutions[conflict.key] === side}
                ><input
                  type="radio"
                  name={conflict.key}
                  value={side}
                  bind:group={resolutions[conflict.key]}
                /><strong>{side === 'team' ? 'Keep the team’s version' : 'Keep my draft'}</strong
                ><span
                  >{readable(side === 'team' ? conflict.team : conflict.draft).slice(0, 700)}</span
                ></label
              >{/each}
          </div>
        </div>{/each}{#if sync.conflicts.length === 0 && sync.ops.length === 0}<div class="empty">
          Your draft matches the team version.
        </div>{/if}
      <div class="include-list">
        {#each [...new Set( [...sync.ops.flatMap( (op) => ('node_id' in op && op.node_id ? [op.node_id] : []) ), ...sync.conflicts.map((c) => c.node_id)] )] as id}<label
            ><input type="checkbox" value={id} bind:group={included} /><span
              >{String(
                local.nodes[id]?.current_fields.label || local.nodes[id]?.var_name || 'Block / row'
              ).slice(0, 100)}</span
            ></label
          >{/each}
      </div>
      <div class="sync-actions">
        <label class="remember-selection"
          ><input type="checkbox" bind:checked={rememberSelection} /> Keep unchecked blocks private in
          future comparisons</label
        >
        <button
          class="secondary"
          disabled={busy || sync.conflicts.some((c) => !resolutions[c.key])}
          onclick={() => void synchronize('pull')}
          ><RefreshCw size={15} /> Get latest into my draft</button
        >{#if author}<button
            class="primary"
            disabled={busy || sync.conflicts.some((c) => !resolutions[c.key])}
            onclick={() => void synchronize('share')}
            ><UploadCloud size={15} /> Share included changes</button
          >{/if}
      </div>
      {#if !author}<p class="footnote">
          An editor must grant you editing access before you can share this draft into the team
          version.
        </p>{/if}{#if error}<p class="error" role="alert">{error}</p>{/if}
    </div>
  </div>{/if}

<style>
  .recovery-banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 24px;
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
    font-size: 13px;
    flex-wrap: wrap;
  }
  .recovery-banner > span {
    flex: 1;
  }
  .recovery-banner button {
    border: 1px solid var(--color-accent-border);
    border-radius: 9px;
    background: var(--color-surface-raised);
    color: var(--color-accent);
    padding: 8px 12px;
    cursor: pointer;
  }
  .recovery-note {
    display: flex;
    justify-content: center;
    gap: 10px;
    align-items: center;
    font-size: 11px;
    color: var(--color-ink-soft);
    padding: 5px 15px;
    background: var(--color-surface-sunken);
  }
  .recovery-note button {
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
    display: flex;
  }
  .document-tabs {
    display: flex;
    gap: 4px;
    padding: 8px 16px;
    overflow: auto;
    min-height: 42px;
  }
  .document-tabs button {
    white-space: nowrap;
    border: 1px solid transparent;
    border-radius: 10px;
    background: var(--color-surface-raised);
    color: var(--color-ink);
    padding: 7px 12px;
    font-size: 13px;
    cursor: pointer;
  }
  .document-tabs button.active {
    background: var(--color-surface-selected);
    color: var(--color-accent-strong);
    border-color: var(--color-accent-border);
  }

  .workspace-editor {
    --editor-accent: var(--color-word);
    --editor-tint: var(--color-word-bg);
    --editor-chrome: var(--color-chrome);
    /* Filled buttons with white text; night keeps a mid shade so white stays readable. */
    --editor-solid: var(--color-word);
    --editor-side-shadow: #21354815;
    height: 100dvh;
    display: flex;
    flex-direction: column;
    font-family: var(--font-ui);
    background: var(--editor-chrome);
    color: var(--color-ink);
  }
  .workspace-editor.sheet-theme {
    --editor-accent: var(--color-excel);
    --editor-tint: var(--color-excel-bg);
    --editor-chrome: color-mix(in srgb, var(--color-excel-bg) 30%, var(--color-chrome));
    --editor-solid: var(--color-excel);
  }
  .workspace-editor.slides-theme {
    --editor-accent: var(--color-powerpoint);
    --editor-tint: var(--color-powerpoint-bg);
    --editor-chrome: color-mix(in srgb, var(--color-powerpoint-bg) 30%, var(--color-chrome));
    --editor-solid: var(--color-powerpoint);
  }
  @media screen {
    :global([data-theme='dark']) .workspace-editor {
      --editor-solid: var(--color-accent-solid);
      --editor-side-shadow: #00000066;
    }
    :global([data-theme='dark']) .workspace-editor.sheet-theme {
      --editor-solid: #178548;
    }
    :global([data-theme='dark']) .workspace-editor.slides-theme {
      --editor-solid: #c43e1c;
    }
  }
  header {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 16px 24px 14px;
    background: var(--editor-chrome);
    min-height: 82px;
  }
  .back {
    color: var(--color-ink-soft);
    display: grid;
    place-items: center;
    padding: 8px;
    border-radius: 10px;
  }
  .back:hover {
    background: var(--color-surface-hover);
  }
  .file-icon {
    color: var(--color-word);
    display: grid;
    place-items: center;
  }
  .file-icon.green {
    color: var(--color-excel);
  }
  .file-icon.orange {
    color: var(--color-powerpoint);
  }
  .heading {
    min-width: 0;
    flex: 1;
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .title-row input {
    color: var(--color-ink);
    font-size: 18px;
    font-weight: 400;
    background: none;
    border: 1px solid transparent;
    padding: 0 3px;
    min-width: 60px;
    width: clamp(180px, 35vw, 580px);
    text-overflow: ellipsis;
    border-radius: 3px;
  }
  .title-row input:hover {
    border-color: var(--color-border);
  }
  .title-row input:focus {
    outline: 1px solid var(--editor-accent);
  }
  .title-row input:disabled {
    color: var(--color-ink);
  }
  .cloud {
    display: grid;
    place-items: center;
    color: var(--color-ink-soft);
  }
  nav {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-top: 4px;
  }
  nav button {
    background: none;
    border: 0;
    border-radius: 4px;
    padding: 3px 7px;
    color: var(--color-ink-soft);
    font-size: 13px;
    white-space: nowrap;
    cursor: pointer;
  }
  nav button:hover {
    background: var(--color-surface-hover);
  }
  .save-state {
    color: var(--color-ink-soft);
    font-size: 13px;
    margin-left: 15px;
  }
  .actions {
    display: flex;
    gap: 15px;
    align-items: center;
  }
  .round {
    display: grid;
    place-items: center;
    background: none;
    border: 0;
    color: var(--color-ink-soft);
    width: 32px;
    height: 32px;
    cursor: pointer;
  }
  .share {
    display: flex;
    align-items: center;
    gap: 9px;
    border: 0;
    background: var(--editor-tint);
    color: var(--editor-accent);
    border-radius: 10px;
    font-size: 13px;
    padding: 11px 21px;
    cursor: pointer;
  }
  .file-menu {
    position: relative;
  }
  .dropdown {
    position: absolute;
    top: 28px;
    left: 0;
    width: 260px;
    padding: 7px;
    background: var(--color-surface-raised);
    box-shadow: var(--shadow-menu);
    border: 1px solid var(--color-border);
    border-radius: 7px;
    z-index: 40;
  }
  .dropdown button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    text-align: left;
    font-size: 13px;
    padding: 10px;
  }
  .dropdown button:focus-visible {
    outline: 2px solid var(--editor-accent);
    outline-offset: -2px;
  }
  .requests-button {
    position: relative;
  }
  .requests-button .count {
    position: absolute;
    top: -3px;
    right: -4px;
    min-width: 15px;
    height: 15px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--editor-solid);
    color: var(--color-on-accent);
    font-size: 10px;
    line-height: 15px;
  }
  .secondary.rebase {
    margin-top: 10px;
    width: 100%;
  }
  .request-card {
    display: grid;
    gap: 4px;
    width: 100%;
    text-align: left;
    background: none;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 12px 14px;
    margin: 12px 0 0;
    cursor: pointer;
    color: var(--color-ink);
    font: inherit;
  }
  .request-card:hover {
    border-color: var(--color-accent-border);
  }
  .request-card strong {
    font-size: 13px;
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .request-card small {
    font-size: 12px;
    color: var(--color-ink-soft);
    overflow-wrap: anywhere;
  }
  .request-card .status {
    justify-self: start;
    font-size: 11px;
    text-transform: capitalize;
    padding: 1px 8px;
    border-radius: 10px;
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
  }
  .request-card .status.merged {
    background: var(--color-purple-bg);
    color: var(--color-purple);
  }
  .request-card .status.declined,
  .request-card .status.withdrawn {
    background: var(--color-surface-sunken);
    color: var(--color-ink-soft);
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  aside {
    background: var(--color-surface-raised);
    border-left: 1px solid var(--color-border);
    width: 330px;
    min-width: 330px;
    overflow: auto;
    padding: 22px 22px 35px;
  }
  .panel-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .panel-title h2 {
    font-size: 15px;
    font-weight: 500;
    margin: 0;
    color: var(--color-ink);
  }
  .panel-title button {
    border: 0;
    background: none;
    color: var(--color-ink-soft);
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  aside p,
  .sync-modal > p {
    font-size: 13px;
    line-height: 1.8;
    color: var(--color-ink-soft);
    margin: 19px 0;
  }
  form {
    display: grid;
    gap: 13px;
    margin: 22px 0;
  }
  form label {
    display: grid;
    gap: 7px;
    color: var(--color-ink-soft);
    font-size: 13px;
  }
  form input,
  form select,
  form textarea,
  .history-search {
    width: 100%;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 9px 11px;
    font: 12px Arial;
    color: var(--color-ink-soft);
    background: var(--color-surface-raised);
  }
  .primary,
  .secondary {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    border: 0;
    border-radius: 10px;
    background: var(--editor-solid);
    color: var(--color-on-accent);
    padding: 10px 14px;
    font: 12px Arial;
    cursor: pointer;
    text-decoration: none;
  }
  .primary:disabled,
  .secondary:disabled {
    opacity: 0.4;
    cursor: wait;
  }
  .secondary {
    background: var(--color-success-bg);
    color: var(--color-success);
    border: 1px solid var(--color-success-border);
  }
  .people {
    display: grid;
    gap: 19px;
    margin: 28px 0;
  }
  .people > div {
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .avatar {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: var(--color-surface-muted);
    color: var(--color-ink-soft);
    font-size: 13px;
  }
  .people strong {
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-muted);
    max-width: 145px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .people small {
    display: block;
    font-size: 13px;
    color: var(--color-ink-soft);
    max-width: 145px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .people em {
    font-style: normal;
    margin-left: auto;
    color: var(--color-ink-soft);
    font-size: 13px;
  }
  .version {
    padding: 19px 0;
    border-bottom: 1px solid var(--color-border-soft);
  }
  .version strong,
  .activity strong {
    display: block;
    color: var(--color-muted);
    font-size: 13px;
    font-weight: 500;
  }
  .version small,
  .activity small {
    display: block;
    font-size: 13px;
    color: var(--color-ink-soft);
    margin-top: 6px;
  }
  .version p {
    font-size: 13px;
    margin: 10px 0;
  }
  .version > div {
    display: flex;
    gap: 10px;
    margin-top: 12px;
  }
  .version button,
  .text-button {
    background: none;
    border: 0;
    padding: 0;
    color: var(--color-ink-soft);
    font-size: 13px;
    text-decoration: underline;
    text-underline-offset: 3px;
    cursor: pointer;
  }
  .version .secondary {
    margin-top: 15px;
  }
  .empty {
    padding: 36px 10px;
    color: var(--color-ink-soft);
    font-size: 13px;
    text-align: center;
  }
  .draft-card {
    display: flex;
    gap: 12px;
    align-items: center;
    background: none;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    width: 100%;
    padding: 16px;
    margin: 12px 0;
    text-align: left;
    cursor: pointer;
    color: var(--color-ink-soft);
  }
  .draft-card span {
    flex: 1;
    font-size: 13px;
  }
  .draft-card small {
    display: block;
    color: var(--color-ink-soft);
    font-size: 13px;
    margin-top: 5px;
  }
  .draft-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--color-success-bg);
    color: var(--color-success);
    padding: 9px 30px;
    font-size: 13px;
    border-block: 1px solid var(--color-success-border);
    margin-bottom: 7px;
  }
  .draft-banner span {
    color: var(--color-ink-soft);
  }
  .draft-banner button {
    background: var(--color-success-border);
    border: 0;
    border-radius: 4px;
    color: var(--color-ink);
    padding: 5px 11px;
    font-size: 13px;
    cursor: pointer;
  }
  .draft-banner button:first-of-type {
    margin-left: auto;
  }
  .error {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 11px 24px;
    background: var(--color-warn-bg);
    color: var(--color-ink-soft);
    font-size: 13px;
  }
  .error > span {
    flex: 1;
  }
  .error button {
    background: none;
    border: 1px solid var(--color-warn-border);
    border-radius: 4px;
    color: inherit;
    padding: 4px 9px;
    font-size: 13px;
    cursor: pointer;
  }
  .activity {
    padding: 18px 0;
    border-bottom: 1px solid var(--color-border-soft);
  }
  .activity p {
    color: var(--color-ink-soft);
    font-size: 13px;
    margin: 10px 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .activity blockquote {
    padding: 8px 12px;
    margin: 10px 0;
    border-left: 2px solid var(--color-border);
    color: var(--color-ink-soft);
    font-size: 13px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .footnote {
    display: block;
    font-size: 13px;
    line-height: 1.8;
    color: var(--color-ink-soft);
    margin-top: 24px;
  }
  .comparison {
    background: var(--color-surface-sunken);
    padding: 15px;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    margin: 20px 0;
  }
  .comparison strong {
    font-size: 13px;
    color: var(--color-ink-soft);
  }
  .comparison > span {
    display: block;
    font-size: 13px;
    color: var(--color-ink-soft);
    margin-top: 4px;
  }
  .comparison p {
    font-size: 13px;
    border-top: 1px solid var(--color-border);
    padding-top: 10px;
    margin: 10px 0;
  }
  .comparison small {
    display: block;
    font-size: 13px;
    color: var(--color-ink-soft);
    overflow-wrap: anywhere;
  }
  .loading {
    font-size: 13px;
    color: var(--color-ink-soft);
    padding-top: 12px;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 22px;
    background: var(--editor-chrome);
    border-top: 1px solid var(--color-border-soft);
    font-size: 13px;
    color: var(--color-ink-soft);
    min-height: 26px;
  }
  .connection-dot {
    width: 5px;
    height: 5px;
    background: var(--color-success);
    border-radius: 50%;
  }
  .connection-dot.offline {
    background: var(--color-warn);
  }
  .foot-right {
    margin-left: auto;
  }
  .modal-shade {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
    backdrop-filter: blur(4px);
    z-index: 100;
    display: grid;
    place-items: center;
    padding: 30px;
  }
  .sync-modal {
    background: var(--color-surface-raised);
    width: 780px;
    max-width: 100%;
    max-height: 90vh;
    overflow: auto;
    padding: 30px;
    border-radius: 12px;
    box-shadow: var(--shadow-dialog);
    font-family: var(--font-ui);
  }
  .sync-modal .panel-title h2 {
    font-size: 21px;
  }
  .conflict {
    margin: 25px 0;
    border: 1px solid var(--color-border);
    padding: 17px;
    border-radius: 7px;
  }
  .conflict > strong {
    font-size: 13px;
    color: var(--color-ink-soft);
  }
  .ancestor {
    background: var(--color-surface-sunken);
    margin: 12px 0;
    padding: 10px;
    color: var(--color-ink-soft);
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .ancestor small {
    display: block;
    color: var(--color-ink-soft);
    font-size: 13px;
    margin-bottom: 4px;
  }
  .choices {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .choices label {
    position: relative;
    display: flex;
    gap: 7px;
    flex-wrap: wrap;
    padding: 14px;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    cursor: pointer;
  }
  .choices label.chosen {
    border-color: var(--color-success);
    background: var(--color-success-bg);
  }
  .choices strong {
    font-size: 13px;
    color: var(--color-ink-soft);
  }
  .choices span {
    display: block;
    width: 100%;
    font-size: 13px;
    color: var(--color-ink-soft);
    line-height: 1.8;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .include-list {
    display: grid;
    gap: 8px;
    max-height: 250px;
    overflow: auto;
    margin: 24px 0;
  }
  .include-list label {
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 13px;
    color: var(--color-ink-soft);
    padding: 7px;
    background: var(--color-surface-sunken);
  }
  .sync-actions {
    display: flex;
    align-items: center;
    justify-content: end;
    gap: 12px;
    margin-top: 25px;
  }
  @media (max-width: 1000px) {
    .save-state {
      display: none;
    }
    nav button {
      font-size: 13px;
    }
    .actions {
      gap: 5px;
    }
    header {
      gap: 8px;
      padding: 12px;
    }
    .title-row input {
      font-size: 16px;
    }
    aside {
      width: 290px;
      min-width: 290px;
      padding: 18px;
    }
    .file-icon {
      display: none;
    }
  }
  @media (max-width: 700px) {
    nav button {
      font-size: 13px;
      padding: 3px;
    }
    .actions .round {
      display: none;
    }
    .share {
      padding: 9px 13px;
    }
    .title-row input {
      width: 180px;
    }
    .cloud {
      display: none;
    }
    .back {
      padding: 3px;
    }
    .heading {
      overflow: visible;
    }
    aside {
      position: fixed;
      right: 0;
      top: 82px;
      bottom: 26px;
      width: 300px;
      z-index: 45;
      box-shadow: -5px 0 25px var(--editor-side-shadow);
    }
    .draft-banner {
      padding: 8px 15px;
      flex-wrap: wrap;
    }
    .draft-banner span {
      display: none;
    }
    .sync-modal {
      padding: 22px;
    }
    .modal-shade {
      padding: 12px;
    }
    .choices {
      grid-template-columns: 1fr;
    }
    .sync-actions {
      flex-wrap: wrap;
    }
    .error {
      flex-wrap: wrap;
      padding: 9px 15px;
    }
  }
  @media print {
    header,
    footer,
    aside,
    .draft-banner,
    .error {
      display: none;
    }
    .workspace-editor {
      height: auto;
    }
    .body,
    main {
      display: block;
    }
  }
  header {
    padding: 5px 12px;
    min-height: 44px;
    height: 44px;
    gap: 10px;
    background: var(--color-chrome);
  }
  .back {
    padding: 5px;
    border-radius: 3px;
  }
  .file-icon :global(svg) {
    width: 20px;
    height: 20px;
  }
  .heading {
    display: flex;
    align-items: center;
    gap: 15px;
  }
  .title-row {
    gap: 6px;
  }
  .title-row input {
    width: clamp(140px, 24vw, 370px);
    font-size: 13px;
  }
  .heading nav {
    margin: 0;
    gap: 7px;
  }
  .heading nav > button {
    display: flex;
    align-items: center;
    min-height: 28px;
  }
  .save-state {
    font-size: 11px;
    margin-left: 6px;
    white-space: nowrap;
  }
  .actions {
    gap: 8px;
  }
  .round {
    width: 28px;
    height: 28px;
  }
  .share {
    padding: 5px 12px;
    border-radius: 3px;
    background: var(--editor-solid);
    color: var(--color-on-accent);
    gap: 6px;
  }
  .document-tabs {
    order: 2;
    background: var(--color-chrome);
    min-height: 24px;
    padding: 0 12px;
    margin: 0;
  }
  .document-tabs button {
    font-size: 11px;
    padding: 3px 9px;
  }
  @media (max-width: 850px) {
    .heading {
      gap: 4px;
    }
    .save-state {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
    }
    .title-row input {
      width: clamp(100px, 25vw, 220px);
    }
    .heading nav {
      gap: 0;
    }
  }
</style>
