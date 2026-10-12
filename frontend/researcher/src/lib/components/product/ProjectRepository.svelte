<script lang="ts">
  import { onMount, tick } from 'svelte';
  import MoveProjectFile from './MoveProjectFile.svelte';
  import RepositoryShell from '$lib/components/product/RepositoryShell.svelte';
  import {
    FolderGit2,
    FileText,
    GitBranch,
    GitPullRequest,
    GitMerge,
    GitPullRequestClosed,
    GitGraph,
    Settings2,
    LockKeyhole,
    Upload,
    PanelsTopLeft,
    History,
    Activity,
    Eye,
    ChevronDown,
    Check,
    Search,
    Plus,
    BookOpen,
    Plug
  } from '@lucide/svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { workspaceRequest, saveBatch } from '$lib/workspace/api';
  import { createDocument } from '$lib/api/client';
  import type { DocumentDetail, DocumentState } from '$lib/api/types';
  import { importFile } from '$lib/workspace/files';
  import { people, inviteByEmail, invitationNotice } from '$lib/workspace/collaboration';
  import { projectRoleName, downloadJson, type Project } from '$lib/workspace/projects';
  import {
    loadActivity,
    loadContributors,
    nameIndex,
    type ActivityItem,
    type Contributor,
    type MemberLite,
    type RepoFile,
    type RepoRequest
  } from '$lib/workspace/activity';
  import {
    absoluteTime,
    displayName,
    formatBreakdown,
    plural,
    relativeTime
  } from '$lib/workspace/repo-ui';
  import ProvenancePanel from '$lib/components/product/ProvenancePanel.svelte';
  import ChangeRequestReview from '$lib/components/workspace/ChangeRequestReview.svelte';
  import PushFileDialog from '$lib/components/workspace/PushFileDialog.svelte';
  import Avatar from './repo/Avatar.svelte';
  import FormatIcon from './repo/FormatIcon.svelte';
  import FormatsBar from './repo/FormatsBar.svelte';
  import Menu from './repo/Menu.svelte';
  import RelTime from './repo/RelTime.svelte';
  import TabNav from './repo/TabNav.svelte';
  import ActivityFeed from './repo/ActivityFeed.svelte';
  import ProjectBranches from './repo/ProjectBranches.svelte';

  type Listing = 'files' | 'requests' | 'drafts';
  const prFilters = [
    { key: 'open', label: 'Open', icon: GitPullRequest },
    { key: 'merged', label: 'Merged', icon: GitMerge },
    { key: 'declined', label: 'Declined', icon: GitPullRequestClosed }
  ] as const;
  let project = $state<Project | null>(null),
    tab = $state('files'),
    files = $state<RepoFile[]>([]),
    requests = $state<RepoRequest[]>([]),
    drafts = $state<RepoRequest[]>([]),
    members = $state<MemberLite[]>([]),
    contributors = $state<Contributor[]>([]),
    contributorsDerived = $state(true),
    activity = $state<ActivityItem[]>([]),
    activityNext = $state<string | null>(null),
    activityDerived = $state(false),
    activityLoaded = $state(false);
  let offsets = $state<Record<Listing, number>>({ files: 0, requests: 0, drafts: 0 }),
    mores = $state<Record<Listing, boolean>>({ files: false, requests: false, drafts: false }),
    seen = $state<Record<Listing, boolean>>({ files: false, requests: false, drafts: false });
  let error = $state(''),
    notice = $state(''),
    busy = $state(false),
    loaded = $state(false),
    prFilter = $state<'open' | 'merged' | 'declined'>('open'),
    email = $state(''),
    role = $state('reviewer'),
    notify = $state(true),
    note = $state('');
  let sync = $state<RepoFile | null>(null),
    push = $state<RepoFile | null>(null),
    review = $state<{
      request: RepoRequest;
      doc: DocumentDetail;
      canMerge: boolean;
      canComment: boolean;
    } | null>(null),
    fileInput = $state<HTMLInputElement | null>(null),
    moveOpen = $state(false),
    readme = $state<{ id: string; title: string; state: DocumentState } | null>(null),
    Preview = $state<typeof import('./FilePreview.svelte').default | null>(null);
  const id = $derived($page.params.id || ''),
    manage = $derived(project && ['owner', 'manager'].includes(project.role)),
    canImport = $derived(project && ['owner', 'manager', 'editor'].includes(project.role));
  const tabs = $derived([
    {
      key: 'files',
      label: 'Files',
      icon: FileText,
      count: seen.files ? count(files, mores.files) : null
    },
    {
      key: 'requests',
      label: 'Pull requests',
      icon: GitPullRequest,
      count: seen.requests
        ? count(
            requests.filter((r) => r.status === 'open'),
            mores.requests
          )
        : null
    },
    {
      key: 'drafts',
      label: 'Your drafts',
      icon: GitBranch,
      count: seen.drafts ? count(drafts, mores.drafts) : null
    },
    { key: 'branches', label: 'Branches', icon: GitGraph, count: null },
    { key: 'activity', label: 'Activity', icon: Activity, count: null },
    { key: 'editors', label: 'Web editors', icon: PanelsTopLeft, count: null },
    { key: 'settings', label: 'Settings', icon: Settings2, count: null }
  ]);
  function count(list: unknown[], more: boolean) {
    return `${list.length}${more ? '+' : ''}`;
  }
  const owner = $derived(members.find((m) => m.role === 'owner')),
    ownerName = $derived(owner ? displayName(owner.name, owner.email) : ''),
    formats = $derived(formatBreakdown(files)),
    revisions = $derived(files.reduce((n, f) => n + f.latest_seq, 0)),
    latest = $derived(
      files.reduce<RepoFile | null>((best, f) => {
        const at = (x: RepoFile) => Date.parse(x.last_change?.at || x.updated_at) || 0;
        return !best || at(f) > at(best) ? f : best;
      }, null)
    ),
    groups = $derived({
      open: requests.filter((r) => r.status === 'open'),
      merged: requests.filter((r) => r.status === 'merged'),
      declined: requests.filter((r) => r.status === 'declined' || r.status === 'withdrawn')
    }),
    shownRequests = $derived(groups[prFilter]),
    editorMode = $derived(tab === 'editors');

  async function fetchList(which: Listing) {
    const path = which === 'requests' ? 'change-requests' : which;
    const result = await workspaceRequest<{ items: unknown[]; more: boolean }>(
      `/projects/${id}/${path}?offset=${offsets[which]}`
    );
    mores[which] = result.more;
    seen[which] = true;
    if (which === 'files') files = result.items as RepoFile[];
    else if (which === 'requests') requests = result.items as RepoRequest[];
    else drafts = result.items as RepoRequest[];
  }
  async function loadMembers() {
    members = (await workspaceRequest<{ items: MemberLite[] }>(`/spaces/${id}/members`)).items;
  }
  async function loadFeed(more = false) {
    const result = await loadActivity(
      id,
      files,
      nameIndex(members),
      more ? activityNext : null,
      50
    );
    activity = more ? [...activity, ...result.items] : result.items;
    activityNext = result.next;
    activityDerived = result.derived;
    activityLoaded = true;
  }
  async function loadTab() {
    if (tab === 'settings') {
      await loadMembers();
      return;
    }
    // The Branches tab reads shared history itself, and aborts when the tab is left.
    if (tab === 'branches') return;
    if (tab === 'activity') {
      if (!members.length) await loadMembers().catch(() => {});
      if (!seen.files) await fetchList('files');
      await loadFeed();
      return;
    }
    await fetchList(tab === 'editors' ? 'files' : (tab as Listing));
  }
  /** Counters, contributors and the README load quietly; they never raise the page error. */
  async function loadExtras() {
    const wanted = (['files', 'requests', 'drafts'] as Listing[]).filter((w) => !seen[w]);
    await Promise.allSettled([...wanted.map(fetchList), loadMembers()]);
    const list = await loadContributors(id, members).catch(() => null);
    if (list) {
      contributors = list.contributors;
      contributorsDerived = list.derived;
    }
    await loadReadme();
  }
  async function loadReadme() {
    const file = files.find(
      (f) => f.kind === 'document' && /^readme(\.[a-z0-9]+)?$/i.test(f.title)
    );
    if (!file) {
      readme = null;
      return;
    }
    if (readme?.id === file.id) return;
    try {
      const [doc, module] = await Promise.all([
        workspaceRequest<DocumentDetail>(`/documents/${file.id}`),
        import('./FilePreview.svelte')
      ]);
      Preview = module.default;
      readme = { id: file.id, title: file.title, state: doc.state };
    } catch {
      readme = null;
    }
  }
  async function load() {
    project = await workspaceRequest<Project>(`/projects/${id}`);
    await loadTab();
    loaded = true;
    void loadExtras();
  }
  async function run(action: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = '';
    try {
      await action();
    } catch (e) {
      error = e instanceof Error ? e.message : 'The request failed';
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    const requested = $page.url.searchParams.get('tab');
    if (requested && tabs.some((t) => t.key === requested)) tab = requested;
    void run(load);
    const add = () => void openUpload();
    window.addEventListener('dynodoc:add-file', add);
    return () => window.removeEventListener('dynodoc:add-file', add);
  });
  async function select(value: string) {
    tab = value;
    await goto(`/projects/${id}?tab=${value}`, { replaceState: true, noScroll: true });
    offsets = { files: 0, requests: 0, drafts: 0 };
    sync = null;
    await loadTab();
  }
  async function openUpload() {
    if (!canImport) return;
    if (tab !== 'files' && tab !== 'editors') await run(() => select('files'));
    await tick();
    fileInput?.click();
  }
  async function importNew(file?: File) {
    if (!file) return;
    notice = 'Reading file…';
    const imported = await importFile(file);
    if (imported.warnings.length) notice = imported.warnings.join(' ');
    const doc = await createDocument({ title: file.name.replace(/\.[^.]+$/, '') });
    try {
      await workspaceRequest(`/documents/${doc.id}/metadata`, {
        settings: { kind: imported.kind, import_warnings: imported.warnings }
      });
      await saveBatch(doc.id, imported.ops, 0);
      const bytes = new Uint8Array(await file.arrayBuffer());
      let binary = '';
      for (let i = 0; i < bytes.length; i += 8192)
        binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
      await workspaceRequest(`/documents/${doc.id}/uploads`, {
        filename: file.name,
        content_type: file.type || 'application/octet-stream',
        content: btoa(binary),
        report: { warnings: imported.warnings }
      });
      await workspaceRequest(`/documents/${doc.id}/location`, { space_id: id });
    } catch (e) {
      throw new Error(
        `Import stopped. The new file is recoverable in the Editors library (${doc.id}). ${e instanceof Error ? e.message : ''}`
      );
    }
    notice = `${file.name} added. ${imported.warnings.join(' ')}`;
    await loadTab();
  }
  async function openReview(r: RepoRequest) {
    const doc = await workspaceRequest<DocumentDetail>(`/documents/${r.document.id}`),
      access = await people(r.document.id);
    review = {
      request: r,
      doc,
      canMerge: access.you.can.merge,
      canComment: access.you.can.comment
    };
  }
  async function invite() {
    const result = await inviteByEmail({ space_id: id }, email, role, { notify, note });
    notice = invitationNotice(result.email_status, email);
    email = '';
    await loadTab();
  }
  async function setWatch(level: 'ignore' | 'requests') {
    if (!project || project.watch === level) return;
    await run(async () => {
      await workspaceRequest(`/projects/${id}/watch`, { level });
      project = project ? { ...project, watch: level } : project;
    });
  }
  function draftUrl(d: RepoRequest) {
    return `/projects/${id}/files/${d.document.id}?draft=${d.id}`;
  }
  /** What a file row says about its newest change; never just repeats the file name. */
  const changeText = (f: RepoFile) => {
    const text = f.last_change?.summary?.trim();
    return text && text.toLowerCase() !== f.title.toLowerCase() ? text : '';
  };
