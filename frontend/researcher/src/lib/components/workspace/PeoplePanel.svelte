<script lang="ts">
  import { onMount } from 'svelte';
  import { Crown, ShieldCheck, Copy, UserPlus, LoaderCircle, ChevronDown } from '@lucide/svelte';
  import {
    RELATION_TEXT,
    ROLES,
    listCopies,
    people as loadPeople,
    percent,
    respondToCopy,
    roleName,
    inviteByEmail,
    invitationNotice,
    setPolicy,
    inheritPolicy,
    shareDocument,
    transferOwnership,
    type CopyRow,
    type MemberRole,
    type People,
    type Policy
  } from '$lib/workspace/collaboration';

  let {
    documentId,
    focus = '',
    onChanged
  }: {
    documentId: string;
    /** `copies` scrolls to detected copies when opened from a notification. */
    focus?: string;
    onChanged?: (people: People) => void;
  } = $props();

  let data = $state<People | null>(null),
    copies = $state<CopyRow[]>([]),
    canAct = $state(false),
    busy = $state(''),
    error = $state(''),
    notice = $state(''),
    email = $state(''),
    // Like Google Docs, invitations default to editing; choose a narrower role as needed.
    role = $state<MemberRole>('editor'),
    rules = $state<Policy>({
      protect_team_version: false,
      required_approvals: 0,
      merge_roles: 'editors'
    }),
    newOwner = $state(''),
    notify = $state(true),
    note = $state(''),
    guideOpen = $state(false),
    copiesSection = $state<HTMLElement | null>(null);

  const you = $derived(data?.you);
  const grantable = ROLES.filter((r) => r.grantable);
  const describeRole = $derived(ROLES.find((r) => r.role === role)?.summary || '');

  async function refresh() {
    try {
      data = await loadPeople(documentId);
      rules = { ...data.policy };
      onChanged?.(data);
      if (
        data.you.role !== 'viewer' &&
        data.you.role !== 'contributor' &&
        data.you.role !== 'reviewer'
      ) {
        const found = await listCopies(documentId);
        copies = found.items;
        canAct = found.can_act;
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not load people.';
    }
  }
  onMount(async () => {
    await refresh();
    if (focus === 'copies') copiesSection?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  });
  async function run(label: string, fn: () => Promise<unknown>, done = '') {
    busy = label;
    error = '';
    notice = '';
    try {
      await fn();
      notice = done;
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Something went wrong. Please try again.';
    } finally {
      busy = '';
    }
  }
  async function invite() {
    const address = email.trim();
    let status: Awaited<ReturnType<typeof inviteByEmail>>['email_status'] = 'skipped';
    await run('Sharing…', async () => {
      status = (
        await inviteByEmail({ document_id: documentId }, address, role, {
          notify,
          note: note.trim()
        })
      ).email_status;
      email = '';
      note = '';
    });
    if (!error) notice = invitationNotice(status, address);
  }
  const change = (address: string, next: MemberRole | 'remove') =>
    run('Updating access…', () => shareDocument(documentId, address, next), 'Access updated.');
  const saveRules = () =>
    run('Saving review rules…', () => setPolicy(documentId, rules), 'Review rules saved.');
  function transfer() {
    const person = data?.members.find((m) => m.id === newOwner);
    if (!person?.email) return;
    if (
      !confirm(
        `Make ${person.name || person.email} the owner of this document? You will stay an editor, and they will be told when copies are uploaded.`
      )
    )
      return;
    void run(
      'Transferring ownership…',
      () => transferOwnership(documentId, person.email!),
      'Ownership transferred.'
    );
  }
  const answer = (row: CopyRow, action: 'invite' | 'dismiss' | 'link') =>
    run(
      'Updating…',
      () => respondToCopy(row.id, action),
      action === 'invite'
        ? 'Invited. They can now send their copy to this document as a change request.'
        : ''
    );
  const name = (p: { name: string | null; email: string | null }) =>
    p.name || p.email?.split('@')[0] || 'Deleted account';
  const MATRIX: [string, keyof NonNullable<typeof you>['can']][] = [
    ['Comment and suggest', 'comment'],
    ['Drafts and change requests', 'propose'],
    ['Approve requests', 'approve'],
    ['Edit the team version', 'edit_team'],
    ['Merge and decline', 'merge'],
    ['Share and set rules', 'share']
  ];
</script>

<div class="people-panel">
  {#if !data && !error}<p class="state"><LoaderCircle size={15} /> Loading people…</p>{/if}
  {#if data && you}
    <section class="owner">
      <Crown size={18} />
      <div>
        <small>Owner</small>
        <strong>{name(data.owner)}</strong>{#if data.owner.email}<span>{data.owner.email}</span
          >{/if}
      </div>
    </section>
    <section class="you">
      <p>
        You are <strong>{roleName(you.role)}</strong
        >{#if you.role !== 'owner' && you.role !== 'manager'}.
          {ROLES.find((r) => r.role === you.role)?.summary}{/if}
      </p>
      <ul class="caps" aria-label="What you can do">
        {#each MATRIX as [label, key]}<li class:yes={you.can[key]}>
            <span aria-hidden="true">{you.can[key] ? '✓' : '–'}</span>{label}
          </li>{/each}
      </ul>
    </section>

    {#if you.can.share}<form
        class="invite"
        onsubmit={(e) => {
          e.preventDefault();
          void invite();
        }}
      >
        <h3><UserPlus size={16} /> Add people</h3>
        <label
          >Email address<input
            type="email"
            placeholder="colleague@example.com"
            bind:value={email}
            required
          /></label
        >
        <label
          >Role<select bind:value={role} aria-label="Role" aria-describedby="role-help"
            >{#each grantable as r}<option value={r.role}>{r.name}</option>{/each}</select
          ></label
        >
        <p id="role-help" class="help">{describeRole}</p>
        <label class="check"
          ><input type="checkbox" bind:checked={notify} /> Send an email invitation with a link</label
        >
        {#if notify}<label
            >Message (optional)<textarea
              bind:value={note}
              rows="2"
              maxlength="500"
              placeholder="e.g. Please review section 2 by Friday"
            ></textarea></label
          >{/if}
        <button class="primary" disabled={!!busy}>Add person</button>
      </form>{/if}

    <section>
      <h3>People with access</h3>
      <ul class="list">
        {#each data.members as m (m.id + m.via)}<li>
            <span class="avatar" aria-hidden="true"
              >{(m.name || m.email || '?').slice(0, 1).toUpperCase()}</span
            >
            <div class="who">
              <strong>{name(m)}</strong>
              <small
                >{m.email || ''}{#if m.via === 'workspace'}{m.email ? ' · ' : ''}through {m.space ||
                    'a workspace'}{/if}</small
              >
            </div>
            {#if you.can.share && m.via === 'direct' && m.role !== 'owner' && m.email}<select
                aria-label={`Role for ${name(m)}`}
                value={m.role}
                disabled={!!busy}
                onchange={(e) =>
                  void change(m.email!, (e.currentTarget as HTMLSelectElement).value as MemberRole)}
                >{#each grantable as r}<option value={r.role}>{r.name}</option>{/each}<option
                  value="remove">Remove access</option
                ></select
              >{:else}<em>{roleName(m.role)}</em>{/if}
          </li>{/each}
      </ul>
      <button
        class="guide-toggle"
        aria-expanded={guideOpen}
        onclick={() => (guideOpen = !guideOpen)}
        ><ChevronDown size={14} /> What each role can do</button
      >
      {#if guideOpen}<dl class="guide">
          {#each ROLES as r}<div>
              <dt>{r.name}</dt>
              <dd>{r.summary}</dd>
            </div>{/each}
        </dl>{/if}
    </section>

    <section class="rules">
      <h3><ShieldCheck size={16} /> Review rules</h3>
      {#if data?.policy_source?.kind === 'project'}<p>
          Inherited from <a href={`/projects/${data.policy_source.id}`}>{data.policy_source.name}</a
          >. Saving rules here creates a file override.
        </p>{:else if data?.policy_source?.kind === 'file'}<p>
          This file has its own review rules.
        </p>
        {#if you.can.manage_rules}<button
            disabled={!!busy}
            onclick={() =>
              run(
                'Restoring inherited rules…',
                () => inheritPolicy(documentId),
                'Inherited rules restored.'
              )}>Use project or default rules</button
          >{/if}{/if}
      <label class="check"
        ><input
          type="checkbox"
          bind:checked={rules.protect_team_version}
          disabled={!you.can.manage_rules}
        />
        <span
          ><strong>Protect the team version</strong><small
            >Only the owner and managers change it directly. Editors and everyone else send changes
            for review.</small
          ></span
        ></label
      >
      <label
        >Approvals needed before merging<select
          bind:value={rules.required_approvals}
          disabled={!you.can.manage_rules}
          >{#each [0, 1, 2, 3, 4, 5] as n}<option value={n}
              >{n === 0 ? 'None' : `${n} approval${n === 1 ? '' : 's'}`}</option
            >{/each}</select
        ></label
      >
      <fieldset disabled={!you.can.manage_rules}>
        <legend>Who merges change requests</legend>
        <label class="check"
          ><input type="radio" value="editors" bind:group={rules.merge_roles} /> Editors, managers and
          the owner</label
        >
        <label class="check"
          ><input type="radio" value="owners" bind:group={rules.merge_roles} /> Only the owner and managers</label
        >
      </fieldset>
      {#if you.can.manage_rules}<button
          class="primary"
          disabled={!!busy}
          onclick={() => void saveRules()}>Save rules</button
        >
        <p class="help">
          The owner and managers can still merge without the required approvals; that choice is
          recorded in the audit log.
        </p>{/if}
    </section>

    {#if you.can.transfer}<section>
        <h3><Crown size={16} /> Transfer ownership</h3>
        <p class="help">
          The owner is notified about copies of this document. Share it with the new owner first.
        </p>
        <div class="row">
          <select bind:value={newOwner} aria-label="New owner"
            ><option value="">Choose a person</option
            >{#each data.members.filter((m) => m.role !== 'owner' && m.email) as m}<option
                value={m.id}>{name(m)} ({roleName(m.role)})</option
              >{/each}</select
          ><button class="secondary" disabled={!newOwner || !!busy} onclick={transfer}
            >Transfer</button
          >
        </div>
      </section>{/if}

    {#if you.role === 'owner' || you.role === 'manager' || you.role === 'editor'}<section
        bind:this={copiesSection}
        class="copies"
      >
        <h3><Copy size={16} /> Copies and similar files</h3>
        <p class="help">
          Uploaded files are compared with this document by content, even when renamed. You see who
          uploaded a matching file and how similar it is — never its content.
        </p>
        {#if !copies.length}<p class="empty">No copies found.</p>{/if}
        <ul class="list">
          {#each copies as c (c.id)}<li class="copy">
              <div class="who">
                <strong
                  >{c.direction === 'copy_of_this'
                    ? `${name(c.person)} uploaded ${c.document.filename ? `“${c.document.filename}”` : `“${c.document.title}”`}`
                    : `This document resembles “${c.document.title}”`}</strong
                >
                <small
                  >{RELATION_TEXT[c.relation]} · {percent(c.score)} match{#if c.shared_blocks}
                    · {c.shared_blocks} identical paragraph{c.shared_blocks === 1 ? '' : 's'}{/if}
                  · {new Date(c.detected_at).toLocaleDateString()}</small
                >
                {#if c.direction === 'copy_of_this' && canAct && c.status === 'open'}<div
                    class="row"
                  >
                    {#if c.relation !== 'same_name'}<button
                        class="primary small"
                        disabled={!!busy}
                        onclick={() => void answer(c, 'invite')}>Invite to send changes</button
                      >{/if}<button
                      class="secondary small"
                      disabled={!!busy}
                      onclick={() => void answer(c, 'link')}>Known, it’s fine</button
                    ><button
                      class="secondary small"
                      disabled={!!busy}
                      onclick={() => void answer(c, 'dismiss')}>Dismiss</button
                    >
                  </div>{:else}<em class="status"
                    >{c.status === 'open'
                      ? 'Open'
                      : c.status === 'invited'
                        ? 'Invited to send changes'
                        : c.status === 'linked'
                          ? c.document.role
                            ? 'Made with “Make a copy”'
                            : 'Known'
                          : 'Dismissed'}</em
                  >{/if}
              </div>
            </li>{/each}
        </ul>
      </section>{/if}
  {/if}
  {#if busy}<p class="state" role="status"><LoaderCircle size={15} /> {busy}</p>{/if}
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
  .people-panel {
    display: grid;
    gap: 18px;
    font-size: 13.5px;
    color: var(--color-ink);
  }
  section,
  form {
    display: grid;
    gap: 10px;
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 0;
    font-size: 14px;
    font-weight: 650;
    color: var(--color-ink-strong);
  }
  .owner {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 12px 14px;
    border-radius: 10px;
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
  }
  .owner div {
    display: grid;
  }
  .owner small {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .owner strong {
    color: var(--color-ink-strong);
  }
  .owner span {
    font-size: 12.5px;
    color: var(--color-ink-soft);
  }
  .you p {
    margin: 0;
    line-height: 1.55;
  }
  .caps {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .caps li {
    display: inline-flex;
    gap: 5px;
    padding: 3px 9px;
    border-radius: 12px;
    font-size: 12px;
    background: var(--color-surface-sunken);
    color: var(--color-muted);
  }
  .caps li.yes {
    background: var(--color-success-bg);
    color: var(--color-success);
  }
  label {
    display: grid;
    gap: 5px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-ink-soft);
  }
  input[type='email'],
  textarea,
  select {
    font: inherit;
    font-size: 13.5px;
    padding: 7px 9px;
    border: 1px solid var(--color-border-strong);
    border-radius: 8px;
    background: var(--color-surface-raised);
    color: var(--color-ink);
  }
  .help {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.55;
    color: var(--color-ink-soft);
  }
  .check {
    display: flex;
    gap: 9px;
    align-items: flex-start;
    font-weight: 400;
    color: var(--color-ink);
  }
  .check span {
    display: grid;
    gap: 2px;
  }
  .check small {
    color: var(--color-ink-soft);
    line-height: 1.5;
  }
  fieldset {
    border: 0;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  legend {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-ink-soft);
    margin-bottom: 4px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
  }
  .list li {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 8px 0;
    border-bottom: 1px solid var(--color-border-soft);
  }
  .avatar {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
    font-weight: 700;
  }
  .who {
    display: grid;
    min-width: 0;
    flex: 1;
  }
  .who strong,
  .who small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .who small {
    color: var(--color-ink-soft);
  }
  .copy .who strong,
  .copy .who small {
    white-space: normal;
    overflow-wrap: anywhere;
  }
  .list li select {
    flex-shrink: 0;
    max-width: 46%;
  }
  em {
    font-style: normal;
    font-size: 12.5px;
    color: var(--color-ink-soft);
    white-space: nowrap;
  }
  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 6px;
  }
  .row select {
    flex: 1;
    min-width: 0;
  }
  .primary,
  .secondary {
    font: inherit;
    font-size: 13px;
    padding: 7px 12px;
    border-radius: 8px;
    cursor: pointer;
    justify-self: start;
  }
  .primary {
    border: 1px solid var(--color-accent-solid);
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
  }
  .secondary {
    border: 1px solid var(--color-border-strong);
    background: var(--color-surface-raised);
    color: var(--color-ink);
  }
  .small {
    font-size: 12px;
    padding: 4px 9px;
  }
  button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .guide-toggle {
    justify-self: start;
    display: inline-flex;
    gap: 5px;
    align-items: center;
    border: 0;
    background: none;
    padding: 0;
    color: var(--color-accent);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .guide {
    margin: 0;
    display: grid;
    gap: 8px;
  }
  .guide div {
    display: grid;
    gap: 2px;
  }
  .guide dt {
    font-weight: 650;
    color: var(--color-ink-strong);
  }
  .guide dd {
    margin: 0;
    color: var(--color-ink-soft);
    line-height: 1.5;
  }
  .rules,
  .copies {
    padding-top: 14px;
    border-top: 1px solid var(--color-border);
  }
  .empty {
    margin: 0;
    color: var(--color-muted);
  }
  .status {
    margin-top: 4px;
  }
  .state,
  .notice,
  .error {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0;
    font-size: 13px;
  }
  .notice {
    color: var(--color-success);
  }
  .error {
    color: var(--color-danger);
  }
</style>
