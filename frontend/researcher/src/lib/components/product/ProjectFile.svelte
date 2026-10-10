<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import {
    FileText,
    GitBranch,
    GitPullRequest,
    History,
    ArrowDownToLine,
    Plug,
    PanelsTopLeft,
    Plus
  } from '@lucide/svelte';
  import RepositoryShell from '$lib/components/product/RepositoryShell.svelte';
  import FilePreview from '$lib/components/product/FilePreview.svelte';
  import ProvenancePanel from '$lib/components/product/ProvenancePanel.svelte';
  import ChangeRequestReview from '$lib/components/workspace/ChangeRequestReview.svelte';
  import HistoryRecovery from '$lib/components/workspace/HistoryRecovery.svelte';
  import PushFileDialog from '$lib/components/workspace/PushFileDialog.svelte';
  import { workspaceRequest, documentKind, type Draft } from '$lib/workspace/api';
  import { people, type People } from '$lib/workspace/collaboration';
  import {
    listRequests,
    submitDraft,
    type ChangeRequest,
    type PushKind
  } from '$lib/workspace/reviews';
  import { exportDocument, exportSpreadsheet, exportPresentation } from '$lib/workspace/files';
  import type { DocumentDetail, DocumentState, EventPayload, EventRow } from '$lib/api/types';
  type FileDraft = Draft & {
    submitted_at: string | null;
    review_outcome: string | null;
    revision: number;
  };
  let doc = $state<DocumentDetail | null>(null),
    access = $state<People | null>(null),
    project = $state<{ id: string; name: string } | null>(null);
  let drafts = $state<FileDraft[]>([]),
    requests = $state<ChangeRequest[]>([]),
    events = $state<EventRow[]>([]);
  let branch = $state(''),
    tab = $state('file'),
    draftName = $state(''),
    note = $state('');
  let selected = $state<{ state: DocumentState; ops: EventPayload[]; draft: FileDraft } | null>(
    null
  );
  let busy = $state(false),
    error = $state(''),
    notice = $state(''),
    creating = $state(false),
    push = $state(false),
    reviewId = $state('');
  let historyEnd = $state(0),
    recoveryAt = $state(0),
    recoveryOpen = $state(false);
  const id = $derived($page.params.fileId || ''),
    projectId = $derived($page.params.id || '');
  const base = $derived(`/documents/${id}`),
    route = $derived(`/projects/${projectId}/files/${id}`);
  const kind = $derived(doc ? documentKind(doc) : 'document');
  const previewState = $derived(branch ? selected?.state : doc?.state);
  const activeDrafts = $derived(drafts.filter((d) => !d.merged_at && !d.review_outcome));
  const tabs = [
    ['file', 'File', FileText],
    ['requests', 'Pull requests', GitPullRequest],
    ['history', 'History', History],
    ['sync', 'Sync & connections', Plug]
  ] as const;
  async function run(action: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = '';
    try {
      await action();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not complete the request';
    } finally {
      busy = false;
    }
  }
  async function load() {
    const result = await workspaceRequest<{ project: { id: string; name: string } | null }>(
      `${base}/access`
    );
    if (result.project?.id !== projectId)
      throw new Error('This file is not available in this project.');
    project = result.project;
    const [file, membership, versions, reviews] = await Promise.all([
      workspaceRequest<DocumentDetail>(base),
      people(id),
      workspaceRequest<{ items: FileDraft[] }>(`${base}/drafts`),
      listRequests(id)
    ]);
    doc = file;
    access = membership;
    drafts = versions.items;
    requests = reviews.items;
    if (branch && !activeDrafts.some((d) => d.id === branch)) {
      branch = '';
      selected = null;
      await goto(`${route}?tab=${tab}`, { replaceState: true, noScroll: true });
    }
    if (branch) await readDraft(branch);
    if (tab === 'history') await history(doc.latest_seq);
  }
  async function readDraft(value: string) {
    selected = null;
    if (value) selected = await workspaceRequest(`${base}/drafts/${value}`);
  }
  async function changeBranch(value: string) {
    branch = value;
    await readDraft(value);
    await goto(`${route}?tab=${tab}${value ? `&draft=${value}` : ''}`, {
      replaceState: true,
      noScroll: true
    });
  }
  async function selectTab(value: string) {
    tab = value;
    await goto(`${route}?tab=${value}${branch ? `&draft=${branch}` : ''}`, {
      replaceState: true,
      noScroll: true
    });
    if (value === 'history' && doc) await history(doc.latest_seq);
  }
  async function history(end: number) {
    historyEnd = end;
    if (end < 1) {
      events = [];
      return;
    }
    const from = Math.max(1, end - 49);
    const result = await workspaceRequest<{ items: EventRow[] }>(
      `${base}/events?from_seq=${from}&to_seq=${end}&limit=50`
    );
    events = [...result.items].reverse();
  }
  async function createDraft() {
    const result = await workspaceRequest<{ id: string }>(`${base}/drafts`, { name: draftName });
    branch = result.id;
    creating = false;
    draftName = '';
    await load();
    await changeBranch(branch);
    notice = 'Private file draft created';
  }
  async function downloadFile() {
    if (!previewState || !doc) return;
    const title = branch ? `${doc.title}-${selected?.draft.name || 'draft'}` : doc.title;
    const origin = branch ? undefined : { documentId: id, seq: doc.latest_seq };
    if (kind === 'document') await exportDocument(previewState, title, origin);
    else if (kind === 'spreadsheet')
      await exportSpreadsheet(previewState, title, 'xlsx', undefined, origin);
    else if (kind === 'presentation') await exportPresentation(previewState, title, origin);
  }
  onMount(() => {
    const requested = $page.url.searchParams.get('tab');
    if (tabs.some(([key]) => key === requested)) tab = requested!;
    const draft = $page.url.searchParams.get('draft');
    if (draft && /^[0-9a-f-]{36}$/i.test(draft)) branch = draft;
    void run(load);
  });
