<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import {
    FileText,
    GitBranch,
    GitPullRequest,
    GitMerge,
    GitPullRequestClosed,
    GitCommitHorizontal,
    History,
    ArrowDownToLine,
    Plug,
    PanelsTopLeft,
    Plus,
    ChevronDown,
    ShieldCheck,
    ShieldAlert
  } from '@lucide/svelte';
  import RepositoryShell from '$lib/components/product/RepositoryShell.svelte';
  import FilePreview from '$lib/components/product/FilePreview.svelte';
  import ProvenancePanel from '$lib/components/product/ProvenancePanel.svelte';
  import ChangeRequestReview from '$lib/components/workspace/ChangeRequestReview.svelte';
  import AgentBadge from '$lib/components/workspace/AgentBadge.svelte';
  import DiffText from '$lib/components/workspace/DiffText.svelte';
  import HistoryRecovery from '$lib/components/workspace/HistoryRecovery.svelte';
  import PushFileDialog from '$lib/components/workspace/PushFileDialog.svelte';
  import Avatar from './repo/Avatar.svelte';
  import FormatIcon from './repo/FormatIcon.svelte';
  import HashChip from './repo/HashChip.svelte';
  import RelTime from './repo/RelTime.svelte';
  import TabNav from './repo/TabNav.svelte';
  import { workspaceRequest, documentKind, byPosition, type Draft } from '$lib/workspace/api';
  import { people, type People } from '$lib/workspace/collaboration';
  import {
    listRequests,
    submitDraft,
    type ChangeRequest,
    type PushKind
  } from '$lib/workspace/reviews';
  import {
    exportDocument,
    exportSpreadsheet,
    exportPresentation,
    textOf,
    type TextBlock
  } from '$lib/workspace/files';
  import { kindLabel } from '$lib/workspace/activity';
  import { formatOf, groupByDay, hashHex, relativeTime } from '$lib/workspace/repo-ui';
  import type {
    DocumentDetail,
    DocumentState,
    EventPayload,
    EventRow,
    MaterializedNode
  } from '$lib/api/types';
  type FileDraft = Draft & {
    submitted_at: string | null;
    review_outcome: string | null;
    revision: number;
  };
  type Change = { id: string; kind: 'added' | 'edited' | 'removed'; before: string; after: string };
  let doc = $state<DocumentDetail | null>(null),
    access = $state<People | null>(null),
    project = $state<{ id: string; name: string } | null>(null);
  let drafts = $state<FileDraft[]>([]),
    requests = $state<ChangeRequest[]>([]),
    events = $state<EventRow[]>([]),
    latestEvent = $state<EventRow | null>(null);
  let branch = $state(''),
    tab = $state('file'),
    draftName = $state(''),
    note = $state(''),
    diffView = $state<'unified' | 'split'>('unified');
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
    recoveryOpen = $state(false),
    verdict = $state<{ ok: boolean; broken_at_seq: number | null; seq: number } | null>(null);
  const id = $derived($page.params.fileId || ''),
    projectId = $derived($page.params.id || '');
  const base = $derived(`/documents/${id}`),
    route = $derived(`/projects/${projectId}/files/${id}`);
  const kind = $derived(doc ? documentKind(doc) : 'document');
  const previewState = $derived(branch ? selected?.state : doc?.state);
  const activeDrafts = $derived(drafts.filter((d) => !d.merged_at && !d.review_outcome));
  const openRequests = $derived(requests.filter((r) => r.status === 'open').length);
  const tabs = $derived([
    { key: 'file', label: 'File', icon: FileText, count: null },
    { key: 'requests', label: 'Pull requests', icon: GitPullRequest, count: openRequests || null },
    { key: 'history', label: 'History', icon: History, count: null },
    { key: 'sync', label: 'Sync & connections', icon: Plug, count: null }
  ]);
  const names = $derived(
    new Map<string, string>([
      ...(access?.members || []).map(
        (m) => [m.id, m.name || (m.email || '').split('@')[0]] as [string, string]
      ),
      ...(access?.owner ? [[access.owner.id, access.owner.name || ''] as [string, string]] : [])
    ])
  );
  const actorOf = (event: EventRow) => names.get(event.actor_id) || 'A project member';
  const format = $derived(formatOf(kind));
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
    void readLatest(file.latest_seq);
  }
  /** The newest revision, for the "latest change" bar. Quiet: the bar simply stays out if it fails. */
  async function readLatest(seq: number) {
    latestEvent = null;
    if (seq < 1) return;
    try {
      const result = await workspaceRequest<{ items: EventRow[] }>(
        `${base}/events?from_seq=${seq}&to_seq=${seq}&limit=1`
      );
      latestEvent = result.items[0] ?? null;
    } catch {
      latestEvent = null;
    }
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
  async function verify() {
    if (!doc) return;
    const seq = doc.latest_seq;
    const result = await workspaceRequest<{ ok: boolean; broken_at_seq?: number | null }>(
      `${base}/verify`
    );
    verdict = { ok: result.ok === true, broken_at_seq: result.broken_at_seq ?? null, seq };
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
  const nodeText = (node?: MaterializedNode) =>
    node && !node.deleted
      ? node.current_fields.content
        ? textOf(node.current_fields.content as TextBlock)
        : String(node.current_fields.label ?? '')
      : '';
  /** Content that differs between main and the selected draft, in document order. */
  const changes = $derived.by<Change[]>(() => {
    if (!doc || !selected) return [];
    const main = doc.state.nodes,
      draft = selected.state.nodes;
    const out: { node: MaterializedNode; change: Change }[] = [];
    for (const key of new Set([...Object.keys(main), ...Object.keys(draft)])) {
      const a = main[key],
        b = draft[key];
      if ((a || b).type === 'form') continue;
      const before = nodeText(a),
        after = nodeText(b);
      if (before === after) continue;
      out.push({
        node: (b || a) as MaterializedNode,
        change: {
          id: key,
          kind: !a || a.deleted ? 'added' : !b || b.deleted ? 'removed' : 'edited',
          before,
          after
        }
      });
    }
    return out.sort((x, y) => byPosition(x.node, y.node)).map((x) => x.change);
  });
  onMount(() => {
    const requested = $page.url.searchParams.get('tab');
    if (tabs.some(({ key }) => key === requested)) tab = requested!;
    const draft = $page.url.searchParams.get('draft');
    if (draft && /^[0-9a-f-]{36}$/i.test(draft)) branch = draft;
    void run(load);
  });
</script>

<svelte:head><title>{doc?.title || 'File'} · Dynodoc</title></svelte:head>
<RepositoryShell
  {projectId}
  crumbs={[
    { label: 'Projects', href: '/projects' },
    { label: project?.name || 'Project', href: `/projects/${projectId}` },
    { label: doc?.title || 'File' }
  ]}
>
  <div class="rp-band">
    <div class="repo-wrap">
      <div class="rp-title">
        <FormatIcon {kind} size={20} />
        <a class="rp-owner" href={`/projects/${projectId}`}>{project?.name || 'Project'}</a><span
          class="repo-muted"
          aria-hidden="true">/</span
        >
        <h1>{doc?.title || 'Loading file…'}</h1>
        {#if doc}<span class="repo-badge">{format.label}</span>{/if}
        {#if doc && access}<div class="rp-title-actions">
            <a class="repo-btn" href={`/documents/${id}${branch ? `?draft=${branch}` : ''}`}
              ><PanelsTopLeft size={15} aria-hidden="true" /> Web editor</a
            >
            {#if kind !== 'questionnaire'}<button
                disabled={busy || !previewState}
                onclick={() => run(downloadFile)}
                ><ArrowDownToLine size={15} aria-hidden="true" /> Download file</button
              >{/if}
          </div>{/if}
      </div>
      {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}{#if notice}<p
          class="repo-notice"
          role="status"
        >
          {notice}
        </p>{/if}
      {#if doc && access}<TabNav
          label="File sections"
          current={tab}
          {tabs}
          {busy}
          onSelect={(key) => run(() => selectTab(key))}
        />{/if}
    </div>
  </div>
  <div class="repo-wrap rp-body">
    {#if doc && access}
      <div class="rp-toolbar">
        <div class="rp-branch" class:draft={!!branch}>
          <GitBranch size={15} aria-hidden="true" /><select
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
          ><ChevronDown size={13} aria-hidden="true" />
        </div>
        {#if access.you.can.propose}<button disabled={busy} onclick={() => (creating = !creating)}
            ><Plus size={15} aria-hidden="true" /> New file draft</button
          >{/if}
        <span class="repo-muted"
          >{branch
            ? `Based on main r${selected?.draft.base_seq ?? '…'}`
            : `main · revision ${doc.latest_seq}`}</span
        >
      </div>
      {#if creating}<form
          class="repo-box repo-panel-body repo-form"
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
      {#if branch && selected}<div class="repo-box repo-panel-body">
          <div class="repo-actions">
            <GitBranch size={17} aria-hidden="true" /><strong>{selected.draft.name}</strong><span
              class="repo-badge"
              >{selected.draft.submitted_at ? 'In review' : 'Private file draft'}</span
            ><button disabled={busy} onclick={() => (reviewId = branch)}>Compare with main</button
            ><a class="repo-btn" href={`/documents/${id}?draft=${branch}`}>Edit draft</a>
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
      {#if tab === 'file'}
        {#if branch && selected}
          <section class="repo-box" aria-labelledby="draft-changes">
            <div class="repo-box-head">
              <GitPullRequest size={15} aria-hidden="true" />
              <h2 id="draft-changes">Changes from main</h2>
              <span class="repo-counter">{changes.length}</span>
              <div class="repo-seg rp-seg-end" role="group" aria-label="Compare layout">
                <button
                  type="button"
                  aria-pressed={diffView === 'unified'}
                  onclick={() => (diffView = 'unified')}>Unified</button
                ><button
                  type="button"
                  aria-pressed={diffView === 'split'}
                  onclick={() => (diffView = 'split')}>Split</button
                >
              </div>
            </div>
            {#if !changes.length}<p class="repo-empty">
                This draft has no content differences from main yet.
              </p>{/if}
            {#each changes.slice(0, 40) as change (change.id)}
              <div class={`rp-diff ${diffView}`}>
                <span class={`rp-diff-kind ${change.kind}`}>{change.kind}</span>
                {#if diffView === 'split'}
                  <p class="before">{change.before || '—'}</p>
                  <div class="after"><DiffText before={change.before} after={change.after} /></div>
                {:else}
                  <div class="after"><DiffText before={change.before} after={change.after} /></div>
                {/if}
              </div>
            {/each}
            {#if changes.length > 40}<p class="repo-muted repo-small rp-diff-more">
                {changes.length - 40} more changed blocks. Use Compare with main to review them all.
              </p>{/if}
          </section>
        {/if}
        {#if latestEvent && !branch}<div class="rp-latest">
            <Avatar name={actorOf(latestEvent)} size={22} />
            <strong>{actorOf(latestEvent)}</strong>
            <span class="rp-summary-text">{kindLabel(latestEvent.type)}</span>
            <span class="rp-summary-meta"
              ><HashChip hash={hashHex(latestEvent.chain_hash)} label="revision hash" /><span
                >r{latestEvent.seq}</span
              ><RelTime at={latestEvent.created_at} /></span
            >
          </div>{/if}
        <section class="repo-box rp-blob">
          <div class="repo-box-head">
            <FormatIcon {kind} /><strong>{doc.title}</strong><span class="repo-muted"
              >Read-only preview · {kind}</span
            >
          </div>
          {#if previewState}{#key `${branch}:${doc.latest_seq}:${selected?.draft.revision || 0}`}<FilePreview
                documentState={previewState}
                {kind}
              />{/key}{:else}<p class="repo-empty">Loading file draft…</p>{/if}
        </section>
      {:else if tab === 'sync'}<section class="repo-box repo-panel-body">
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
      {:else if tab === 'requests'}
        <div class="rp-section-head">
          <h2>Pull requests</h2>
          {#if kind !== 'questionnaire' && access.you.can.propose}<button
              class="repo-primary"
              onclick={() => (push = true)}>Push an edited file</button
            >{/if}
        </div>
        <div class="repo-box">
          {#if !requests.length}<div class="repo-empty">
              <GitPullRequest size={28} aria-hidden="true" />
              <h2>No pull requests yet</h2>
              <p>Push an edited file or submit a private draft to start a review.</p>
            </div>{/if}{#each requests as r (r.id)}<article class="rp-pr">
              <span class={`rp-status ${r.status}`} aria-hidden="true">
                {#if r.status === 'merged'}<GitMerge
                    size={16}
                  />{:else if r.status === 'open'}<GitPullRequest
                    size={16}
                  />{:else}<GitPullRequestClosed size={16} />{/if}
              </span>
              <div class="rp-pr-main">
                <button class="repo-linkbtn strong" onclick={() => (reviewId = r.id)}
                  >{r.name}</button
                >
                <span class="repo-badge">{r.status}</span>{#if r.agent}<AgentBadge />{/if}
                <span class="rp-pr-by"
                  >{r.author.name || 'Project member'} · Based on revision {r.base_seq}{#if r.submitted_at}
                    · opened {relativeTime(r.submitted_at)}{/if}</span
                >
              </div>
            </article>{/each}
        </div>
      {:else if tab === 'history'}
        <div class="rp-section-head">
          <h2>Main history</h2>
          <div class="repo-actions">
            <button disabled={busy} onclick={() => run(verify)}
              ><ShieldCheck size={15} aria-hidden="true" /> Verify hash chain</button
            ><button
              onclick={() => {
                recoveryAt = doc!.latest_seq;
                recoveryOpen = !recoveryOpen;
              }}>Recover an earlier revision</button
            >
          </div>
        </div>
        <p class="repo-muted">
          Every recorded change stays in the file's history. Download an earlier state, restore
          selected blocks, revert or reapply changes.
        </p>
        {#if verdict}<p class={`rp-verdict ${verdict.ok ? 'ok' : 'bad'}`}>
            {#if verdict.ok}<ShieldCheck size={15} aria-hidden="true" />Verified: the hash chain is
              intact through r{verdict.seq}.{:else}<ShieldAlert size={15} aria-hidden="true" />The
              hash chain does not verify{verdict.broken_at_seq
                ? ` at r${verdict.broken_at_seq}`
                : ''}.{/if}
          </p>{/if}
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
        <div class="repo-box">
          {#if !events.length}<p class="repo-empty">No recorded changes yet.</p>{/if}
          {#each groupByDay(events, (e) => e.created_at) as group (group.key)}
            <div class="rp-day-head">
              <GitCommitHorizontal size={15} aria-hidden="true" />Revisions {group.label ===
                'Today' || group.label === 'Yesterday'
                ? group.label.toLowerCase()
                : `on ${group.label}`}
            </div>
            {#each group.items as event (event.seq)}
              <article class="rp-commit">
                <div class="rp-commit-main">
                  <p class="rp-commit-title">
                    {kindLabel(event.type)}<span class="rev">r{event.seq}</span>
                  </p>
                  <p class="rp-commit-meta">
                    <Avatar name={actorOf(event)} size={18} /><strong>{actorOf(event)}</strong>
                    changed this <RelTime at={event.created_at} />
                  </p>
                </div>
                <div class="rp-commit-side">
                  <HashChip hash={hashHex(event.chain_hash)} label="revision hash" />
                  <button
                    onclick={() => {
                      recoveryAt = event.seq;
                      recoveryOpen = true;
                    }}>Inspect revision</button
                  >
                </div>
              </article>
            {/each}
          {/each}
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
    {:else if !error}
      <div class="repo-skel-stack" aria-busy="true">
        {#each [0, 1, 2, 3, 4] as n (n)}<div class="repo-skel-row" aria-hidden="true">
            <span class="repo-skel" style:width="{34 + ((n * 9) % 36)}%"></span>
          </div>{/each}
      </div>
    {/if}
  </div>
</RepositoryShell>
<div class="repo-tokens">
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
</div>
