<script lang="ts">
  import { onMount } from 'svelte';
  import { invalidateAll } from '$app/navigation';
  import {
    Search,
    Settings2,
    Download,
    X,
    AlertTriangle,
    ShieldCheck,
    ArrowRight
  } from '@lucide/svelte';
  import { workspaceRequest as request } from '$lib/workspace/api';
  type RecordRow = {
    id: string;
    label: string;
    name?: string;
    owner?: string;
    actor?: string;
    resource_id?: string;
    created_at: string;
    removed_at?: string | null;
  };
  type Impact = { revision: string; counts: Record<string, number>; blockers: string[] };
  let kind = $state('users'),
    q = $state(''),
    offset = $state(0);
  let items = $state<RecordRow[]>([]),
    busy = $state(false),
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
  let sequence = 0,
    listSequence = 0;
  const readOnly = $derived(kind === 'audit' || kind === 'erasures');
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
          `/admin/records?${new URLSearchParams({ kind, q, offset: String(offset) })}`
        )
      ).items;
      if (current === listSequence) items = result;
    } catch (e) {
      if (current !== listSequence) return;
      error = e instanceof Error ? e.message : 'Could not load records';
    } finally {
      if (current === listSequence) busy = false;
    }
  }
  onMount(() => {
    void refresh();
  });
  function open(row: RecordRow) {
    selected = row;
    action = kind === 'spaces' ? 'transfer_workspace' : 'lifecycle';
    confirmation = '';
    error = '';
    impact = null;
    acknowledged = false;
    recipient = '';
    recipients = [];
    dialog.showModal();
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
    const fields = [
      'id',
      'label',
      'name',
      'owner',
      'actor',
      'resource_id',
      'created_at',
      'removed_at'
    ] as const;
    const cell = (v: unknown) => {
      let text = String(v ?? '');
      if (/^[\s]*[=+\-@]/.test(text)) text = "'" + text;
      return '"' + text.replaceAll('"', '""') + '"';
    };
    const csv = [
      fields.join(','),
      ...items.map((row) => fields.map((f) => cell(row[f])).join(','))
    ].join('\r\n');
    const href = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }));
    const a = document.createElement('a');
    a.href = href;
    a.download = `dynodoc-${kind}-page-${offset / 50 + 1}.csv`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(href), 1000);
  }
</script>

