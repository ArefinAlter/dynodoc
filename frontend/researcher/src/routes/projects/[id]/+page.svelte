<script lang="ts">
  import { onMount } from 'svelte';
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
  let project = $state<Project | null>(null),
    tab = $state('files'),
    files = $state<ProjectFile[]>([]),
    requests = $state<ProjectRequest[]>([]),
    drafts = $state<ProjectRequest[]>([]),
    members = $state<{ email: string; role: string }[]>([]);
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
        await workspaceRequest<{ items: { email: string; role: string }[] }>(
          `/spaces/${id}/members`
        )
      ).items;
      return;
    }
    const result = await workspaceRequest<{
      items: ProjectFile[] | ProjectRequest[];
      more: boolean;
    }>(`/projects/${id}/${path}?offset=${offset}`);
    more = result.more;
    if (tab === 'files') files = result.items as ProjectFile[];
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
    void run(load);
  });
  async function select(value: string) {
    tab = value;
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
        `Import stopped. The new file is recoverable in Home (${doc.id}). ${e instanceof Error ? e.message : ''}`
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
    return `/documents/${d.document.id}?draft=${d.id}`;
  }
</script>

<svelte:head
  ><title>{project?.name || 'Project'} · Dynodoc</title><meta
    name="robots"
    content="noindex"
  /></svelte:head
>
<main>
  <header>
    <a href="/projects">← Projects</a><a href="/workspace">Home</a>
    <h1>{project?.name || 'Loading project…'}</h1>
    <p>{project?.description}</p>
  </header>
  {#if error}<p role="alert">{error}</p>{/if}{#if notice}<p role="status">{notice}</p>{/if}
  {#if project}
    <p>
      Your role: {projectRoleName(project.role)}.
      <label
        >New-request notifications<select
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
            >Watch new requests</option
          ></select
        ></label
      >
    </p>
    <nav aria-label="Project sections">
      {#each [['files', 'Files'], ['requests', 'Change requests'], ['drafts', 'Your drafts'], ['settings', 'Settings']] as [key, label]}<button
          aria-current={tab === key ? 'page' : undefined}
          disabled={busy}
          onclick={() => run(() => select(key))}>{label}</button
        >{/each}
    </nav>
    {#if tab === 'files'}
      <h2>Files</h2>
      <p>
        Files and block changes share one review history. Filename and content matching help
        recognise copies; they do not establish authorship.
      </p>
      {#if canImport}<label
          >Add a file<input
            type="file"
            disabled={busy}
            accept=".docx,.doc,.xlsx,.csv,.pptx,.tex,.zip"
            onchange={(e) => {
              const f = e.currentTarget.files?.[0];
              e.currentTarget.value = '';
              void run(() => importNew(f));
            }}
          /></label
        >
        <details>
          <summary>Move an existing file here</summary>
          <form
            onsubmit={(e) => {
              e.preventDefault();
              const doc = new FormData(e.currentTarget).get('document');
              void run(async () => {
                await workspaceRequest(`/documents/${doc}/location`, { space_id: id });
                await loadTab();
              });
            }}
          >
            <label>File ID<input name="document" required pattern="[0-9a-fA-F-]{36}" /></label
            ><button disabled={busy}>Move file</button>
          </form>
          <p>Moving requires permission to manage that file. Its existing direct access remains.</p>
        </details>{/if}
      {#if !files.length}<p>No files on this page.</p>{/if}
      {#each files as f}<article>
          <strong>{f.title}</strong><span>{f.kind} · revision {f.latest_seq}</span><button
            disabled={busy}
            onclick={() => {
              sync = f;
            }}>Sync & connections</button
          >{#if ['document', 'spreadsheet', 'presentation'].includes(f.kind) && f.role !== 'viewer'}<button
              disabled={busy}
              onclick={() => {
                push = f;
              }}>Push edited file</button
            >{/if}<a href={`/documents/${f.id}`}>Open workspace</a>
        </article>{/each}
      {#if sync}{#key sync.id}<ProvenancePanel
            documentId={sync.id}
            title={sync.title}
            canPropose={sync.role !== 'viewer'}
            onChanged={() => {
              void run(loadTab);
            }}
          />{/key}{/if}
    {:else if tab === 'requests'}
      <h2>Change requests</h2>
      {#if !requests.length}<p>No submitted requests on this page.</p>{/if}
      {#each requests as r}<article>
          <strong>{r.name}</strong><span>{r.document.title} · {r.status} · {r.author?.email}</span
          ><button disabled={busy} onclick={() => run(() => openReview(r))}>Review changes</button
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
      <p>Only your unsubmitted drafts appear here, including when you manage the project.</p>
      {#if !drafts.length}<p>No private drafts on this page.</p>{/if}
      {#each drafts as d}<article>
          <a href={draftUrl(d)}>{d.name}</a><span>{d.document.title} · revision {d.revision}</span>
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
          >Who may merge<select bind:value={project.policy.merge_roles} disabled={!manage || busy}
            ><option value="editors">Editors, managers and owners</option><option value="owners"
              >Managers and owners</option
            ></select
          ></label
        >
        {#if manage}<button disabled={busy}>Save settings</button>{/if}
      </form>
      <h3>People</h3>
      {#each members as m}<p>
          {m.email} · {projectRoleName(m.role)}{#if manage && m.role !== 'owner'}<select
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
              ><option value="manager">Manager</option><option value="editor">Editor</option><option
                value="approver">Reviewer</option
              ><option value="reviewer">Contributor</option><option value="viewer">Viewer</option
              ><option value="remove">Remove access</option></select
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
              ><option value="manager">Manager</option><option value="editor">Editor</option><option
                value="approver">Reviewer</option
              ><option value="reviewer">Contributor</option><option value="viewer">Viewer</option
              ></select
            ></label
          ><label
            ><input type="checkbox" bind:checked={notify} /> Send an email invitation with a link</label
          ><label>Short message<textarea bind:value={note} maxlength="500"></textarea></label
          ><button disabled={busy}>Add person</button>
        </form>{/if}
    {/if}
    {#if tab !== 'settings'}<footer>
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
  {/if}
</main>
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
  main {
    max-width: 1200px;
    margin: auto;
    padding: 28px;
    color: var(--color-ink);
  }
  header > a {
    margin-right: 20px;
  }
  nav,
  footer {
    display: flex;
    gap: 8px;
    margin: 20px 0;
    flex-wrap: wrap;
  }
  button,
  input,
  select,
  textarea {
    font: inherit;
    color: inherit;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    padding: 9px;
    border-radius: 6px;
  }
  button {
    cursor: pointer;
  }
  button[aria-current='page'] {
    border-color: var(--workspace-accent);
    font-weight: 600;
  }
  label {
    display: block;
    margin: 12px 0;
  }
  label > input:not([type='checkbox']),
  label > textarea {
    display: block;
    max-width: 100%;
    width: 400px;
    margin-top: 6px;
  }
  article {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
    padding: 18px 0;
    border-bottom: 1px solid var(--color-border);
  }
  article strong {
    min-width: 180px;
  }
  span {
    color: var(--color-ink-muted);
  }
  a {
    color: var(--workspace-accent);
  }
  form,
  details {
    padding: 16px 0;
  }
  p[role='alert'] {
    color: var(--color-error);
  }
</style>