</script>

<svelte:head><title>{doc?.title || 'File'} · Dynodoc</title></svelte:head>
<RepositoryShell>
  <div class="repo-title">
    <FileText size={23} /><a href="/projects">Projects</a><span>/</span><a
      href={`/projects/${projectId}`}>{project?.name || 'Project'}</a
    ><span>/</span>
    <h1>{doc?.title || 'Loading file…'}</h1>
  </div>
  {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}{#if notice}<p
      class="repo-notice"
      role="status"
    >
      {notice}
    </p>{/if}
  {#if doc && access}
    <nav class="repo-tabs" aria-label="File sections">
      {#each tabs as [key, label, Icon]}<button
          aria-current={tab === key ? 'page' : undefined}
          disabled={busy}
          onclick={() => run(() => selectTab(key))}><Icon size={16} />{label}</button
        >{/each}
      <a href={`/documents/${id}${branch ? `?draft=${branch}` : ''}`}
        ><PanelsTopLeft size={16} /> Web editor</a
      >
    </nav>
    <div class="repo-toolbar">
      <GitBranch size={17} /><label
        ><span class="repo-small">File branch</span><select
          aria-label="File branch"
          value={branch}
          disabled={busy}
          onchange={(e) => {
            const value = e.currentTarget.value;
            void run(() => changeBranch(value));
          }}
          ><option value="">main</option>{#each activeDrafts as draft}<option value={draft.id}
              >{draft.name}{draft.submitted_at ? ' · in review' : ' · private draft'}</option
            >{/each}</select
        ></label
      >
      {#if access.you.can.propose}<button disabled={busy} onclick={() => (creating = !creating)}
          ><Plus size={15} /> New file draft</button
        >{/if}
      <span class="repo-muted"
        >{branch
          ? `Based on main r${selected?.draft.base_seq ?? '…'}`
          : `main · revision ${doc.latest_seq}`}</span
      >
      {#if kind !== 'questionnaire'}<button
          disabled={busy || !previewState}
          onclick={() => run(downloadFile)}><ArrowDownToLine size={15} /> Download file</button
        >{/if}
    </div>
    {#if creating}<form
        class="repo-panel repo-panel-body repo-form"
        onsubmit={(e) => {
          e.preventDefault();
          void run(createDraft);
        }}
      >
        <label
          >Draft name<input
            bind:value={draftName}
            required
            maxlength="200"
            placeholder="e.g. Revise introduction"
          /></label
        >
        <p class="repo-muted">
          Starts from this file's current main version. Visible only to you until submitted for
          review.
        </p>
        <button class="repo-primary" disabled={busy}>Create draft</button>
      </form>{/if}
    {#if branch && selected}<div class="repo-panel repo-panel-body">
        <div class="repo-actions">
          <GitBranch size={17} /><strong>{selected.draft.name}</strong><span class="repo-badge"
            >{selected.draft.submitted_at ? 'In review' : 'Private file draft'}</span
          ><button disabled={busy} onclick={() => (reviewId = branch)}>Compare with main</button><a
            class="repo-button"
            href={`/documents/${id}?draft=${branch}`}>Edit draft</a
          >
        </div>
        {#if !selected.draft.submitted_at && access.you.can.propose}<form
            class="repo-form"
            onsubmit={(e) => {
              e.preventDefault();
              void run(async () => {
                await submitDraft(id, branch, note);
                notice = 'Pull request opened';
                await load();
                await selectTab('requests');
              });
            }}
          >
            <label
              >Pull request message<textarea
                bind:value={note}
                maxlength="4000"
                placeholder="What changed, and why?"
              ></textarea></label
            ><button class="repo-primary" disabled={busy || !selected.ops.length}
              >Open pull request</button
            >
          </form>{/if}
      </div>{/if}
    {#if tab === 'file'}<section class="repo-panel">
        <div class="repo-panel-head">
          <FileText size={16} /><strong>{doc.title}</strong><span class="repo-muted"
            >Read-only preview · {kind}</span
          >
        </div>
        {#if previewState}{#key `${branch}:${doc.latest_seq}:${selected?.draft.revision || 0}`}<FilePreview
              documentState={previewState}
              {kind}
            />{/key}{:else}<p class="repo-empty">Loading file draft…</p>{/if}
      </section>
    {:else if tab === 'sync'}<section class="repo-panel repo-panel-body">
        <ProvenancePanel
          documentId={id}
          title={doc.title}
          canPropose={access.you.can.propose}
          onChanged={() => {
            void run(load);
          }}
        />{#if kind !== 'questionnaire' && access.you.can.propose}<button
            class="repo-primary"
            onclick={() => (push = true)}>Push an edited file</button
          >{/if}
      </section>
    {:else if tab === 'requests'}<div class="repo-title">
        <h2>Pull requests</h2>
        {#if kind !== 'questionnaire' && access.you.can.propose}<button
            class="repo-primary"
            onclick={() => (push = true)}>Push an edited file</button
          >{/if}
      </div>
      <div class="repo-panel">
        {#if !requests.length}<div class="repo-empty">
            <GitPullRequest size={28} />
            <h2>No pull requests yet</h2>
            <p>Push an edited file or submit a private draft to start a review.</p>
          </div>{/if}{#each requests as r}<article class="repo-row">
            <div class="repo-actions">
              <GitPullRequest size={17} /><button onclick={() => (reviewId = r.id)}>{r.name}</button
              ><span class="repo-badge">{r.status}</span>
            </div>
            <p class="repo-muted">
              {r.author.name || 'Project member'} · Based on revision {r.base_seq}
            </p>
          </article>{/each}
      </div>
    {:else if tab === 'history'}
      <div class="repo-title">
        <h2>Main history</h2>
        <button
          onclick={() => {
            recoveryAt = doc!.latest_seq;
            recoveryOpen = !recoveryOpen;
          }}>Recover an earlier revision</button
        >
      </div>
      <p class="repo-muted">
        Every recorded change stays in the file's history. Download an earlier state, restore
        selected blocks, revert or reapply changes.
      </p>
      {#if recoveryOpen}{#key recoveryAt}<HistoryRecovery
            documentId={id}
            head={doc.latest_seq}
            initialRevision={recoveryAt}
            canCreate={access.you.can.propose}
            onCreated={(draft) => {
              void run(async () => {
                branch = draft;
                await load();
                tab = 'file';
                await changeBranch(draft);
                notice = 'Recovery saved in a private file draft';
              });
            }}
          />{/key}{/if}
      <div class="repo-panel">
        {#if !events.length}<p class="repo-empty">
            No recorded changes yet.
          </p>{/if}{#each events as event}<article class="repo-row">
            <div class="repo-actions">
              <History size={15} /><strong>r{event.seq}</strong><span
                >{event.type.replace(/([a-z])([A-Z])/g, '$1 $2')}</span
              ><span class="repo-muted repo-small"
                >{new Date(event.created_at).toLocaleString()}</span
              ><button
                onclick={() => {
                  recoveryAt = event.seq;
                  recoveryOpen = true;
                }}>Inspect revision</button
              >
            </div>
          </article>{/each}
      </div>
      <div class="repo-pagination">
        <button
          disabled={busy || historyEnd >= doc.latest_seq}
          onclick={() => run(() => history(Math.min(doc!.latest_seq, historyEnd + 50)))}
          >Newer changes</button
        ><button
          disabled={busy || historyEnd <= 50}
          onclick={() => run(() => history(historyEnd - 50))}>Older changes</button
        >
      </div>
    {/if}
  {/if}
</RepositoryShell>
{#if reviewId && doc && access}<ChangeRequestReview
    documentId={id}
    requestId={reviewId}
    team={doc.state}
    teamSeq={doc.latest_seq}
    canMerge={access.you.can.merge && requests.some((r) => r.id === reviewId && !!r.submitted_at)}
    canComment={access.you.can.comment}
    onClose={() => (reviewId = '')}
    onChanged={() => {
      reviewId = '';
      void run(load);
    }}
    onOpenDraft={(draft) => {
      void goto(`/documents/${id}?draft=${draft}`);
    }}
  />{/if}
{#if push && doc}<PushFileDialog
    documentId={id}
    title={doc.title}
    kind={kind as PushKind}
    onClose={() => (push = false)}
    onCreated={() => {
      push = false;
      tab = 'requests';
      void run(load);
      notice = 'Changes saved for review';
    }}
  />{/if}
