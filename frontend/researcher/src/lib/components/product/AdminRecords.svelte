<script lang="ts">
  import { onMount } from 'svelte';
  import { invalidateAll } from '$app/navigation';
  import {
    Search,
    Settings2,
    Download,
    X,
    TriangleAlert,
    CircleAlert,
    ShieldCheck,
    ArrowRight,
    ArrowLeft,
    Trash2,
    Ellipsis,
    ChevronRight,
    SearchX
  } from '@lucide/svelte';
  import { workspaceRequest as request } from '$lib/workspace/api';
  import CopyButton from '$lib/components/admin/CopyButton.svelte';
  import RelativeTime from '$lib/components/admin/RelativeTime.svelte';
  import TableScroll from '$lib/components/admin/TableScroll.svelte';
  import { formatBytes } from '$lib/components/admin/health';
  import '$lib/components/admin/admin.css';
  type RecordRow = {
    id: string;
    label: string;
    name?: string | null;
    owner?: string | null;
    actor?: string | null;
    resource_id?: string | null;
    created_at: string;
    updated_at?: string | null;
    removed_at?: string | null;
    kind?: string;
    /** Optional structured context an audit entry may carry. */
    detail?: unknown;
    members?: number | null;
    files?: number | null;
    bytes?: number | null;
    last_activity_at?: string | null;
  };
  type Impact = {
    revision: string;
    counts: Record<string, number>;
    blockers: string[];
    in_trash?: boolean | null;
  };
  let { onkind }: { onkind?: (kind: string) => void } = $props();
  const PAGE = 50;
  // Documents come first: permanent deletion is the most common administrative task.
  let kind = $state('documents'),
    status = $state(''),
    q = $state(''),
    offset = $state(0);
  // Offered only when the engine answers the project listing.
  let projectsAvailable = $state(false);
  const kinds = $derived([
    { value: 'documents', label: 'Documents' },
    { value: 'users', label: 'Accounts' },
    ...(projectsAvailable ? [{ value: 'projects', label: 'Projects' }] : []),
    { value: 'spaces', label: 'Organizations & folders' },
    { value: 'audit', label: 'Audit log' },
    { value: 'erasures', label: 'Deletion receipts' }
  ]);
  const placeholders: Record<string, string> = {
    documents: 'Document title',
    users: 'Email or name',
    projects: 'Project name',
    spaces: 'Organization or folder name',
    audit: 'Action, for example account.removed',
    erasures: 'Kind or reason'
  };
  // Documents ticked for deletion, kept across pages until the list changes.
  let picked = $state<RecordRow[]>([]),
    bulkConfirmation = $state(''),
    bulkReason = $state('duplicate'),
    bulkAcknowledged = $state(false),
    bulkResult = $state(''),
    bulkDialog: HTMLDialogElement;
  const bulkPhrase = $derived(`DELETE ${picked.length} DOCUMENT${picked.length === 1 ? '' : 'S'}`);
  let items = $state<RecordRow[]>([]),
    busy = $state(false),
    loaded = $state(false),
    error = $state('');
  let selected = $state<RecordRow | null>(null),
    confirmation = $state('');
  let action = $state('lifecycle'),
    impact = $state<Impact | null>(null),
    previewing = $state(false);
  let reason = $state('owner_request'),
    acknowledged = $state(false);
  let recipient = $state(''),
    recipientSearch = $state(''),
    recipients = $state<RecordRow[]>([]);
  let dialog: HTMLDialogElement;
  let root: HTMLElement;
  let sequence = 0,
    listSequence = 0;
  // Audit filters: actor and dates are also sent to the engine (older engines ignore them),
  // and every filter is applied again to the loaded page, so the view is correct either way.
  let auditActions = $state<string[]>([]),
    auditActor = $state(''),
    auditFrom = $state(''),
    auditTo = $state(''),
    knownActors = $state<string[]>([]);
  let expanded = $state<string[]>([]);
  const readOnly = $derived(kind === 'audit' || kind === 'erasures' || kind === 'projects');
  // Receipts and project totals are read-only, so they have no action column.
  const showActions = $derived(kind !== 'erasures' && kind !== 'projects');
  // Dates are the administrator's local days; the engine receives them as exact instants.
  const dayStart = (value: string) => new Date(`${value}T00:00:00`).getTime();
  const dayEnd = (value: string) => {
    const end = new Date(dayStart(value));
    end.setDate(end.getDate() + 1);
    return end.getTime();
  };
  const shown = $derived(
    kind !== 'audit'
      ? items
      : items.filter((row) => {
          if (auditActions.length && !auditActions.includes(row.label)) return false;
          if (auditActor && row.actor !== auditActor) return false;
          const at = Date.parse(row.created_at);
          if (auditFrom && at < dayStart(auditFrom)) return false;
          if (auditTo && at >= dayEnd(auditTo)) return false;
          return true;
        })
  );
  const actionCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const row of items) counts.set(row.label, (counts.get(row.label) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  });
  const auditFiltered = $derived(!!(auditActions.length || auditActor || auditFrom || auditTo));
  const columns = $derived(
    { documents: 7, users: 4, projects: 6, spaces: 4, audit: 5, erasures: 4 }[kind] ?? 5
  );
  const noun = $derived(
    {
      documents: 'document',
      users: 'account',
      projects: 'project',
      spaces: 'organization or folder',
      audit: 'event',
      erasures: 'receipt'
    }[kind] ?? 'record'
  );
  const actionTitle = $derived(
    action === 'erase'
      ? 'Permanently delete'
      : action === 'revoke_sessions'
        ? 'Sign out all sessions'
        : action === 'revoke_public_links'
          ? 'Revoke every public link'
          : action.startsWith('transfer_')
            ? 'Transfer ownership'
            : selected?.removed_at
              ? 'Restore access'
              : kind === 'users'
                ? 'Disable account'
                : 'Move to Trash'
  );
  async function refresh() {
    const current = ++listSequence;
    busy = true;
    error = '';
    try {
      const result = (
        await request<{ items: RecordRow[] }>(
          `/admin/records?${new URLSearchParams({
            kind,
            q,
            offset: String(offset),
            ...(kind === 'documents' ? { status } : {}),
            ...(kind === 'audit' && auditActor ? { actor: auditActor } : {}),
            ...(kind === 'audit' && auditFrom
              ? { since: new Date(dayStart(auditFrom)).toISOString() }
              : {}),
            ...(kind === 'audit' && auditTo
              ? { until: new Date(dayEnd(auditTo)).toISOString() }
              : {})
          })}`
        )
      ).items;
      if (current === listSequence) {
        items = result;
        expanded = [];
        if (kind === 'audit')
          knownActors = [
            ...new Set([
              ...knownActors,
              ...result.map((r) => r.actor).filter((a): a is string => !!a)
            ])
          ].sort();
      }
    } catch (e) {
      if (current !== listSequence) return;
      error = e instanceof Error ? e.message : 'Could not load records';
    } finally {
      if (current === listSequence) {
        busy = false;
        loaded = true;
      }
    }
  }
  async function probeProjects() {
    try {
      await request(`/admin/records?${new URLSearchParams({ kind: 'projects', q: '' })}`);
      projectsAvailable = true;
    } catch {
      projectsAvailable = false;
    }
  }
  onMount(() => {
    void refresh().then(probeProjects);
  });
  function changeKind() {
    offset = 0;
    picked = [];
    expanded = [];
    auditActions = [];
    auditActor = '';
    auditFrom = '';
    auditTo = '';
    knownActors = [];
    onkind?.(kind);
    void refresh();
  }
  /** Switch the listing from outside (navigation, danger zone). Unknown kinds fall back safely. */
  export function show(next: string) {
    kind = kinds.some((k) => k.value === next) ? next : 'spaces';
    q = '';
    changeKind();
  }
  function open(row: RecordRow, preset = '') {
    closeMenus();
    selected = row;
    action = preset || (kind === 'spaces' ? 'transfer_workspace' : 'lifecycle');
    confirmation = '';
    error = '';
    impact = null;
    acknowledged = false;
    recipient = '';
    recipients = [];
    dialog.showModal();
    if (preset === 'erase') void changeAction();
  }
  function toggle(row: RecordRow, on: boolean) {
    picked = on
      ? [...picked.filter((p) => p.id !== row.id), row]
      : picked.filter((p) => p.id !== row.id);
  }
  function openBulk() {
    bulkConfirmation = '';
    bulkAcknowledged = false;
    bulkResult = '';
    error = '';
    bulkDialog.showModal();
  }
  async function eraseBulk() {
    if (busy || bulkConfirmation !== bulkPhrase || !bulkAcknowledged) return;
    busy = true;
    error = '';
    try {
      const result = await request<{
        erased: number;
        results: { id: string; ok: boolean; error?: string }[];
      }>('/admin/erase-batch', {
        ids: picked.map((p) => p.id),
        confirmation: bulkConfirmation,
        reason: bulkReason,
        acknowledge_backups: bulkAcknowledged
      });
      const failed = result.results.filter((r) => !r.ok);
      bulkResult = `Permanently deleted ${result.erased} document${result.erased === 1 ? '' : 's'}.${failed.length ? ` ${failed.length} could not be deleted.` : ''}`;
      picked = picked.filter((p) => failed.some((f) => f.id === p.id));
      await refresh();
      await invalidateAll();
      if (!failed.length) bulkDialog.close();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not delete these documents';
    } finally {
      busy = false;
    }
  }
  async function changeAction() {
    const current = ++sequence;
    confirmation = '';
    error = '';
    acknowledged = false;
    impact = null;
    previewing = false;
    if (action !== 'erase' || !selected) return;
    previewing = true;
    try {
      const result = await request<Impact>(`/admin/impact?kind=${kind}&id=${selected.id}`);
      if (current === sequence) impact = result;
    } catch (e) {
      if (current === sequence)
        error = e instanceof Error ? e.message : 'Could not load deletion impact';
    } finally {
      if (current === sequence) previewing = false;
    }
  }
  async function findRecipient() {
    try {
      recipients = (
        await request<{ items: RecordRow[] }>(
          `/admin/records?kind=users&q=${encodeURIComponent(recipientSearch)}`
        )
      ).items.filter((r) => !r.removed_at);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not find accounts';
    }
  }
  async function update() {
    if (!selected || busy) return;
    busy = true;
    error = '';
    try {
      if (action === 'erase') {
        if (!impact || impact.blockers.length || !acknowledged) return;
        await request('/admin/erase', {
          kind,
          id: selected.id,
          confirmation,
          revision: impact.revision,
          reason,
          acknowledge_backups: acknowledged
        });
      } else if (action === 'lifecycle') {
        await request('/admin/actions', {
          kind,
          id: selected.id,
          removed: !selected.removed_at,
          confirmation
        });
      } else {
        await request('/admin/control', {
          action,
          id: selected.id,
          confirmation,
          recipient: recipient || null
        });
      }
      dialog.close();
      await refresh();
      await invalidateAll();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not update record';
    } finally {
      busy = false;
    }
  }
  function exportPage() {
    const base = [
      'id',
      'label',
      'name',
      'owner',
      'actor',
      'resource_id',
      'created_at',
      'removed_at'
    ] as const;
    const extra = [
      'updated_at',
      'members',
      'files',
      'bytes',
      'last_activity_at',
      'detail'
    ] as const;
    // Newer columns are written only when the API supplied them.
    const fields = [...base, ...extra.filter((f) => shown.some((row) => row[f] != null))];
    const cell = (v: unknown) => {
      let text = typeof v === 'object' && v !== null ? JSON.stringify(v) : String(v ?? '');
      if (/^[\s]*[=+\-@]/.test(text)) text = "'" + text;
      return '"' + text.replaceAll('"', '""') + '"';
    };
    const csv = [
      fields.join(','),
      ...shown.map((row) => fields.map((f) => cell(row[f])).join(','))
    ].join('\r\n');
    const href = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }));
    const a = document.createElement('a');
    a.href = href;
    a.download = `dynodoc-${kind}-page-${offset / PAGE + 1}.csv`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(href), 1000);
  }
  function detailEntries(row: RecordRow): [string, string][] {
    const d = row.detail;
    if (d == null || d === '') return [];
    if (typeof d === 'object' && !Array.isArray(d))
      return Object.entries(d).map(([k, v]) => [k, typeof v === 'string' ? v : JSON.stringify(v)]);
    return [['detail', typeof d === 'string' ? d : JSON.stringify(d)]];
  }
  const hashLike = (value: string) =>
    /^[0-9a-f]{32,}$/i.test(value) ||
    /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);
  function toggleDetail(id: string) {
    expanded = expanded.includes(id) ? expanded.filter((e) => e !== id) : [...expanded, id];
  }
  function toggleAction(name: string) {
    auditActions = auditActions.includes(name)
      ? auditActions.filter((a) => a !== name)
      : [...auditActions, name];
  }
  /** Actor and date filters change which entries exist, so they are requested again from page one. */
  function applyAudit() {
    offset = 0;
    void refresh();
  }
  function clearAudit() {
    auditActions = [];
    auditActor = '';
    auditFrom = '';
    auditTo = '';
    applyAudit();
  }
  // Row menus are native <details>; they close on outside click, Escape and scroll.
  function closeMenus(except?: Element | null) {
    for (const menu of root?.querySelectorAll('details.kebab[open]') ?? [])
      if (menu !== except) menu.removeAttribute('open');
  }
  /** The panel is fixed-position so the scrolling table cannot clip it. */
  function placeMenu(event: MouseEvent) {
    const summary = event.currentTarget as HTMLElement;
    const rect = summary.getBoundingClientRect();
    const panel = summary.parentElement!.querySelector<HTMLElement>('.menu')!;
    panel.style.top = `${rect.bottom + 4}px`;
    panel.style.right = `${Math.max(8, window.innerWidth - rect.right)}px`;
  }