</script>

<svelte:head
  ><title>{project?.name || 'Project'} · Dynodoc</title><meta
    name="robots"
    content="noindex"
  /></svelte:head
>
<RepositoryShell
  projectId={id}
  canAdd={!!canImport}
  crumbs={[{ label: 'Projects', href: '/projects' }, { label: project?.name || 'Project' }]}
>
  <div class="rp-band">
    <div class="repo-wrap">
      <div class="rp-title">
        <FolderGit2 size={18} aria-hidden="true" />
        {#if ownerName}<a class="rp-owner" href={owner?.id ? `/profiles/${owner.id}` : '/projects'}
            >{ownerName}</a
          ><span class="repo-muted" aria-hidden="true">/</span>{/if}
        <h1>{project?.name || 'Loading project...'}</h1>
        <span class="repo-badge"><LockKeyhole size={12} aria-hidden="true" /> Private</span>
        {#if project}<div class="rp-title-actions">
            <Menu align="right" title="Choose project notifications">
              {#snippet trigger()}<Eye size={15} aria-hidden="true" />{project?.watch === 'requests'
                  ? 'Watching'
                  : 'Watch'}<ChevronDown size={13} aria-hidden="true" />{/snippet}
              <p class="menu-head" role="presentation">Notifications for this project</p>
              <button
                type="button"
                role="menuitemradio"
                aria-checked={project.watch !== 'requests'}
                onclick={() => void setWatch('ignore')}
                ><span class="menu-check-slot"
                  >{#if project.watch !== 'requests'}<Check
                      size={14}
                      aria-hidden="true"
                    />{/if}</span
                ><span
                  ><strong>Standard</strong><small
                    >Only the notifications your role already receives.</small
                  ></span
                ></button
              >
              <button
                type="button"
                role="menuitemradio"
                aria-checked={project.watch === 'requests'}
                onclick={() => void setWatch('requests')}
                ><span class="menu-check-slot"
                  >{#if project.watch === 'requests'}<Check
                      size={14}
                      aria-hidden="true"
                    />{/if}</span
                ><span
                  ><strong>Pull requests</strong><small
                    >Notify me when changes are sent for review.</small
                  ></span
                ></button
              >
            </Menu>
          </div>{/if}
      </div>
      {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}{#if notice}<p
          class="repo-notice"
          role="status"
        >
          {notice}
        </p>{/if}
      {#if project}<TabNav
          label="Project sections"
          current={tab}
          {tabs}
          {busy}
          onSelect={(key) => run(() => select(key))}
        />{/if}
    </div>
  </div>
  <div class="repo-wrap rp-body">
    {#if project}
      <div class="rp-layout">
        <div class="rp-primary">
          {#if tab === 'files' || tab === 'editors'}
            {#if editorMode}<h2>Web editors</h2>
              <p class="repo-muted">
                Open a file in the built-in editor. Use Files, pull requests and history to manage
                the team's shared work.
              </p>{/if}
            <div class="rp-toolbar">
              <Menu title="Switch branch">
                {#snippet trigger()}<GitBranch size={15} aria-hidden="true" />main<ChevronDown
                    size={13}
                    aria-hidden="true"
                  />{/snippet}
                <p class="menu-head" role="presentation">Switch branches or drafts</p>
                <button type="button" role="menuitemradio" aria-checked="true"
                  ><span class="menu-check-slot"><Check size={14} aria-hidden="true" /></span
                  >main</button
                >
                <button type="button" role="menuitem" onclick={() => run(() => select('drafts'))}
                  ><span class="menu-check-slot"></span><span
                    >Your drafts<small
                      >Private branches of single files{seen.drafts
                        ? ` · ${count(drafts, mores.drafts)}`
                        : ''}</small
                    ></span
                  ></button
                >
              </Menu>
              <button type="button" class="repo-linkbtn" onclick={() => run(() => select('drafts'))}
                ><GitBranch size={14} aria-hidden="true" />{seen.drafts
                  ? plural(drafts.length, 'draft')
                  : 'Drafts'}</button
              >
              <span class="rp-spacer"></span>
              <button
                type="button"
                onclick={() =>
                  window.dispatchEvent(
                    new CustomEvent('dynodoc:palette', { detail: { mode: 'files' } })
                  )}><Search size={14} aria-hidden="true" />Go to file <kbd>t</kbd></button
              >
              {#if canImport}
                <Menu align="right" title="Add a file to this project">
                  {#snippet trigger()}<Plus size={15} aria-hidden="true" />Add file<ChevronDown
                      size={13}
                      aria-hidden="true"
                    />{/snippet}
                  <button type="button" role="menuitem" onclick={() => fileInput?.click()}
                    ><Upload size={16} aria-hidden="true" />Upload a file</button
                  >
                  <button
                    type="button"
                    role="menuitem"
                    onclick={() => {
                      moveOpen = true;
                      void tick().then(() =>
                        document
                          .getElementById('move-project-file')
                          ?.scrollIntoView({ block: 'center' })
                      );
                    }}><FolderGit2 size={16} aria-hidden="true" />Move an existing file</button
                  >
                </Menu>
                <input
                  bind:this={fileInput}
                  type="file"
                  hidden
                  disabled={busy}
                  accept=".docx,.doc,.xlsx,.csv,.pptx,.txt,.md,.tex,.zip"
                  onchange={(e) => {
                    const f = e.currentTarget.files?.[0];
                    e.currentTarget.value = '';
                    void run(() => importNew(f));
                  }}
                />
              {/if}
            </div>
            <div class="repo-box" aria-busy={!loaded}>
              {#if !loaded}
                {#each [0, 1, 2, 3, 4] as n (n)}<div class="repo-skel-row" aria-hidden="true">
                    <span class="repo-skel" style:width="16px"></span><span
                      class="repo-skel"
                      style:width="{22 + ((n * 17) % 30)}%"
                    ></span><span class="repo-skel" style:width="{14 + ((n * 11) % 20)}%"></span>
                  </div>{/each}
              {:else if !files.length}
                <div class="repo-empty">
                  <FileText size={30} aria-hidden="true" />
                  <h2>Add your first file</h2>
                  <p>
                    Upload a document, workbook or presentation up to 3 MB. Your team can then
                    propose and review changes.
                  </p>
                  {#if canImport}<button class="repo-primary" onclick={() => fileInput?.click()}
                      ><Upload size={15} aria-hidden="true" />Upload a file</button
                    >{/if}
                </div>
              {:else}
                <div class="rp-summary">
                  {#if latest?.last_change}
                    <Avatar name={latest.last_change.actor_name || 'Project member'} size={22} />
                    <strong>{latest.last_change.actor_name || 'A project member'}</strong>
                    <span class="rp-summary-text"
                      >{changeText(latest) || `Updated a file to r${latest.last_change.seq}`}</span
                    >
                    <span class="rp-summary-meta"><RelTime at={latest.last_change.at} /></span>
                  {:else if latest}
                    <History size={15} aria-hidden="true" />
                    <span class="rp-summary-text">
                      Updated {latest.title}
                      <RelTime at={latest.updated_at} />
                    </span>
                  {/if}
                  <button
                    type="button"
                    class="repo-linkbtn rp-revisions"
                    onclick={() => run(() => select('activity'))}
                    ><History size={14} aria-hidden="true" />{plural(
                      revisions,
                      'revision'
                    )}{mores.files ? '+' : ''}</button
                  >
                </div>
                <ul
                  class="rp-files"
                  aria-label={editorMode
                    ? 'Files you can open in a web editor'
                    : 'Files in this project'}
                >
                  {#each files as f (f.id)}
                    <li>
                      <div class="c-name">
                        <FormatIcon kind={f.kind} /><a
                          href={editorMode ? `/documents/${f.id}` : `/projects/${id}/files/${f.id}`}
                          >{f.title}</a
                        >
                      </div>
                      <div class="c-msg">
                        {#if changeText(f)}<span class="msg">{changeText(f)}</span>{/if}<a
                          class="rev"
                          href={`/projects/${id}/files/${f.id}?tab=history`}
                          title="Open this file's history">r{f.latest_seq}</a
                        >
                      </div>
                      <div class="c-time">
                        <time
                          datetime={f.last_change?.at || f.updated_at}
                          title={absoluteTime(f.last_change?.at || f.updated_at)}
                          >{relativeTime(f.last_change?.at || f.updated_at)}</time
                        >
                      </div>
                      <div class="c-act">
                        <div class="repo-actions">
                          <button
                            aria-label={`Sync ${f.title}`}
                            disabled={busy}
                            onclick={() => (sync = f)}>Sync</button
                          >{#if ['document', 'spreadsheet', 'presentation'].includes(f.kind) && f.role !== 'viewer'}<button
                              aria-label={`Push ${f.title}`}
                              disabled={busy}
                              onclick={() => (push = f)}>Push</button
                            >{/if}
                        </div>
                      </div>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
            {#if canImport}<MoveProjectFile
                projectId={id}
                bind:open={moveOpen}
                onMoved={async () => {
                  await loadTab();
                  notice = 'File moved into this project';
                }}
              />{/if}
            {#if sync}<section class="repo-box" aria-label="Sync and connections">
                <div class="repo-box-head">
                  <Plug size={15} aria-hidden="true" /><strong>Sync and connections</strong>
                </div>
                <div class="repo-panel-body">
                  {#key sync.id}<ProvenancePanel
                      documentId={sync.id}
                      title={sync.title}
                      canPropose={sync.role !== 'viewer'}
                      onChanged={() => {
                        void run(loadTab);
                      }}
                    />{/key}
                </div>
              </section>{/if}
            {#if readme && Preview && !editorMode}
              <section class="repo-box rp-readme" aria-labelledby="readme-title">
                <div class="repo-box-head">
                  <BookOpen size={15} aria-hidden="true" />
                  <h2 id="readme-title">{readme.title}</h2>
                </div>
                <Preview documentState={readme.state} kind="document" />
              </section>
            {/if}
          {:else if tab === 'requests'}
            <div class="rp-section-head">
              <h2>Pull requests</h2>
              <div class="repo-seg" role="group" aria-label="Filter pull requests">
                {#each prFilters as filter (filter.key)}
                  <button
                    type="button"
                    aria-pressed={prFilter === filter.key}
                    onclick={() => (prFilter = filter.key)}
                    ><filter.icon size={14} aria-hidden="true" />{filter.label}<span
                      class="count"
                      aria-hidden="true"
                      >{groups[filter.key].length}{mores.requests ? '+' : ''}</span
                    ></button
                  >
                {/each}
              </div>
            </div>
            <p class="repo-muted">
              Review proposed file changes and merge approved work into main.
            </p>
            <div class="repo-box" aria-busy={!loaded}>
              {#if !requests.length}<p class="repo-empty">
                  No submitted requests on this page.
                </p>{:else if !shownRequests.length}<p class="repo-empty">
                  Nothing is {prFilter === 'open' ? 'waiting for review' : prFilter} on this page.
                </p>{/if}
              {#each shownRequests as r (r.id)}
                <article class="rp-pr">
                  <span class={`rp-status ${r.status}`} aria-hidden="true">
                    {#if r.status === 'merged'}<GitMerge
                        size={16}
                      />{:else if r.status === 'open'}<GitPullRequest
                        size={16}
                      />{:else}<GitPullRequestClosed size={16} />{/if}
                  </span>
                  <div class="rp-pr-main">
                    <strong>{r.name}</strong>
                    <span class="rp-pr-doc">{r.document.title} · {r.status}</span>
                    <span class="rp-pr-by"
                      >opened {relativeTime(r.submitted_at || r.created_at)} by {r.author?.name ||
                        'Project member'}{#if r.status !== 'open' && (r.reviewed_at || r.merged_at)}
                        · {r.status} {relativeTime(r.merged_at || r.reviewed_at)}{/if}</span
                    >
                  </div>
                  <div class="repo-actions">
                    <button disabled={busy} onclick={() => run(() => openReview(r))}
                      >Review changes</button
                    >{#if r.source?.bundle_id}<button
                        disabled={busy}
                        onclick={() =>
                          run(async () =>
                            downloadJson(
                              await workspaceRequest(
                                `/documents/${r.document.id}/provenance/bundles/${r.source.bundle_id}`
                              ),
                              `${r.source.bundle_id}.provenance.json`
                            )
                          )}>Download provenance</button
                      >{/if}
                  </div>
                </article>
              {/each}
            </div>
          {:else if tab === 'drafts'}
            <h2>Your drafts</h2>
            <p class="repo-muted">
              Each draft is a private branch of one file. Compare it with main, edit it and open a
              pull request when ready. Project-wide branches of shared history are listed under the
              Branches tab.
            </p>
            <div class="repo-box" aria-busy={!loaded}>
              {#if !drafts.length}<p class="repo-empty">No private drafts on this page.</p>{/if}
              {#each drafts as d (d.id)}
                <article class="rp-pr">
                  <span class="rp-status draft" aria-hidden="true"><GitBranch size={16} /></span>
                  <div class="rp-pr-main">
                    <a href={draftUrl(d)}>{d.name}</a>
                    <span class="rp-pr-doc">{d.document.title} · revision {d.revision}</span>
                    {#if d.created_at}<span class="rp-pr-by"
                        >created {relativeTime(d.created_at)}</span
                      >{/if}
                  </div>
                </article>
              {/each}
            </div>
          {:else if tab === 'branches'}
            <ProjectBranches
              projectId={id}
              requiredApprovals={project.policy.required_approvals}
              {members}
            />
          {:else if tab === 'activity'}
            <h2>Activity</h2>
            <p class="repo-muted">
              {activityDerived && activityLoaded
                ? 'Recent changes to the most recently updated files, newest first.'
                : 'Every recorded change across this project, newest first.'}
            </p>
            <div aria-busy={!activityLoaded}>
              {#if !activityLoaded}
                {#each [0, 1, 2, 3] as n (n)}<div class="repo-skel-row" aria-hidden="true">
                    <span class="repo-skel" style:width="28px"></span><span
                      class="repo-skel"
                      style:width="{40 + ((n * 13) % 30)}%"
                    ></span>
                  </div>{/each}
              {:else if !activity.length}
                <div class="repo-box"><p class="repo-empty">No recorded activity yet.</p></div>
              {:else}
                <ActivityFeed items={activity} projectId={id} />
                {#if activityNext}<div class="repo-pagination">
                    <button disabled={busy} onclick={() => run(() => loadFeed(true))}
                      >Show more</button
                    >
                  </div>{/if}
              {/if}
            </div>
          {:else}
            <h2>Project settings</h2>
            <p class="repo-muted">
              Files inherit these review rules unless they have their own override.
            </p>
            <form
              class="repo-box repo-panel-body"
              onsubmit={(e) => {
                e.preventDefault();
                void run(async () => {
                  await workspaceRequest(`/projects/${id}/settings`, {
                    name: project?.name,
                    description: project?.description,
                    revision: project?.revision,
                    ...project?.policy
                  });
                  notice = 'Project settings saved';
                  await load();
                });
              }}
            >
              <label
                >Name<input
                  bind:value={project.name}
                  maxlength="160"
                  required
                  disabled={!manage || busy}
                /></label
              ><label
                >Description<textarea
                  bind:value={project.description}
                  maxlength="2000"
                  disabled={!manage || busy}
                ></textarea></label
              >
              <label
                ><input
                  type="checkbox"
                  bind:checked={project.policy.protect_team_version}
                  disabled={!manage || busy}
                /> Require editors to send changes through review</label
              >
              <label
                >Required approvals<input
                  type="number"
                  min="0"
                  max="5"
                  bind:value={project.policy.required_approvals}
                  disabled={!manage || busy}
                /></label
              >
              <label
                >Who may merge<select
                  bind:value={project.policy.merge_roles}
                  disabled={!manage || busy}
                  ><option value="editors">Editors, managers and owners</option><option
                    value="owners">Managers and owners</option
                  ></select
                ></label
              >
              {#if manage}<button class="repo-primary" disabled={busy}>Save settings</button>{/if}
            </form>
            <h3>People</h3>
            <div class="repo-box">
              {#each members as m}<div class="rp-person">
                  <Avatar name={displayName(m.name, m.email)} size={28} />
                  <span class="rp-person-name"
                    ><a href={`/profiles/${m.id}`}>{m.name || m.email}</a> · {projectRoleName(
                      m.role
                    )}</span
                  >{#if manage && m.role !== 'owner'}<select
                      aria-label={`Role for ${m.email}`}
                      value={m.role}
                      disabled={busy}
                      onchange={(e) => {
                        const selected = e.currentTarget.value;
                        void run(async () => {
                          await workspaceRequest(`/spaces/${id}/members`, {
                            email: m.email,
                            role: selected
                          });
                          await loadTab();
                        });
                      }}
                      ><option value="manager">Manager</option><option value="editor">Editor</option
                      ><option value="approver">Reviewer</option><option value="reviewer"
                        >Contributor</option
                      ><option value="viewer">Viewer</option><option value="remove"
                        >Remove access</option
                      ></select
                    >{/if}
                </div>{/each}
            </div>
            {#if manage}<h3>Add a person</h3>
              <form
                class="repo-box repo-panel-body"
                onsubmit={(e) => {
                  e.preventDefault();
                  void run(invite);
                }}
              >
                <label>Email<input type="email" bind:value={email} required /></label><label
                  >Role<select bind:value={role}
                    ><option value="manager">Manager</option><option value="editor">Editor</option
                    ><option value="approver">Reviewer</option><option value="reviewer"
                      >Contributor</option
                    ><option value="viewer">Viewer</option></select
                  ></label
                ><label
                  ><input type="checkbox" bind:checked={notify} /> Send an email invitation with a link</label
                ><label>Short message<textarea bind:value={note} maxlength="500"></textarea></label
                ><button class="repo-primary" disabled={busy}>Add person</button>
              </form>{/if}
          {/if}
          {#if tab !== 'settings' && tab !== 'activity' && tab !== 'branches' && (offsets[tab === 'editors' ? 'files' : (tab as Listing)] > 0 || mores[tab === 'editors' ? 'files' : (tab as Listing)])}<footer
              class="repo-pagination"
            >
              <button
                disabled={busy || offsets[tab === 'editors' ? 'files' : (tab as Listing)] === 0}
                onclick={() =>
                  run(async () => {
                    const which = tab === 'editors' ? 'files' : (tab as Listing);
                    offsets[which] = Math.max(0, offsets[which] - 100);
                    await loadTab();
                  })}>Previous page</button
              ><button
                disabled={busy || !mores[tab === 'editors' ? 'files' : (tab as Listing)]}
                onclick={() =>
                  run(async () => {
                    const which = tab === 'editors' ? 'files' : (tab as Listing);
                    offsets[which] += 100;
                    await loadTab();
                  })}>Next page</button
              >
            </footer>{/if}
        </div>
        <aside class="rp-aside" aria-label="About this project">
          <section>
            <h2>About</h2>
            <p>{project.description || 'A shared home for team documents and changes.'}</p>
            <ul class="rp-facts">
              <li><LockKeyhole size={15} aria-hidden="true" /> Invite-only project</li>
              <li>
                <Eye size={15} aria-hidden="true" />
                {project.watch === 'requests' ? 'Watching pull requests' : 'Standard notifications'}
              </li>
              <li>
                <GitPullRequest size={15} aria-hidden="true" />
                <span>Your role: {projectRoleName(project.role)}</span>
              </li>
            </ul>
            <p class="repo-small">
              Edit in Word or Google Docs, Excel or Sheets, PowerPoint or Slides. Push changes here
              for review. <a href="/connectors">Install an editor connector</a>
            </p>
          </section>
          {#if formats.length}<section>
              <h2>Formats</h2>
              <FormatsBar shares={formats} />
            </section>{/if}
          <section>
            <h2>
              Contributors{#if contributors.length}<span class="repo-counter"
                  >{contributors.length}</span
                >{/if}
            </h2>
            {#if !contributors.length}<p class="repo-muted repo-small">
                {loaded ? 'Members appear here once the project has people.' : 'Loading people…'}
              </p>{/if}
            <ul class="rp-people">
              {#each contributors.slice(0, 8) as c (c.identity_id)}
                <li>
                  <a href={`/profiles/${c.identity_id}`}
                    ><Avatar name={c.name} size={28} /><span class="rp-people-name">{c.name}</span
                    ></a
                  >{#if !contributorsDerived && (c.merges || c.edits)}<span
                      class="repo-small repo-muted"
                      >{plural(c.merges, 'merge')} · {plural(c.edits, 'edit')}</span
                    >{/if}
                </li>
              {/each}
            </ul>
            {#if contributors.length > 8}<button
                type="button"
                class="repo-linkbtn"
                onclick={() => run(() => select('settings'))}
                >and {contributors.length - 8} more</button
              >{/if}
          </section>
          <section>
            <h2>Review rules</h2>
            <p class="repo-muted">
              {project.policy.protect_team_version
                ? 'Main is protected. Propose changes through review.'
                : 'Permitted editors can update main directly.'}
            </p>
            <p class="repo-muted">{project.policy.required_approvals} required approvals</p>
          </section>
          <p class="repo-muted repo-small">
            During testing, each uploaded file can be at most 3 MB (3,000,000 bytes).
          </p>
        </aside>
      </div>
    {:else if !error}
      <div class="repo-skel-stack" aria-busy="true">
        {#each [0, 1, 2, 3, 4, 5] as n (n)}<div class="repo-skel-row" aria-hidden="true">
            <span class="repo-skel" style:width="{30 + ((n * 7) % 40)}%"></span>
          </div>{/each}
      </div>
    {/if}
  </div>
</RepositoryShell>
<div class="repo-tokens">
  {#if review}<ChangeRequestReview
      documentId={review.request.document.id}
      requestId={review.request.id}
      team={review.doc.state}
      teamSeq={review.doc.latest_seq}
      canMerge={review.canMerge}
      canComment={review.canComment}
      onClose={() => {
        review = null;
      }}
      onChanged={() => {
        const reviewed = review?.request.id;
        review = null;
        void run(async () => {
          await loadTab();
          // Show the request where it now lives, so a merge is visible at once.
          const moved = requests.find((r) => r.id === reviewed);
          if (moved)
            prFilter =
              moved.status === 'merged' ? 'merged' : moved.status === 'open' ? 'open' : 'declined';
        });
      }}
      onOpenDraft={(draft) => {
        void goto(`/documents/${review?.request.document.id}?draft=${draft}`);
      }}
    />{/if}
  {#if push}<PushFileDialog
      documentId={push.id}
      title={push.title}
      kind={push.kind as 'document' | 'spreadsheet' | 'presentation'}
      onClose={() => {
        push = null;
      }}
      onCreated={() => {
        push = null;
        notice = 'Changes saved for review';
        void run(loadTab);
      }}
    />{/if}
</div>
