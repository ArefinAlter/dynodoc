<script lang="ts">
  import { onMount } from 'svelte';
  import MoveProjectFile from './MoveProjectFile.svelte';
  import RepositoryShell from '$lib/components/product/RepositoryShell.svelte';
  import {
    FolderGit2,
    FileText,
    GitBranch,
    GitPullRequest,
    Settings2,
    LockKeyhole,
    Upload,
    PanelsTopLeft
  } from '@lucide/svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { workspaceRequest, saveBatch } from '$lib/workspace/api';
  import { createDocument } from '$lib/api/client';
  import type { DocumentDetail } from '$lib/api/types';
  import { importFile } from '$lib/workspace/files';
  import { people, inviteByEmail, invitationNotice } from '$lib/workspace/collaboration';
  import {
    projectRoleName,
    downloadJson,
    type Project,
    type ProjectFile,
    type ProjectRequest
  } from '$lib/workspace/projects';
  import ProvenancePanel from '$lib/components/product/ProvenancePanel.svelte';
  import ChangeRequestReview from '$lib/components/workspace/ChangeRequestReview.svelte';
  import PushFileDialog from '$lib/components/workspace/PushFileDialog.svelte';
  const tabs = [
    ['files', 'Files', FileText],
    ['requests', 'Pull requests', GitPullRequest],
    ['drafts', 'Your drafts', GitBranch],
    ['editors', 'Web editors', PanelsTopLeft],
    ['settings', 'Settings', Settings2]
  ] as const;
  let project = $state<Project | null>(null),
    tab = $state('files'),
    search = $state(''),
    files = $state<ProjectFile[]>([]),
    requests = $state<ProjectRequest[]>([]),
    drafts = $state<ProjectRequest[]>([]),
    members = $state<{ id: string; name: string | null; email: string; role: string }[]>([]);
  let error = $state(''),
    notice = $state(''),
    busy = $state(false),
    offset = $state(0),
    more = $state(false),
    email = $state(''),
    role = $state('reviewer'),
    notify = $state(true),
    note = $state('');
  let sync = $state<ProjectFile | null>(null),
    push = $state<ProjectFile | null>(null),
    review = $state<{
      request: ProjectRequest;
      doc: DocumentDetail;
      canMerge: boolean;
      canComment: boolean;
    } | null>(null);
  const id = $derived($page.params.id || ''),
    manage = $derived(project && ['owner', 'manager'].includes(project.role)),
    canImport = $derived(project && ['owner', 'manager', 'editor'].includes(project.role));
  async function load() {
    project = await workspaceRequest<Project>(`/projects/${id}`);
    await loadTab();
  }
  async function loadTab() {
    const path = tab === 'requests' ? 'change-requests' : tab === 'drafts' ? 'drafts' : 'files';
    if (tab === 'settings') {
      members = (
        await workspaceRequest<{
          items: { id: string; name: string | null; email: string; role: string }[];
        }>(`/spaces/${id}/members`)
      ).items;
      return;
    }
    const result = await workspaceRequest<{
      items: ProjectFile[] | ProjectRequest[];
      more: boolean;
    }>(`/projects/${id}/${path}?offset=${offset}`);
    more = result.more;
    if (tab === 'files' || tab === 'editors') files = result.items as ProjectFile[];
    else if (tab === 'requests') requests = result.items as ProjectRequest[];
    else drafts = result.items as ProjectRequest[];
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
    if (requested && ['files', 'requests', 'drafts', 'settings', 'editors'].includes(requested))
      tab = requested;
    void run(load);
  });
  async function select(value: string) {
    tab = value;
    await goto(`/projects/${id}?tab=${value}`, { replaceState: true, noScroll: true });
    offset = 0;
    sync = null;
    await loadTab();
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
  async function openReview(r: ProjectRequest) {
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
  function draftUrl(d: ProjectRequest) {
    return `/projects/${id}/files/${d.document.id}?draft=${d.id}`;
  }
  const visibleFiles = $derived(
    files.filter((f) => f.title.toLowerCase().includes(search.toLowerCase()))
  );
</script>

<svelte:head
  ><title>{project?.name || 'Project'} · Dynodoc</title><meta
    name="robots"
    content="noindex"
  /></svelte:head
>
<RepositoryShell>
  <div class="repo-title">
    <FolderGit2 size={24} /><a href="/projects">Projects</a><span class="repo-muted">/</span>
    <h1>{project?.name || 'Loading project...'}</h1>
    <span class="repo-badge"><LockKeyhole size={12} /> Private</span>
  </div>
  {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}{#if notice}<p
      class="repo-notice"
      role="status"
    >
      {notice}
    </p>{/if}
  {#if project}
    <nav class="repo-tabs" aria-label="Project sections">
      {#each tabs as [key, label, Icon]}
        <button
          aria-current={tab === key ? 'page' : undefined}
          disabled={busy}
          onclick={() => run(() => select(key as string))}><Icon size={16} />{label}</button
        >
      {/each}
    </nav>
    <div class="repo-columns">
      <div>
        {#if tab === 'files' || tab === 'editors'}
          {#if tab === 'editors'}<h2>Web editors</h2>
            <p class="repo-muted">
              Open a file in the built-in editor. Use Files, pull requests and history to manage the
              team's shared work.
            </p>{/if}
          <div class="repo-toolbar">
            <button onclick={() => run(() => select('drafts'))}
              ><GitBranch size={16} /> main · Your drafts</button
            >
            <input
              type="search"
              aria-label="Find a file"
              placeholder="Find a file on this page..."
              bind:value={search}
            />
            {#if canImport}<label class="upload-control"
                ><span><Upload size={15} /> Add a file</span><input
                  type="file"
                  disabled={busy}
                  accept=".docx,.doc,.xlsx,.csv,.pptx,.txt,.md,.tex,.zip"
                  onchange={(e) => {
                    const f = e.currentTarget.files?.[0];
                    e.currentTarget.value = '';
                    void run(() => importNew(f));
                  }}
                /></label
              >{/if}
          </div>
          <div class="repo-panel">
            <div class="repo-panel-head">
              <GitBranch size={16} /><strong>main</strong><span class="repo-muted"
                >The current shared versions</span
              ><span class="repo-badge">{files.length}{more ? '+' : ''} files on this page</span>
            </div>
            {#if !visibleFiles.length}<div class="repo-empty">
                <FileText size={30} />
                <h2>{files.length ? 'No matching files' : 'Add your first file'}</h2>
                <p>
                  Upload a document, workbook or presentation up to 3 MB. Your team can then propose
                  and review changes.
                </p>
              </div>{:else}
              <table class="repo-table">
                <thead
                  ><tr
                    ><th>File</th><th class="repo-optional">Format</th><th>Main revision</th><th
                      class="repo-optional">Updated</th
                    ><th><span class="repo-small">Actions</span></th></tr
                  ></thead
                ><tbody>
                  {#each visibleFiles as f}<tr>
                      <td
                        ><div class="repo-file-name">
                          <FileText size={17} /><a
                            href={tab === 'editors'
                              ? `/documents/${f.id}`
                              : `/projects/${id}/files/${f.id}`}>{f.title}</a
                          >
                        </div></td
                      >
                      <td class="repo-optional repo-muted">{f.kind}</td><td
                        ><a href={`/projects/${id}/files/${f.id}?tab=history`}>r{f.latest_seq}</a
                        ></td
                      ><td class="repo-optional repo-muted"
                        ><time datetime={f.updated_at}
                          >{new Date(f.updated_at).toLocaleDateString()}</time
                        ></td
                      >
                      <td
                        ><div class="repo-actions">
                          <button
                            aria-label={`Sync ${f.title}`}
                            disabled={busy}
                            onclick={() => (sync = f)}>Sync</button
                          >{#if ['document', 'spreadsheet', 'presentation'].includes(f.kind) && f.role !== 'viewer'}<button
                              aria-label={`Push ${f.title}`}
                              disabled={busy}
                              onclick={() => (push = f)}>Push</button
                            >{/if}
                        </div></td
                      >
                    </tr>{/each}</tbody
                >
              </table>
            {/if}
          </div>
          {#if canImport}<MoveProjectFile
              projectId={id}
              onMoved={async () => {
                await loadTab();
                notice = 'File moved into this project';
              }}
            />{/if}
          {#if sync}<section class="repo-panel">
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
        {:else if tab === 'requests'}
          <h2>Pull requests</h2>
          <p class="repo-muted">Review proposed file changes and merge approved work into main.</p>
          {#if !requests.length}<p>No submitted requests on this page.</p>{/if}
          {#each requests as r}<article class="repo-row">
              <strong>{r.name}</strong><span
                >{r.document.title} · {r.status} · {r.author?.name || 'Project member'}</span
              ><button disabled={busy} onclick={() => run(() => openReview(r))}
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
            </article>{/each}
        {:else if tab === 'drafts'}
          <h2>Your drafts</h2>
          <p>
            Each draft is a private branch of one file. Compare it with main, edit it and open a
            pull request when ready. Project-wide branches are not available yet.
          </p>
          {#if !drafts.length}<p>No private drafts on this page.</p>{/if}
          {#each drafts as d}<article class="repo-row">
              <a href={draftUrl(d)}>{d.name}</a><span
                >{d.document.title} · revision {d.revision}</span
              >
            </article>{/each}
        {:else}
          <h2>Project settings</h2>
          <p>Files inherit these review rules unless they have their own override.</p>
          <form
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
                ><option value="editors">Editors, managers and owners</option><option value="owners"
                  >Managers and owners</option
                ></select
              ></label
            >
            {#if manage}<button disabled={busy}>Save settings</button>{/if}
          </form>
          <h3>People</h3>
          {#each members as m}<p>
              <a href={`/profiles/${m.id}`}>{m.name || m.email}</a> · {projectRoleName(
                m.role
              )}{#if manage && m.role !== 'owner'}<select
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
            </p>{/each}
          {#if manage}<form
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
              ><button disabled={busy}>Add person</button>
            </form>{/if}
        {/if}
        {#if tab !== 'settings'}<footer class="repo-pagination">
            <button
              disabled={busy || offset === 0}
              onclick={() =>
                run(async () => {
                  offset = Math.max(0, offset - 100);
                  await loadTab();
                })}>Previous page</button
            ><button
              disabled={busy || !more}
              onclick={() =>
                run(async () => {
                  offset += 100;
                  await loadTab();
                })}>Next page</button
            >
          </footer>{/if}
      </div>
      <aside class="repo-aside">
        <section>
          <h2>About</h2>
          <p>{project.description || 'A shared home for team documents and changes.'}</p>
          <p class="repo-muted"><LockKeyhole size={14} /> Invite-only project</p>
          <p class="repo-muted">Your role: {projectRoleName(project.role)}</p>
        </section>
        <section>
          <h3>Working together</h3>
          <p class="repo-muted">
            Edit in Word or Google Docs, Excel or Sheets, PowerPoint or Slides. Push changes here
            for review.
          </p>
          <a href="/connectors">Install an editor connector</a>
        </section>
        <section>
          <h3>Review rules</h3>
          <p class="repo-muted">
            {project.policy.protect_team_version
              ? 'Main is protected. Propose changes through review.'
              : 'Permitted editors can update main directly.'}
          </p>
          <p class="repo-muted">{project.policy.required_approvals} required approvals</p>
          <label
            >Notifications<select
              value={project.watch}
              disabled={busy}
              onchange={(e) => {
                const level = e.currentTarget.value;
                void run(async () => {
                  await workspaceRequest(`/projects/${id}/watch`, { level });
                  await load();
                });
              }}
              ><option value="ignore">Standard role notifications</option><option value="requests"
                >Watch pull requests</option
              ></select
            ></label
          >
        </section>
        <p class="repo-muted repo-small">
          During testing, each uploaded file can be at most 3 MB (3,000,000 bytes).
        </p>
      </aside>
    </div>
  {/if}
</RepositoryShell>
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
      review = null;
      void run(loadTab);
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

<style>
  .upload-control {
    position: relative;
    overflow: hidden;
    cursor: pointer;
    border: 1px solid var(--color-border-strong);
    border-radius: 6px;
    padding: 8px 12px;
    background: var(--color-surface);
  }
  .upload-control span {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .upload-control input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
  }
  .upload-control:focus-within {
    outline: 2px solid var(--workspace-accent);
    outline-offset: 3px;
  }
</style>
