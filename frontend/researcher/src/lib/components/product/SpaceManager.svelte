<script lang="ts">
  // Stored workspace roles keep their historical names; show the product names.
  const ROLE_NAMES: Record<string, string> = {
    owner: 'Owner',
    manager: 'Manager',
    editor: 'Editor',
    approver: 'Reviewer',
    reviewer: 'Contributor',
    viewer: 'Viewer'
  };
  const roleLabel = (role?: string) => (role ? ROLE_NAMES[role] || role : '');
  import { Plus, Folder, Building2, Users, X } from '@lucide/svelte';
  import { workspaceRequest as request } from '$lib/workspace/api';
  import { inviteByEmail, invitationNotice } from '$lib/workspace/collaboration';
  import type { Space } from '$lib/workspace/access';
  let {
    spaces,
    selected,
    onSelect,
    onRefresh
  }: {
    spaces: Space[];
    selected: string;
    onSelect: (id: string) => void;
    onRefresh: () => Promise<void>;
  } = $props();
  let dialog: HTMLDialogElement;
  let name = $state(''),
    kind = $state('organization'),
    email = $state(''),
    role = $state('editor'),
    error = $state(''),
    busy = $state(false);
  let members = $state<
    { id: string; email: string; role: string; space_name: string; inherited: boolean }[]
  >([]);
  const current = $derived(spaces.find((s) => s.id === selected));
  const manager = $derived(current && ['owner', 'manager'].includes(current.role));
  function depth(s: Space) {
    let n = 0,
      p = s.parent_id;
    while (p && n < 12) {
      n++;
      p = spaces.find((x) => x.id === p)?.parent_id || null;
    }
    return n;
  }
  async function open() {
    error = '';
    name = '';
    kind = current?.kind === 'organization' ? 'team' : current ? 'folder' : 'organization';
    dialog.showModal();
    await run(people);
  }
  async function people() {
    members = current
      ? (await request<{ items: typeof members }>(`/spaces/${current.id}/members`)).items
      : [];
  }
  async function run(work: () => Promise<void>) {
    busy = true;
    error = '';
    try {
      await work();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not update workspace';
    } finally {
      busy = false;
    }
  }
  async function create() {
    await run(async () => {
      const x = await request<{ id: string }>('/spaces', {
        name,
        kind,
        parent_id: kind === 'organization' ? null : selected
      });
      await onRefresh();
      onSelect(x.id);
      dialog.close();
    });
  }
  let notify = $state(true),
    notice = $state('');
  async function share() {
    notice = '';
    await run(async () => {
      const address = email.trim();
      const result = await inviteByEmail({ space_id: selected }, address, role, { notify });
      notice = invitationNotice(result.email_status, address, result.invitation_link);
      email = '';
      await people();
    });
  }
</script>