<section class="records" aria-labelledby="records-heading">
  <div class="section-heading">
    <div>
      <p class="eyebrow">WORKSPACE ADMINISTRATION</p>
      <h2 id="records-heading">People, content & access</h2>
      <p>Review access, transfer ownership and manage the full content lifecycle.</p>
    </div>
    <button class="quiet" disabled={busy || !items.length} onclick={exportPage}
      ><Download size={16} /> Export this page</button
    >
  </div>
  <form
    class="filters"
    onsubmit={(e) => {
      e.preventDefault();
      offset = 0;
      void refresh();
    }}
  >
    <label
      >Browse<select
        bind:value={kind}
        onchange={() => {
          offset = 0;
          void refresh();
        }}
        ><option value="users">Accounts</option><option value="documents">Documents</option><option
          value="spaces">Organizations & folders</option
        ><option value="audit">Operations log</option><option value="erasures"
          >Deletion receipts</option
        ></select
      ></label
    >
    <label class="search"
      >Search
      <div>
        <Search size={17} /><input
          bind:value={q}
          maxlength="200"
          placeholder="Name, email or action"
        />
      </div></label
    ><button class="primary" disabled={busy}>Search</button>
  </form>
  {#if error && !selected}<p class="error" role="alert">{error}</p>{/if}
  <div class="table" aria-busy={busy}>
    <table>
      <thead
        ><tr
          ><th>{readOnly ? 'Record' : 'Name'}</th><th>{kind === 'audit' ? 'Actor' : 'Details'}</th
          ><th>Created</th><th>{readOnly ? 'Record status' : 'Access'}</th><th
            ><span class="sr-only">Actions</span></th
          ></tr
        ></thead
      ><tbody>
        {#each items as row}<tr
            ><td><strong>{row.label}</strong><small>{row.id}</small></td><td
              >{row.name || row.owner || row.actor || '—'}{#if row.resource_id}<small
                  >{row.resource_id}</small
                >{/if}</td
            ><td>{new Date(row.created_at).toLocaleDateString()}</td><td
              ><span class="status" class:removed={!!row.removed_at}
                >{readOnly ? 'Recorded' : row.removed_at ? 'Disabled / Trash' : 'Active'}</span
              ></td
            ><td
              >{#if !readOnly}<button
                  class="quiet manage"
                  disabled={busy}
                  onclick={() => open(row)}
                  aria-label={`Manage ${row.label}`}><Settings2 size={15} /> Manage</button
                >{/if}</td
            ></tr
          >{/each}
        {#if !items.length}<tr
            ><td colspan="5" class="empty"
              >{busy ? 'Loading records…' : 'No matching records. Try a different search.'}</td
            ></tr
          >{/if}
      </tbody>
    </table>
  </div>
  <div class="pages">
    <span>{items.length ? `${offset + 1}–${offset + items.length}` : '0 records'}</span><button
      class="quiet"
      disabled={busy || offset === 0}
      onclick={() => {
        offset = Math.max(0, offset - 50);
        void refresh();
      }}>Previous</button
    ><button
      class="quiet"
      disabled={busy || items.length < 50}
      onclick={() => {
        offset += 50;
        void refresh();
      }}>Next <ArrowRight size={14} /></button
    >
  </div>
</section>
<dialog
  bind:this={dialog}
  aria-labelledby="action-title"
  onclose={() => {
    selected = null;
    ++sequence;
    previewing = false;
  }}
>
  <div class="dialog-top">
    <span class="dialog-icon"><ShieldCheck size={22} /></span><button
      type="button"
      class="quiet"
      aria-label="Close administration dialog"
      disabled={busy}
      onclick={() => dialog.close()}><X size={20} /></button
    >
  </div>
  <p class="eyebrow">{selected?.label}</p>
  <h2 id="action-title">{actionTitle}</h2>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void update();
    }}
  >
    <label
      >Action<select bind:value={action} onchange={changeAction} disabled={busy}>
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
      <div class="warning">
        <AlertTriangle size={20} />
        <p>
          {kind === 'documents'
            ? 'Deletes the document, history, files, versions, drafts and public copies from the live database. This cannot be undone.'
            : 'Removes the account profile, credentials, personal drafts and access. Shared contributions retain a deleted-user identity so other documents remain verifiable.'}
        </p>
      </div>
      {#if previewing}<p role="status">Calculating deletion impact…</p>{/if}
      {#if impact}<dl class="impact">
          {#each Object.entries(impact.counts) as [key, value]}<div>
              <dt>{key.replaceAll('_', ' ')}</dt>
              <dd>{value.toLocaleString()}</dd>
            </div>{/each}
        </dl>
        {#each impact.blockers as blocker}<p class="error">{blocker}</p>{/each}{/if}
      <label
        >Reason<select bind:value={reason}
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
      <p>
        Give an active account ownership. Existing collaborators keep their access. Workspace owners
        become managers when a replacement owner is selected.
      </p>
      <label
        >Find the new owner
        <div class="recipient-search">
          <input bind:value={recipientSearch} placeholder="Search account email" /><button
            class="quiet"
            type="button"
            onclick={findRecipient}>Find</button
          >
        </div></label
      >
      <label
        >New owner<select bind:value={recipient} required
          ><option value="">Choose an active account</option>{#each recipients as person}<option
              value={person.id}>{person.label}</option
            >{/each}</select
        ></label
      >
    {:else}<p>
        {action === 'revoke_sessions'
          ? 'All existing sessions and pending sign-in links stop working. The account can sign in again.'
          : action === 'revoke_public_links'
            ? 'Every published link to this document stops working. Previously downloaded copies cannot be recalled.'
            : kind === 'users'
              ? 'Disabling blocks existing sessions and revokes published links. Restoring requires a fresh sign-in.'
              : 'Trash keeps content and history, and revokes public links. Restoring never reactivates old public links.'}
      </p>{/if}
    <label
      >Type “{selected?.label}” to confirm<input
        bind:value={confirmation}
        autocomplete="off"
        required
      /></label
    >
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="dialog-actions">
      <button class="quiet" type="button" disabled={busy} onclick={() => dialog.close()}
        >Cancel</button
      ><button
        class="primary"
        class:danger={action === 'erase'}
        disabled={busy ||
          confirmation !== selected?.label ||
          (action === 'erase' && (!impact || impact.blockers.length > 0 || !acknowledged)) ||
          (action.startsWith('transfer_') && !recipient)}>{busy ? 'Applying…' : actionTitle}</button
      >
    </div>
  </form>
</dialog>

<style>
  .records {
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border-soft);
    border-radius: 24px;
    margin: 28px 0;
    color: var(--color-ink);
    box-shadow: var(--shadow-panel);
    overflow: hidden;
  }
  .section-heading {
    padding: 28px 30px 12px;
    display: flex;
    justify-content: space-between;
    gap: 24px;
    align-items: center;
  }
  h2 {
    font-size: 24px;
    letter-spacing: -0.7px;
    margin: 6px 0;
  }
  p {
    font-size: 14px;
    line-height: 1.65;
    color: var(--color-ink-soft);
    margin: 8px 0;
  }
  .eyebrow {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 1.5px;
    color: var(--color-accent);
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    cursor: pointer;
    white-space: nowrap;
  }
  .quiet,
  .primary {
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 10px 14px;
    font: inherit;
    font-size: 13px;
    background: var(--color-surface-raised);
    color: var(--color-ink);
  }
  .quiet:hover {
    background: var(--color-surface-hover);
  }
  .primary {
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
    border-color: transparent;
  }
  .primary:hover {
    background: var(--color-accent-solid-hover);
  }
  .danger {
    background: var(--color-danger);
    /* White fails on the light night-mode red; the pale danger tint reads in both modes. */
    color: var(--color-danger-bg);
  }
  .danger:hover {
    background: color-mix(in srgb, var(--color-danger) 85%, var(--color-ink-strong));
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .filters {
    display: flex;
    gap: 14px;
    align-items: end;
    padding: 16px 30px 24px;
  }
  label {
    display: grid;
    gap: 7px;
    font-size: 12px;
    font-weight: 600;
    color: var(--color-ink-soft);
  }
  input,
  select {
    width: 100%;
    min-width: 0;
    font: inherit;
    font-size: 14px;
    color: var(--color-ink);
    border: 1px solid var(--color-border);
    background: var(--color-surface-sunken);
    border-radius: 10px;
    padding: 11px 12px;
  }
  .search {
    flex: 1;
  }
  .search > div {
    position: relative;
  }
  .search :global(svg) {
    position: absolute;
    left: 13px;
    top: 14px;
  }
  .search input {
    padding-left: 39px;
  }
  .table {
    overflow: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    text-align: left;
    font-size: 13px;
  }
  th {
    background: var(--color-surface-sunken);
    padding: 13px 28px;
    font-size: 11px;
    letter-spacing: 0.3px;
    color: var(--color-ink-soft);
    font-weight: 600;
    white-space: nowrap;
  }
  td {
    padding: 18px 28px;
    border-bottom: 1px solid var(--color-border-soft);
  }
  td strong {
    font-weight: 600;
  }
  td small {
    display: block;
    color: var(--color-ink-soft);
    font-size: 11px;
    margin-top: 5px;
    overflow-wrap: anywhere;
  }
  tbody tr:hover {
    background: var(--color-surface-hover);
  }
  .status {
    font-size: 11px;
    color: var(--color-success);
    background: var(--color-success-bg);
    border-radius: 20px;
    padding: 5px 9px;
    white-space: nowrap;
  }
  .status.removed {
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
  }
  .pages {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 18px 28px;
    font-size: 12px;
    color: var(--color-ink-soft);
  }
  .pages > span {
    margin-right: auto;
  }
  .empty {
    padding: 45px;
    text-align: center;
  }
  .error {
    color: var(--color-danger);
    background: var(--color-danger-bg);
    padding: 12px;
    border-radius: 10px;
  }
  .records > .error {
    margin: 0 30px;
  }
  dialog {
    border: 1px solid var(--color-border);
    border-radius: 24px;
    padding: 28px;
    max-width: 590px;
    width: calc(100% - 32px);
    background: var(--color-surface-raised);
    color: var(--color-ink);
    box-shadow: var(--shadow-dialog);
    max-height: 90dvh;
  }
  dialog::backdrop {
    background: var(--color-overlay);
    backdrop-filter: blur(4px);
  }
  .dialog-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }
  .dialog-icon {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 14px;
    background: var(--color-accent-bg);
    color: var(--color-accent);
  }
  dialog form {
    display: grid;
    gap: 17px;
    margin-top: 20px;
  }
  .warning {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    background: var(--color-warn-bg);
    padding: 15px;
    border-radius: 12px;
    color: var(--color-warn);
  }
  .warning :global(svg) {
    flex-shrink: 0;
    margin-top: 4px;
  }
  .warning p {
    margin: 0;
    color: var(--color-warn-ink);
  }
  .impact {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    margin: 0;
  }
  .impact > div {
    padding: 10px;
    background: var(--color-surface-sunken);
    border-radius: 10px;
  }
  .impact dt {
    text-transform: capitalize;
    font-size: 11px;
    color: var(--color-ink-soft);
  }
  .impact dd {
    margin: 4px 0 0;
    font-size: 18px;
    font-weight: 650;
  }
  .ack {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    font-weight: 400;
    line-height: 1.6;
  }
  .ack input {
    width: 17px;
    height: 17px;
    flex-shrink: 0;
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 4px;
  }
  .recipient-search {
    display: flex;
    gap: 8px;
  }
  @media (max-width: 650px) {
    .section-heading,
    .filters {
      padding: 20px;
      flex-wrap: wrap;
    }
    .section-heading {
      align-items: flex-start;
    }
    .search {
      min-width: 160px;
    }
    td,
    th {
      padding: 13px 16px;
    }
    .records {
      border-radius: 18px;
    }
    .impact {
      grid-template-columns: repeat(2, 1fr);
    }
  }
</style>