</script>

<svelte:window
  onclick={(e) => closeMenus((e.target as Element | null)?.closest?.('details.kebab'))}
  onkeydown={(e) => e.key === 'Escape' && closeMenus()}
  onresize={() => closeMenus()}
  onscroll={() => closeMenus()}
/>
<section id="records" class="records" aria-labelledby="records-heading" bind:this={root}>
  <div class="section-head">
    <div>
      <h2 id="records-heading" class="section-title" tabindex="-1">People, content & access</h2>
      <p class="muted">Review access, transfer ownership and manage the full content lifecycle.</p>
    </div>
    <button class="btn" disabled={busy || !shown.length} onclick={exportPage}
      ><Download size={16} aria-hidden="true" /> Export this page</button
    >
  </div>
  <form
    class="toolbar"
    onsubmit={(e) => {
      e.preventDefault();
      offset = 0;
      void refresh();
    }}
  >
    <label class="field"
      >Browse<select class="select" bind:value={kind} onchange={changeKind}
        >{#each kinds as option (option.value)}<option value={option.value}>{option.label}</option
          >{/each}</select
      ></label
    >
    <label class="field grow"
      >Search
      <span class="search"
        ><Search size={16} aria-hidden="true" /><input
          class="input"
          bind:value={q}
          maxlength="200"
          placeholder={placeholders[kind] ?? 'Name, email or action'}
        /></span
      ></label
    ><button class="btn btn-primary" disabled={busy}>Search</button>
  </form>
  {#if error && !selected}<p class="flash flash-error" role="alert">
      <CircleAlert size={16} aria-hidden="true" />
      <span>{error}</span>
      <button type="button" class="btn btn-sm retry" onclick={refresh}>Retry</button>
    </p>{/if}
  <div class="box">
    <div class="box-header">
      {#if kind === 'documents'}
        <div class="seg" role="group" aria-label="Document status">
          {#each [['', 'All documents'], ['active', 'Active'], ['trash', 'In Trash']] as [value, label]}
            <button
              type="button"
              aria-pressed={status === value}
              onclick={() => {
                status = value;
                offset = 0;
                void refresh();
              }}>{label}</button
            >
          {/each}
        </div>
        <div class="bulk">
          {#if picked.length}
            <span class="selected"
              >{picked.length} document{picked.length === 1 ? '' : 's'} selected</span
            ><button type="button" class="btn btn-sm" onclick={() => (picked = [])}>Clear</button
            ><button
              type="button"
              class="btn btn-sm btn-danger-solid"
              disabled={busy}
              onclick={openBulk}><Trash2 size={14} aria-hidden="true" /> Delete permanently…</button
            >
          {:else}<span class="muted">Tick documents to delete several at once.</span>{/if}
        </div>
      {:else}
        <h3 class="box-title">{kinds.find((k) => k.value === kind)?.label}</h3>
        {#if kind === 'audit'}<span class="muted"
            >Entries on this page{auditFiltered
              ? `: ${shown.length} of ${items.length} match`
              : ''}</span
          >{/if}
      {/if}
    </div>
    {#if kind === 'audit' && (items.length || auditFiltered)}
      <div class="box-row audit-filters" role="group" aria-label="Audit filters">
        <div class="chips">
          <span class="muted">Action</span>
          {#each actionCounts as [name, count] (name)}
            <button
              type="button"
              class="chip"
              aria-pressed={auditActions.includes(name)}
              onclick={() => toggleAction(name)}
              ><code>{name}</code><span class="counter">{count}</span></button
            >
          {/each}
        </div>
        <div class="audit-fields">
          <label class="field"
            >Filter by actor<select class="select" bind:value={auditActor} onchange={applyAudit}
              ><option value="">Any actor</option>{#each knownActors as actor}<option value={actor}
                  >{actor}</option
                >{/each}</select
            ></label
          >
          <label class="field"
            >From date<input
              class="input"
              type="date"
              bind:value={auditFrom}
              onchange={applyAudit}
            /></label
          >
          <label class="field"
            >To date<input
              class="input"
              type="date"
              bind:value={auditTo}
              onchange={applyAudit}
            /></label
          >
          {#if auditFiltered}<button type="button" class="btn btn-sm" onclick={clearAudit}
              >Clear filters</button
            >{/if}
        </div>
      </div>
    {/if}
    <TableScroll label="Records table" {busy}>
      <table class:dim={busy && items.length > 0}>
        <thead>
          <tr>
            {#if kind === 'documents'}
              <th scope="col" class="pick"
                ><input
                  type="checkbox"
                  aria-label="Select every document on this page"
                  checked={items.length > 0 &&
                    items.every((r) => picked.some((p) => p.id === r.id))}
                  onchange={(e) => {
                    for (const row of items) toggle(row, e.currentTarget.checked);
                  }}
                /></th
              ><th scope="col">Document</th><th scope="col">Owner</th><th scope="col">Created</th
              ><th scope="col">Updated</th><th scope="col">Status</th>
            {:else if kind === 'users'}
              <th scope="col">Account</th><th scope="col">Created</th><th scope="col">Status</th>
            {:else if kind === 'projects'}
              <th scope="col">Project</th><th scope="col" class="num">Members</th><th
                scope="col"
                class="num">Files</th
              ><th scope="col" class="num">Size</th><th scope="col">Created</th><th scope="col"
                >Last activity</th
              >
            {:else if kind === 'spaces'}
              <th scope="col">Organization or folder</th><th scope="col">Type</th><th scope="col"
                >Created</th
              >
            {:else if kind === 'audit'}
              <th scope="col">When</th><th scope="col">Action</th><th scope="col">Actor</th><th
                scope="col">Resource</th
              >
            {:else}
              <th scope="col">Erased</th><th scope="col">Receipt</th><th scope="col">Reason</th><th
                scope="col">Resource ID</th
              >
            {/if}
            {#if showActions}<th scope="col" class="actions-head"
                >{#if kind === 'audit'}Details{:else}<span class="sr-only">Actions</span>{/if}</th
              >{/if}
          </tr>
        </thead>
        <tbody>
          {#each shown as row, index}
            <tr>
              {#if kind === 'documents'}
                <td class="pick"
                  ><input
                    type="checkbox"
                    aria-label={`Select ${row.label}`}
                    checked={picked.some((p) => p.id === row.id)}
                    onchange={(e) => toggle(row, e.currentTarget.checked)}
                  /></td
                >
                <td class="ident"
                  ><strong>{row.label}</strong><span class="idline"
                    ><code>{row.id}</code><CopyButton
                      value={row.id}
                      label={`Copy ID ${row.id}`}
                    /></span
                  ></td
                >
                <td>{row.owner || '—'}</td>
                <td><RelativeTime value={row.created_at} /></td>
                <td><RelativeTime value={row.updated_at} fallback="—" /></td>
                <td
                  ><span
                    class="pill"
                    class:pill-success={!row.removed_at}
                    class:pill-attention={!!row.removed_at}
                    >{row.removed_at ? 'In Trash' : 'Active'}</span
                  ></td
                >
              {:else if kind === 'users'}
                <td class="ident"
                  ><strong>{row.label}</strong>{#if row.name}<span class="muted">{row.name}</span
                    >{/if}<span class="idline"
                    ><code>{row.id}</code><CopyButton
                      value={row.id}
                      label={`Copy ID ${row.id}`}
                    /></span
                  ></td
                >
                <td><RelativeTime value={row.created_at} /></td>
                <td
                  ><span
                    class="pill"
                    class:pill-success={!row.removed_at}
                    class:pill-attention={!!row.removed_at}
                    >{row.removed_at ? 'Disabled' : 'Active'}</span
                  ></td
                >
              {:else if kind === 'projects'}
                <td class="ident"
                  ><strong>{row.label}</strong><span class="idline"
                    ><code>{row.id}</code><CopyButton
                      value={row.id}
                      label={`Copy ID ${row.id}`}
                    /></span
                  ></td
                >
                <td class="num">{row.members ?? '—'}</td>
                <td class="num">{row.files ?? '—'}</td>
                <td class="num">{row.bytes != null ? formatBytes(row.bytes) : '—'}</td>
                <td><RelativeTime value={row.created_at} /></td>
                <td><RelativeTime value={row.last_activity_at} fallback="—" /></td>
              {:else if kind === 'spaces'}
                <td class="ident"
                  ><strong>{row.label}</strong><span class="idline"
                    ><code>{row.id}</code><CopyButton
                      value={row.id}
                      label={`Copy ID ${row.id}`}
                    /></span
                  ></td
                >
                <td class="cap">{row.name || '—'}</td>
                <td><RelativeTime value={row.created_at} /></td>
              {:else if kind === 'audit'}
                <td><RelativeTime value={row.created_at} /></td>
                <td><code class="action">{row.label}</code></td>
                <td>{row.actor || '—'}</td>
                <td
                  >{#if row.resource_id}<span class="idline"
                      ><code>{row.resource_id}</code><CopyButton
                        value={row.resource_id}
                        label={`Copy resource ID ${row.resource_id}`}
                      /></span
                    >{:else}—{/if}</td
                >
              {:else}
                <td><RelativeTime value={row.created_at} /></td>
                <td>{row.label}</td>
                <td class="cap">{(row.name || '—').replaceAll('_', ' ')}</td>
                <td
                  ><span class="idline"
                    ><code>{row.id}</code><CopyButton
                      value={row.id}
                      label={`Copy ID ${row.id}`}
                    /></span
                  ></td
                >
              {/if}
              {#if showActions}<td class="actions">
                  {#if kind === 'audit'}
                    {#if detailEntries(row).length}<button
                        type="button"
                        class="btn btn-sm btn-ghost"
                        aria-expanded={expanded.includes(row.id)}
                        aria-controls={`detail-${index}`}
                        onclick={() => toggleDetail(row.id)}
                        ><ChevronRight
                          size={14}
                          class={expanded.includes(row.id) ? 'turn' : ''}
                          aria-hidden="true"
                        />Details</button
                      >{/if}
                  {:else if !readOnly}
                    <div class="row-actions">
                      <button
                        class="btn btn-sm"
                        disabled={busy}
                        onclick={() => open(row)}
                        aria-label={`Manage ${row.label}`}
                        ><Settings2 size={14} aria-hidden="true" /> Manage</button
                      >
                      {#if kind === 'users' || kind === 'documents'}
                        <details class="kebab">
                          <summary
                            class="btn btn-sm btn-icon"
                            aria-label={`More actions for ${row.label}`}
                            onclick={placeMenu}><Ellipsis size={16} aria-hidden="true" /></summary
                          >
                          <div class="menu">
                            {#if kind === 'users'}
                              <button
                                type="button"
                                disabled={busy}
                                onclick={() => open(row, 'revoke_sessions')}
                                >Sign out all sessions</button
                              >
                            {:else}
                              <button
                                type="button"
                                disabled={busy}
                                onclick={() => open(row, 'revoke_public_links')}
                                >Revoke all public links</button
                              ><button
                                type="button"
                                disabled={busy}
                                onclick={() => open(row, 'transfer_document')}
                                >Transfer ownership</button
                              >
                            {/if}
                            <button
                              type="button"
                              class="danger"
                              disabled={busy}
                              onclick={() => open(row, 'erase')}
                              >Permanently delete…<span class="sr-only"> {row.label}</span></button
                            >
                          </div>
                        </details>
                      {/if}
                    </div>
                  {/if}
                </td>{/if}
            </tr>
            {#if kind === 'audit' && expanded.includes(row.id)}
              <tr class="detail-row" id={`detail-${index}`}>
                <td colspan={columns}>
                  <dl class="detail">
                    {#each detailEntries(row) as [key, value]}
                      <div>
                        <dt>{key.replaceAll('_', ' ')}</dt>
                        <dd>
                          <code>{value}</code>{#if hashLike(value)}<CopyButton
                              {value}
                              label={`Copy ${key.replaceAll('_', ' ')}`}
                            />{/if}
                        </dd>
                      </div>
                    {/each}
                  </dl>
                </td>
              </tr>
            {/if}
          {/each}
          {#if !shown.length}
            {#if busy || !loaded}
              {#each [0, 1, 2, 3, 4] as n (n)}
                <tr class="skeleton-row" aria-hidden="true">
                  <td colspan={columns}
                    ><span class="skeleton" style:width="{86 - n * 9}%"></span></td
                  >
                </tr>
              {/each}
              <tr>
                <td colspan={columns} class="sr-only">Loading records…</td>
              </tr>
            {:else}
              <tr>
                <td colspan={columns} class="empty">
                  <SearchX size={28} aria-hidden="true" />
                  <strong
                    >{auditFiltered && items.length
                      ? 'No audit entries match these filters'
                      : q
                        ? `No ${noun}s match “${q}”`
                        : `No ${noun}s to show`}</strong
                  >
                  <span class="muted"
                    >{auditFiltered && items.length
                      ? 'Clear a filter to see the other entries on this page.'
                      : 'No matching records. Try a different search.'}</span
                  >
                </td>
              </tr>
            {/if}
          {/if}
        </tbody>
      </table>
    </TableScroll>
    <noscript
      ><p class="empty">Records load in the browser. Enable JavaScript to browse them.</p></noscript
    >
    <div class="box-footer">
      <span class="muted"
        >{items.length ? `${offset + 1}–${offset + items.length}` : '0 records'}{items.length
          ? ` · page ${offset / PAGE + 1}`
          : ''}</span
      >
      <div class="pager">
        <button
          class="btn btn-sm"
          disabled={busy || offset === 0}
          onclick={() => {
            offset = Math.max(0, offset - PAGE);
            void refresh();
          }}><ArrowLeft size={14} aria-hidden="true" /> Previous</button
        ><button
          class="btn btn-sm"
          disabled={busy || items.length < PAGE}
          onclick={() => {
            offset += PAGE;
            void refresh();
          }}>Next <ArrowRight size={14} aria-hidden="true" /></button
        >
      </div>
    </div>
  </div>
</section>
<dialog
  bind:this={dialog}
  class="dlg"
  aria-labelledby="action-title"
  onclose={() => {
    selected = null;
    ++sequence;
    previewing = false;
  }}
>
  <div class="dlg-head">
    <span class="dlg-icon"><ShieldCheck size={20} aria-hidden="true" /></span>
    <div>
      <p class="eyebrow">{selected?.label}</p>
      <h2 id="action-title">{actionTitle}</h2>
    </div>
    <button
      type="button"
      class="btn btn-ghost btn-icon btn-sm"
      aria-label="Close administration dialog"
      disabled={busy}
      onclick={() => dialog.close()}><X size={18} aria-hidden="true" /></button
    >
  </div>
  <form
    class="dlg-body"
    onsubmit={(e) => {
      e.preventDefault();
      void update();
    }}
  >
    <label class="field"
      >Action<select class="select" bind:value={action} onchange={changeAction} disabled={busy}>
        {#if kind === 'users'}<option value="lifecycle"
            >{selected?.removed_at ? 'Restore account' : 'Disable account'}</option
          ><option value="revoke_sessions">Sign out all sessions</option><option value="erase"
            >Permanently delete account</option
          >
        {:else if kind === 'documents'}<option value="lifecycle"
            >{selected?.removed_at ? 'Restore document' : 'Move document to Trash'}</option
          ><option value="revoke_public_links">Revoke all public links</option><option
            value="transfer_document">Transfer document ownership</option
          ><option value="erase">Permanently delete document</option>
        {:else}<option value="transfer_workspace">Transfer workspace ownership</option>{/if}
      </select></label
    >
    {#if action === 'erase'}
      <div class="flash flash-warn">
        <TriangleAlert size={18} aria-hidden="true" />
        <p>
          {kind === 'documents'
            ? 'Deletes the document, history, files, versions, drafts and public copies from the live database. This cannot be undone.'
            : 'Removes the account profile, credentials, personal drafts and access. Shared contributions retain a deleted-user identity so other documents remain verifiable.'}
        </p>
      </div>
      {#if impact?.in_trash === false}<p class="note">
          This document is not in Trash. Deleting moves it to Trash and removes it permanently in
          one step, and its public links stop working.
        </p>{/if}
      {#if previewing}<p role="status" class="note">Calculating deletion impact…</p>{/if}
      {#if impact}<dl class="impact">
          {#each Object.entries(impact.counts) as [key, value]}<div>
              <dt>{key.replaceAll('_', ' ')}</dt>
              <dd>{value.toLocaleString()}</dd>
            </div>{/each}
        </dl>
        {#each impact.blockers as blocker}<p class="flash flash-error">
            <CircleAlert size={16} aria-hidden="true" />{blocker}
          </p>{/each}{/if}
      <label class="field"
        >Reason<select class="select" bind:value={reason}
          ><option value="owner_request">Owner request</option><option value="duplicate"
            >Duplicate content or account</option
          ><option value="policy">Retention or policy decision</option><option value="other"
            >Other administrative decision</option
          ></select
        ></label
      >
      <label class="ack"
        ><input type="checkbox" bind:checked={acknowledged} /> I understand existing backups may retain
        this data until they expire. A minimal deletion receipt remains.</label
      >
    {:else if action.startsWith('transfer_')}
      <p class="note">
        Give an active account ownership. Existing collaborators keep their access. Workspace owners
        become managers when a replacement owner is selected.
      </p>
      <label class="field"
        >Find the new owner
        <span class="recipient-search">
          <input
            class="input"
            bind:value={recipientSearch}
            placeholder="Search account email"
          /><button class="btn" type="button" onclick={findRecipient}>Find</button>
        </span></label
      >
      <label class="field"
        >New owner<select class="select" bind:value={recipient} required
          ><option value="">Choose an active account</option>{#each recipients as person}<option
              value={person.id}>{person.label}</option
            >{/each}</select
        ></label
      >
    {:else}<p class="note">
        {action === 'revoke_sessions'
          ? 'All existing sessions and pending sign-in links stop working, and the account’s enrolled device signing keys are revoked so command-line sessions cannot be renewed. The account can sign in again; keys must be enrolled again.'
          : action === 'revoke_public_links'
            ? 'Every published link to this document stops working. Previously downloaded copies cannot be recalled.'
            : kind === 'users'
              ? 'Disabling blocks existing sessions and revokes published links. Restoring requires a fresh sign-in.'
              : 'Trash keeps content and history, and revokes public links. Restoring never reactivates old public links.'}
      </p>{/if}
    <label class="field"
      >Type “{selected?.label}” to confirm<input
        class="input"
        bind:value={confirmation}
        autocomplete="off"
        required
      /></label
    >
    {#if error}<p class="flash flash-error" role="alert">
        <CircleAlert size={16} aria-hidden="true" />{error}
      </p>{/if}
    <div class="dlg-foot">
      <button class="btn" type="button" disabled={busy} onclick={() => dialog.close()}
        >Cancel</button
      ><button
        class="btn"
        class:btn-danger-solid={action === 'erase'}
        class:btn-primary={action !== 'erase'}
        disabled={busy ||
          confirmation !== selected?.label ||
          (action === 'erase' && (!impact || impact.blockers.length > 0 || !acknowledged)) ||
          (action.startsWith('transfer_') && !recipient)}>{busy ? 'Applying…' : actionTitle}</button
      >
    </div>
  </form>
</dialog>
<dialog bind:this={bulkDialog} class="dlg" aria-labelledby="bulk-title">
  <div class="dlg-head">
    <span class="dlg-icon danger"><Trash2 size={20} aria-hidden="true" /></span>
    <div>
      <p class="eyebrow">{picked.length} selected</p>
      <h2 id="bulk-title">Permanently delete documents</h2>
    </div>
    <button
      type="button"
      class="btn btn-ghost btn-icon btn-sm"
      aria-label="Close deletion dialog"
      disabled={busy}
      onclick={() => bulkDialog.close()}><X size={18} aria-hidden="true" /></button
    >
  </div>
  <form
    class="dlg-body"
    onsubmit={(e) => {
      e.preventDefault();
      void eraseBulk();
    }}
  >
    <div class="flash flash-warn">
      <TriangleAlert size={18} aria-hidden="true" />
      <p>
        Each document, with its history, files, versions, drafts, change requests and public copies,
        is removed from the live database, and each gets its own deletion receipt. Documents outside
        Trash are moved there first. This cannot be undone.
      </p>
    </div>
    <ul class="picked">
      {#each picked.slice(0, 50) as row (row.id)}<li>
          {row.label}{#if !row.removed_at}<small> · active</small>{/if}
        </li>{/each}
      {#if picked.length > 50}<li>…and {picked.length - 50} more</li>{/if}
    </ul>
    <label class="field"
      >Reason<select class="select" bind:value={bulkReason}
        ><option value="owner_request">Owner request</option><option value="duplicate"
          >Duplicate content</option
        ><option value="policy">Retention or policy decision</option><option value="other"
          >Other administrative decision</option
        ></select
      ></label
    >
    <label class="ack"
      ><input type="checkbox" bind:checked={bulkAcknowledged} /> I understand existing backups may retain
      this data until they expire. A minimal deletion receipt remains.</label
    >
    <label class="field"
      >Type “{bulkPhrase}” to confirm<input
        class="input"
        bind:value={bulkConfirmation}
        autocomplete="off"
        required
      /></label
    >
    {#if bulkResult}<p class="flash flash-success" role="status">{bulkResult}</p>{/if}
    {#if error}<p class="flash flash-error" role="alert">
        <CircleAlert size={16} aria-hidden="true" />{error}
      </p>{/if}
    <div class="dlg-foot">
      <button class="btn" type="button" disabled={busy} onclick={() => bulkDialog.close()}
        >Cancel</button
      ><button
        class="btn btn-danger-solid"
        disabled={busy || bulkConfirmation !== bulkPhrase || !bulkAcknowledged || !picked.length}
        >{busy ? 'Deleting…' : `Delete ${picked.length} permanently`}</button
      >
    </div>
  </form>
</dialog>

<style>
  .section-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    padding-bottom: 8px;
    margin-bottom: 16px;
    border-bottom: 1px solid var(--a-border-muted);
  }
  .section-head p {
    margin-top: 4px;
  }
  #records-heading:focus {
    outline: none;
  }
  .toolbar {
    display: flex;
    align-items: flex-end;
    flex-wrap: wrap;
    gap: 12px;
    margin-bottom: 16px;
  }
  .toolbar .field:first-child {
    min-width: 200px;
  }
  .grow {
    flex: 1;
    min-width: 200px;
  }
  .search {
    position: relative;
    display: block;
  }
  .search :global(svg) {
    position: absolute;
    left: 10px;
    top: 8px;
    color: var(--a-muted);
    pointer-events: none;
  }
  .search .input {
    padding-left: 32px;
  }
  .retry {
    margin-left: auto;
  }
  .flash span {
    flex: 1;
  }
  .flash {
    margin-bottom: 16px;
  }
  .flash :global(svg) {
    flex: none;
  }
  .bulk {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin-left: auto;
  }
  .selected {
    font-weight: 600;
  }
  .audit-filters {
    display: grid;
    gap: 12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 28px;
    padding: 0 8px;
    border: 1px solid var(--a-border);
    border-radius: 2em;
    background: var(--a-canvas);
    color: var(--a-fg);
    font: inherit;
    cursor: pointer;
    transition:
      background-color 0.12s ease,
      border-color 0.12s ease;
  }
  .chip:hover {
    background: var(--a-hover);
  }
  .chip[aria-pressed='true'] {
    background: var(--a-accent-bg);
    border-color: var(--a-accent);
    color: var(--a-accent);
  }
  .audit-fields {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 12px;
  }
  .audit-fields .field {
    min-width: 150px;
  }
  table.dim {
    opacity: 0.6;
  }
  .pick {
    width: 40px;
    padding-right: 0;
  }
  .ident {
    min-width: 220px;
  }
  .ident strong {
    display: block;
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .ident .muted {
    display: block;
    font-size: 12px;
  }
  .idline {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    max-width: 100%;
    color: var(--a-muted);
  }
  .idline code {
    overflow-wrap: anywhere;
  }
  .num {
    text-align: right;
  }
  .cap {
    text-transform: capitalize;
  }
  .action {
    padding: 2px 6px;
    border-radius: 6px;
    background: var(--a-subtle);
    border: 1px solid var(--a-border-muted);
  }
  .actions {
    text-align: right;
    white-space: nowrap;
  }
  .actions-head {
    text-align: right;
  }
  .row-actions {
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .actions :global(.turn) {
    transform: rotate(90deg);
  }
  .actions :global(svg) {
    transition: transform 0.15s ease;
  }
  .empty {
    display: grid;
    justify-items: center;
    gap: 4px;
    padding: 40px 16px;
    text-align: center;
  }
  .empty :global(svg) {
    color: var(--a-muted);
  }
  p.empty {
    border-top: 1px solid var(--a-border-muted);
  }
  .skeleton-row td {
    padding: 14px 16px;
  }
  .skeleton-row:hover {
    background: transparent;
  }
  .detail-row {
    background: var(--a-subtle);
  }
  .detail-row:hover {
    background: var(--a-subtle);
  }
  .detail {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px 24px;
    margin: 0;
  }
  .detail dt {
    color: var(--a-muted);
    font-size: 12px;
    text-transform: capitalize;
  }
  .detail dd {
    margin: 2px 0 0;
    overflow-wrap: anywhere;
  }
  .box-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 16px;
    border-top: 1px solid var(--a-border);
    background: var(--a-subtle);
  }
  .pager {
    display: flex;
    gap: 8px;
  }
  /* Row action menu (native details). */
  .kebab {
    position: relative;
  }
  .kebab summary {
    list-style: none;
  }
  .kebab summary::-webkit-details-marker {
    display: none;
  }
  .kebab[open] summary {
    background: var(--a-inset);
  }
  .menu {
    position: fixed;
    z-index: 30;
    display: grid;
    min-width: 220px;
    padding: 4px 0;
    border: 1px solid var(--a-border);
    border-radius: 6px;
    background: var(--a-canvas);
    box-shadow: var(--a-shadow-pop);
    text-align: left;
    animation: admin-pop 0.12s ease-out;
  }
  .menu button {
    padding: 6px 16px;
    border: 0;
    background: none;
    color: var(--a-fg);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .menu button:hover:not(:disabled) {
    background: var(--a-accent);
    color: #ffffff;
  }
  .menu button.danger {
    color: var(--a-danger);
  }
  .menu button.danger:hover:not(:disabled) {
    background: var(--a-solid-red);
    color: #ffffff;
  }
  .menu button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  /* Dialogs */
  .dlg {
    width: calc(100% - 32px);
    max-width: 560px;
    max-height: 90dvh;
    padding: 0;
    border: 1px solid var(--a-border);
    border-radius: 12px;
    background: var(--a-canvas);
    color: var(--a-fg);
    font-family: var(--a-font);
    font-size: 14px;
    box-shadow: var(--a-shadow-pop);
  }
  .dlg[open] {
    animation: admin-pop 0.16s ease-out;
  }
  .dlg::backdrop {
    background: var(--a-overlay);
  }
  .dlg[open]::backdrop {
    animation: admin-fade 0.16s ease-out;
  }
  @keyframes admin-pop {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(6px);
    }
  }
  @keyframes admin-fade {
    from {
      opacity: 0;
    }
  }
  .dlg-head {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 16px;
    background: var(--a-subtle);
    border-bottom: 1px solid var(--a-border);
  }
  .dlg-head > div {
    flex: 1;
    min-width: 0;
  }
  .dlg-head h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .dlg-icon {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 6px;
    background: var(--a-accent-bg);
    color: var(--a-accent);
    flex: none;
  }
  .dlg-icon.danger {
    background: var(--a-danger-bg);
    color: var(--a-danger);
  }
  .eyebrow {
    margin: 0 0 2px;
    font-size: 12px;
    color: var(--a-muted);
    overflow-wrap: anywhere;
  }
  .dlg-body {
    display: grid;
    gap: 16px;
    padding: 16px;
  }
  .note {
    margin: 0;
    color: var(--a-muted);
    line-height: 1.55;
  }
  .flash p {
    margin: 0;
  }
  .impact {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
    margin: 0;
  }
  .impact > div {
    padding: 8px 12px;
    border: 1px solid var(--a-border);
    border-radius: 6px;
    background: var(--a-subtle);
  }
  .impact dt {
    text-transform: capitalize;
    font-size: 12px;
    color: var(--a-muted);
  }
  .impact dd {
    margin: 2px 0 0;
    font-size: 18px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .ack {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    line-height: 1.5;
  }
  .ack input {
    width: 16px;
    height: 16px;
    margin: 2px 0 0;
    flex: none;
  }
  .dlg-foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 4px;
  }
  .recipient-search {
    display: flex;
    gap: 8px;
  }
  .picked {
    max-height: 180px;
    overflow: auto;
    margin: 0;
    padding: 8px 12px 8px 28px;
    border: 1px solid var(--a-border);
    border-radius: 6px;
    background: var(--a-subtle);
  }
  .picked small {
    color: var(--a-attention);
  }
  @media (max-width: 650px) {
    .section-head {
      flex-direction: column;
      align-items: flex-start;
    }
    .impact {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .bulk {
      margin-left: 0;
    }
  }
</style>