<div class="spaces" data-tour="organization">
  <div class="label">
    <strong>Team workspaces</strong><button
      aria-label="Manage team workspaces"
      onclick={() => void open()}><Plus size={16} /></button
    >
  </div>
  <button class:active={!selected} onclick={() => onSelect('')}
    ><Folder size={16} />All accessible documents</button
  >
  {#each spaces as s}<button
      class:active={selected === s.id}
      style:padding-left={`${10 + depth(s) * 12}px`}
      onclick={() => onSelect(s.id)}
      >{#if s.kind === 'organization'}<Building2 size={15} />{:else if s.kind === 'team'}<Users
          size={15}
        />{:else}<Folder size={15} />{/if}<span>{s.name}</span></button
    >{/each}
  {#if current}<button class="manage" onclick={() => void open()}
      >Access & folders · {roleLabel(current.role)}</button
    >{/if}
</div>
<dialog bind:this={dialog} aria-labelledby="space-title">
  <header>
    <h2 id="space-title">{current?.name || 'Organize your research'}</h2>
    <button aria-label="Close workspace settings" onclick={() => dialog.close()}
      ><X size={18} /></button
    >
  </header>
  <p>
    Organizations contain teams, and teams contain folders. Access is inherited from parent
    workspaces; the strongest applicable role wins.
  </p>
  {#if error}<p role="alert" class="error">{error}</p>{/if}
  {#if !current || manager}<form
      onsubmit={(e) => {
        e.preventDefault();
        void create();
      }}
    >
      <h3>Create a workspace</h3>
      <label>Name<input bind:value={name} required maxlength="160" /></label><label
        >Type<select bind:value={kind}
          ><option value="organization">Organization</option
          >{#if current?.kind === 'organization'}<option value="team">Team in {current.name}</option
            >{:else if current}<option value="folder">Folder in {current.name}</option>{/if}</select
        ></label
      ><button class="primary" disabled={busy}>Create workspace</button>
    </form>{/if}
  {#if current}<h3>People and inherited access</h3>
    {#each members as m}<div class="member">
        <span
          >{m.email}<small>{m.inherited ? `Inherited from ${m.space_name}` : 'Direct member'}</small
          ></span
        ><b>{roleLabel(m.role)}</b>
      </div>{/each}
    {#if manager}<form
        onsubmit={(e) => {
          e.preventDefault();
          void share();
        }}
      >
        <label>Collaborator email<input type="email" bind:value={email} required /></label><label
          >Role<select bind:value={role}
            >{#if current.role === 'owner'}<option value="owner">Owner</option>{/if}<option
              value="manager">Manager</option
            ><option value="editor">Editor</option><option value="approver"
              >Reviewer (approves requests)</option
            ><option value="reviewer">Contributor (comments & suggests)</option><option
              value="viewer">Viewer</option
            ><option value="remove">Remove direct access</option></select
          ></label
        ><label class="notify"
          ><input type="checkbox" bind:checked={notify} /> Send an email invitation</label
        ><button class="primary" disabled={busy}>Update access</button>
        {#if notice}<p role="status" class="notice">{notice}</p>{/if}
      </form>{/if}{/if}
</dialog>

<style>
  .spaces {
    border-top: 1px solid var(--color-border);
    margin-top: 16px;
    padding-top: 12px;
  }
  .label,
  header,
  .member {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .label {
    font-size: 13px;
    margin-bottom: 8px;
  }
  .spaces > button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    padding: 9px 10px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--color-ink-soft);
    font-size: 13px;
  }
  .spaces > button span {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .spaces > button.active {
    background: var(--color-surface-selected);
    color: var(--color-accent-strong);
  }
  .spaces > .manage {
    font-size: 12px;
    text-decoration: underline;
  }
  button {
    cursor: pointer;
  }
  .label button,
  header button {
    border: 0;
    background: none;
    min-width: 32px;
    min-height: 32px;
    color: var(--color-ink);
  }
  dialog {
    border: 1px solid var(--color-border);
    border-radius: 12px;
    max-width: 570px;
    width: calc(100% - 40px);
    max-height: 85vh;
    max-height: 85dvh;
    padding: 24px;
    color: var(--color-ink);
    background: var(--color-surface-raised);
  }
  dialog::backdrop {
    background: var(--color-overlay);
  }
  h2 {
    font-size: 23px;
    margin: 0;
  }
  h3 {
    font-size: 17px;
  }
  p {
    font-size: 15px;
    line-height: 1.7;
    color: var(--color-ink-soft);
  }
  form {
    display: grid;
    gap: 12px;
    border-top: 1px solid var(--color-border);
    margin-top: 18px;
    padding-top: 14px;
  }
  label {
    display: grid;
    gap: 6px;
    font-size: 14px;
  }
  input,
  select {
    padding: 10px;
    border: 1px solid var(--color-border-strong);
    border-radius: 5px;
    background: var(--color-surface-raised);
    color: var(--color-ink);
    font: inherit;
  }
  .primary {
    padding: 11px;
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
    border: 0;
    border-radius: 5px;
  }
  .member {
    font-size: 14px;
    padding: 10px 0;
    border-bottom: 1px solid var(--color-border);
  }
  .member small {
    display: block;
    color: var(--color-ink-soft);
    margin-top: 4px;
  }
  .member b {
    font-size: 12px;
  }
  .error {
    color: var(--color-danger);
  }
  .notify {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .notice {
    margin: 0;
    font-size: 13px;
    color: var(--color-success);
  }
</style>
