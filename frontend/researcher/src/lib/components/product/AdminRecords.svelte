<script lang="ts">
  import { onMount } from 'svelte';
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
  let kind = $state('users'),
    q = $state(''),
    offset = $state(0),
    items = $state<RecordRow[]>([]),
    busy = $state(false),
    error = $state(''),
    selected = $state<RecordRow | null>(null),
    confirmation = $state('');
  let dialog: HTMLDialogElement;
  async function refresh() {
    busy = true;
    error = '';
    try {
      items = (
        await request<{ items: RecordRow[] }>(
          `/admin/records?${new URLSearchParams({ kind, q, offset: String(offset) })}`
        )
      ).items;
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not load records';
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    void refresh();
  });
  function open(row: RecordRow) {
    selected = row;
    confirmation = '';
    dialog.showModal();
  }
  async function update() {
    if (!selected) return;
    busy = true;
    error = '';
    try {
      await request('/admin/actions', {
        kind,
        id: selected.id,
        removed: !selected.removed_at,
        confirmation
      });
      dialog.close();
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not update record';
    } finally {
      busy = false;
    }
  }
</script>

<section class="records">
  <h2>Manage accounts and documents</h2>
  <p>
    Move documents to Trash or disable accounts. History remains intact, and these actions can be
    reversed. Every change is recorded in the operations log.
  </p>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      offset = 0;
      void refresh();
    }}
  >
    <label
      >Records<select
        bind:value={kind}
        onchange={() => {
          offset = 0;
          void refresh();
        }}
        ><option value="users">Accounts</option><option value="documents">Documents</option><option
          value="audit">Operations log</option
        ></select
      ></label
    ><label
      >Search<input bind:value={q} maxlength="200" placeholder="Name, email or action" /></label
    ><button disabled={busy}>Search</button>
  </form>
  {#if error}<p role="alert">{error}</p>{/if}
  <div class="table">
    <table>
      <thead
        ><tr
          ><th>{kind === 'audit' ? 'Action' : 'Name'}</th><th
            >{kind === 'audit' ? 'Actor' : 'Details'}</th
          ><th>Date</th><th>Status & actions</th></tr
        ></thead
      ><tbody
        >{#each items as row}<tr
            ><td>{row.label}<small>{row.id}</small></td><td
              >{row.name || row.owner || row.actor || '—'}{#if row.resource_id}<small
                  >{row.resource_id}</small
                >{/if}</td
            ><td>{new Date(row.created_at).toLocaleDateString()}</td><td
              >{#if kind !== 'audit'}<span>{row.removed_at ? 'Removed' : 'Active'}</span><button
                  disabled={busy}
                  onclick={() => open(row)}
                  >{row.removed_at
                    ? 'Restore'
                    : kind === 'users'
                      ? 'Disable account'
                      : 'Move to Trash'}</button
                >{:else}Recorded{/if}</td
            ></tr
          >{/each}</tbody
      >
    </table>
  </div>
  <div class="pages">
    <button
      disabled={busy || offset === 0}
      onclick={() => {
        offset = Math.max(0, offset - 50);
        void refresh();
      }}>Previous</button
    ><span>{offset + 1}–{offset + items.length}</span><button
      disabled={busy || items.length < 50}
      onclick={() => {
        offset += 50;
        void refresh();
      }}>Next</button
    >
  </div>
</section>
<dialog bind:this={dialog} aria-labelledby="action-title">
  <h2 id="action-title">
    {selected?.removed_at
      ? 'Restore access'
      : kind === 'users'
        ? 'Disable account'
        : 'Move document to Trash'}
  </h2>
  <p>
    {kind === 'users'
      ? 'Disabling prevents sign-in and use of existing sessions. Published links are revoked. Restoring does not reactivate old sessions or links.'
      : 'Moving to Trash hides the document and revokes its public links. Content and version history remain available after restoration.'}
  </p>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void update();
    }}
  >
    <label
      >Type “{selected?.label}” to confirm<input
        bind:value={confirmation}
        autocomplete="off"
        required
      /></label
    >{#if error}<p role="alert">{error}</p>{/if}<button
      disabled={busy || confirmation !== selected?.label}>Confirm</button
    ><button type="button" onclick={() => dialog.close()}>Cancel</button>
  </form>
</dialog>

<style>
  .records {
    background: white;
    border: 1px solid #d4dfe9;
    border-radius: 10px;
    padding: 24px;
    margin: 24px 0;
    color: #243f59;
  }
  h2 {
    font-size: 23px;
  }
  p {
    line-height: 1.7;
    color: #486179;
    font-size: 15px;
  }
  form {
    display: flex;
    align-items: end;
    gap: 12px;
    flex-wrap: wrap;
  }
  label {
    display: grid;
    gap: 7px;
    font-size: 14px;
  }
  input,
  select,
  button {
    font: inherit;
    border: 1px solid #b7c8da;
    border-radius: 5px;
    padding: 10px;
    color: #24466b;
    background: white;
  }
  button {
    cursor: pointer;
    background: #e7f0fc;
  }
  button:disabled {
    opacity: 0.6;
  }
  .table {
    overflow: auto;
    margin: 20px 0;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    text-align: left;
    font-size: 14px;
  }
  td,
  th {
    padding: 13px 10px;
    border-bottom: 1px solid #dae3ec;
  }
  small {
    display: block;
    font-size: 11px;
    color: #526a80;
    margin-top: 5px;
  }
  td button {
    margin-left: 8px;
    font-size: 13px;
  }
  .pages {
    display: flex;
    align-items: center;
    gap: 15px;
  }
  dialog {
    max-width: 530px;
    width: calc(100% - 40px);
    border: 1px solid #bdcddd;
    border-radius: 12px;
    padding: 25px;
  }
  dialog::backdrop {
    background: #142e4966;
  }
  dialog form {
    display: grid;
  }
</style>
