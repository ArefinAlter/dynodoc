<script lang="ts">
  import { tick } from 'svelte';
  import { Bot, Plus, ShieldAlert, ShieldCheck, TriangleAlert } from '@lucide/svelte';
  import {
    ACCESS_LABEL,
    AGENT_SCOPES,
    DEFAULT_AGENT_SCOPES,
    DEFAULT_SCOPES,
    SCOPES,
    clearGrant,
    deviceMessage,
    grantableProjects,
    scopeLabel,
    scopeUsable,
    setAccess,
    setGrant,
    shortKey,
    unusableScopes,
    type DeviceKey,
    type DeviceScope,
    type ProjectChoice,
    type Subject
  } from '$lib/workspace/device-grants';
  import { projectRoleName } from '$lib/workspace/projects';

  let {
    device,
    projects,
    projectsState,
    grantLimit,
    fresh = false,
    onChanged
  }: {
    device: DeviceKey;
    projects: ProjectChoice[];
    projectsState: 'loading' | 'ready' | 'failed';
    grantLimit: number;
    /** The key was just connected: lead with the next step. */
    fresh?: boolean;
    /** Reload the registry after a change; the server's list is the only truth. */
    onChanged: () => Promise<void>;
  } = $props();
  const uid = $props.id();

  const short = $derived(shortKey(device.publicKey));
  const isAgent = $derived(device.keyClass === 'agent');
  // The "all projects" mode never covers an agent's scope: it needs a grant for every project.
  const mode = $derived(isAgent ? 'granted' : (device.projectAccess ?? 'granted'));
  const verb = $derived(isAgent ? 'read' : 'open');
  const grants = $derived(device.grants);
  const available = $derived(grantableProjects(projects, grants));
  const full = $derived(grants.length >= grantLimit);

  let adding = $state(false),
    addProject = $state(''),
    addScopes = $state<DeviceScope[]>([...DEFAULT_SCOPES]),
    editing = $state(''),
    editScopes = $state<DeviceScope[]>([]),
    confirmingAll = $state(false),
    busy = $state(false),
    error = $state(''),
    message = $state('');
  let group = $state<HTMLElement | null>(null),
    projectSelect = $state<HTMLSelectElement | null>(null),
    addButton = $state<HTMLButtonElement | null>(null),
    nextStepButton = $state<HTMLButtonElement | null>(null),
    allowButton = $state<HTMLButtonElement | null>(null),
    cancelAllButton = $state<HTMLButtonElement | null>(null);
  let focusedNextStep = false;

  // A freshly connected key leads the person straight to its one useful action.
  $effect(() => {
    if (fresh && nextStepButton && !focusedNextStep) {
      focusedNextStep = true;
      nextStepButton.focus();
    }
  });

  const chosen = $derived(available.find((p) => p.id === addProject) ?? null);
  const addUnusable = $derived(unusableScopes(chosen?.role ?? null, addScopes));
  const toggle = (list: DeviceScope[], scope: DeviceScope) =>
    list.includes(scope) ? list.filter((s) => s !== scope) : [...list, scope];
  const names = (scopes: readonly DeviceScope[]) => scopes.map(scopeLabel).join(' and ');

  async function run(action: () => Promise<void>, subject: Subject) {
    if (busy) return;
    busy = true;
    error = '';
    message = '';
    try {
      await action();
    } catch (e) {
      error = deviceMessage(e, subject);
    } finally {
      busy = false;
    }
  }
  /** The change is saved; a failed refresh must not read as a failed save. */
  async function refreshed(done: string) {
    try {
      await onChanged();
      message = done;
    } catch {
      message = `${done} The list could not be refreshed. Reload the page to see it.`;
    }
  }
  /** Put focus back somewhere sensible once the DOM has caught up with the change. */
  async function settle(target: () => HTMLElement | null | undefined) {
    await tick();
    (target() ?? group)?.focus();
  }

  async function startAdd() {
    error = '';
    message = '';
    adding = true;
    addScopes = [...(isAgent ? DEFAULT_AGENT_SCOPES : DEFAULT_SCOPES)];
    addProject = available.length === 1 ? available[0].id : '';
    await tick();
    projectSelect?.focus();
  }
  async function cancelAdd() {
    adding = false;
    await settle(() => addButton ?? nextStepButton);
  }
  async function saveAdd() {
    const target = chosen;
    if (!target || !addScopes.length) return;
    await run(async () => {
      await setGrant(device.id, target.id, addScopes);
      adding = false;
      await refreshed(`Granted “${target.name}” to this key.`);
    }, 'grant');
    if (!adding) await settle(() => addButton ?? nextStepButton);
  }
  function startEdit(projectId: string, scopes: DeviceScope[]) {
    error = '';
    message = '';
    editing = projectId;
    editScopes = [...scopes];
  }
  async function saveEdit(projectId: string, name: string) {
    if (!editScopes.length) return;
    const row = `${uid}-edit-${projectId}`;
    await run(async () => {
      const result = await setGrant(device.id, projectId, editScopes);
      editing = '';
      await refreshed(
        result.changed
          ? `Permissions for “${name}” updated. This applies to the key’s next request.`
          : `“${name}” already had these permissions.`
      );
    }, 'grant');
    if (!editing) await settle(() => document.getElementById(row));
  }
  async function revokeGrant(projectId: string, name: string | null) {
    await run(async () => {
      await clearGrant(device.id, projectId);
      editing = '';
      await refreshed(
        `${name ? `Removed “${name}” from this key.` : 'Removed the grant.'} This applies to the key’s next request.`
      );
    }, 'revoke-grant');
    await settle(() => addButton ?? nextStepButton);
  }
  async function allowAll() {
    await run(async () => {
      await setAccess(device.id, 'all', true);
      confirmingAll = false;
      await refreshed('This key can now open every project you can access.');
    }, 'mode');
    await settle(() => null);
  }
  async function limitToGranted() {
    await run(async () => {
      await setAccess(device.id, 'granted');
      await refreshed('This key is limited to the projects you grant. Existing grants were kept.');
    }, 'mode');
    await settle(() => null);
  }
  async function askAll() {
    error = '';
    message = '';
    confirmingAll = true;
    await tick();
    cancelAllButton?.focus();
  }
  async function cancelAll() {
    confirmingAll = false;
    await settle(() => allowButton);
  }
  function escape(event: KeyboardEvent, cancel: () => void) {
    if (event.key === 'Escape') {
      event.stopPropagation();
      cancel();
    }
  }
</script>

{#snippet scopePicker(
  idBase: string,
  legend: string,
  selected: DeviceScope[],
  change: (scope: DeviceScope) => void
)}
  <fieldset class="scopes" disabled={busy}>
    <legend>{legend}</legend>
    {#each SCOPES as scope (scope.id)}
      <label class="scope">
        <input
          type="checkbox"
          checked={selected.includes(scope.id)}
          aria-describedby={`${idBase}-${scope.id}`}
          onchange={() => change(scope.id)}
        />
        <span class="scope-text"
          ><span class="scope-name">{scope.label}</span><span
            class="repo-muted repo-small"
            id={`${idBase}-${scope.id}`}>{scope.help}</span
          ></span
        >
      </label>
    {/each}
  </fieldset>
{/snippet}

<div
  class="access"
  role="group"
  aria-label={`Project access for key ${short}`}
  tabindex="-1"
  bind:this={group}
>
  <div class="access-head">
    <span class="repo-badge mode" class:broad={mode === 'all'}>
      {#if mode === 'all'}<ShieldAlert size={14} aria-hidden="true" />{:else}<ShieldCheck
          size={14}
          aria-hidden="true"
        />{/if}
      {ACCESS_LABEL[mode]}
    </span>
    {#if mode === 'granted'}<span class="repo-small repo-muted"
        >{grants.length} of {grantLimit} projects granted</span
      >{/if}
  </div>

  <div aria-live="polite" class="live">
    {#if message}<p class="repo-notice">{message}</p>{/if}
  </div>
  {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}

  {#if isAgent}
    <p class="agent-note">
      <Bot size={16} aria-hidden="true" />
      <span
        ><strong>For an AI agent.</strong> It can only read documents, only in the projects you grant
        below, and only once that project’s owner or manager has turned on AI agents in the project’s
        settings. An AI agent can only ever propose changes, which a person reviews; in this release it
        can only read. You can take its access away at any time. Whatever it reads is sent to the AI service
        it is connected to.</span
      >
    </p>
  {/if}

  {#if mode === 'all'}
    <p class="broad-note">
      <TriangleAlert size={16} aria-hidden="true" />
      <span
        >A copy of this key file can open every project you can access, with any permission your
        role allows, including review decisions. Limit it to selected projects unless you need this.</span
      >
    </p>
  {:else if grants.length === 0 && !adding}
    {#if fresh}
      <div class="next-step">
        <p class="next-title">Next step: grant a project</p>
        <p>This key can't {verb} any project yet. Grant it a project.</p>
        <button
          type="button"
          class="repo-primary"
          bind:this={nextStepButton}
          disabled={busy || adding}
          aria-label={`Grant a project to key ${short}`}
          onclick={() => void startAdd()}
          ><Plus size={14} aria-hidden="true" />Grant a project</button
        >
      </div>
    {:else}
      <p class="repo-muted">This key can't {verb} any project yet.</p>
    {/if}
  {/if}

  {#if grants.length}
    <ul class="grants" aria-label={`Granted projects for key ${short}`}>
      {#each grants as grant (grant.projectId)}
        {@const label = grant.projectName ?? 'a project you no longer belong to'}
        {@const blocked = unusableScopes(grant.role, grant.scopes)}
        <li class="grant">
          <div class="grant-head">
            {#if grant.projectName}
              <a class="grant-name" href={`/projects/${grant.projectId}`}>{grant.projectName}</a>
              {#if grant.role}<span class="repo-role">{projectRoleName(grant.role)}</span>{/if}
            {:else}
              <span class="grant-name muted">A project you no longer belong to</span>
              <span class="repo-badge">No access</span>
            {/if}
            {#if mode === 'all'}<span class="repo-badge"
                >Not used while all projects are allowed</span
              >{/if}
          </div>
          {#if editing === grant.projectId}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <form
              onsubmit={(event) => {
                event.preventDefault();
                void saveEdit(grant.projectId, label);
              }}
              onkeydown={(event) => escape(event, () => (editing = ''))}
            >
              {@render scopePicker(
                `${uid}-e-${grant.projectId}`,
                `Permissions for ${label} on key ${short}`,
                editScopes,
                (scope) => {
                  editScopes = toggle(editScopes, scope);
                }
              )}
              {#if !editScopes.length}<p class="repo-small repo-muted">
                  Choose at least one permission, or revoke the grant.
                </p>{:else if unusableScopes(grant.role, editScopes).length}<p
                  class="repo-small repo-muted"
                >
                  Your role here ({projectRoleName(grant.role ?? '')}) does not include {names(
                    unusableScopes(grant.role, editScopes)
                  )}, so it would have no effect.
                </p>{/if}
              <div class="actions">
                <button class="repo-primary" disabled={busy || !editScopes.length}
                  >{busy ? 'Saving…' : 'Save permissions'}</button
                >
                <button type="button" disabled={busy} onclick={() => (editing = '')}>Cancel</button>
              </div>
            </form>
          {:else}
            <ul class="chips" aria-label={`Permissions for ${label} on key ${short}`}>
              {#each grant.scopes as scope (scope)}
                {@const usable = scopeUsable(grant.role, scope)}
                <li class="chip" class:idle={!usable}>
                  {scopeLabel(scope)}{#if !usable}<span class="sr-only">
                      (not usable with your role)</span
                    >{/if}
                </li>
              {/each}
            </ul>
            {#if !grant.projectName}
              <p class="repo-small repo-muted">
                You are no longer a member of this project, so this grant does nothing. Revoke it to
                remove it.
              </p>
            {:else if blocked.length}
              <p class="repo-small repo-muted">
                Your role here ({projectRoleName(grant.role ?? '')}) does not include {names(
                  blocked
                )}, so {blocked.length === 1 ? 'that permission has' : 'those permissions have'} no effect.
              </p>
            {/if}
            <div class="actions">
              {#if mode === 'granted' && grant.projectName && !isAgent}
                <button
                  type="button"
                  id={`${uid}-edit-${grant.projectId}`}
                  disabled={busy}
                  aria-label={`Change permissions for ${label} on key ${short}`}
                  onclick={() => startEdit(grant.projectId, grant.scopes)}
                  >Change permissions</button
                >
              {/if}
              <button
                type="button"
                disabled={busy}
                aria-label={`Revoke grant for ${label} on key ${short}`}
                onclick={() => void revokeGrant(grant.projectId, grant.projectName)}
                >Revoke grant</button
              >
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if mode === 'granted'}
    {#if adding}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <form
        class="add"
        aria-label={`Add a project to key ${short}`}
        onsubmit={(event) => {
          event.preventDefault();
          void saveAdd();
        }}
        onkeydown={(event) => escape(event, () => void cancelAdd())}
      >
        <label
          >Project<select
            bind:this={projectSelect}
            bind:value={addProject}
            required
            disabled={busy || projectsState !== 'ready' || !available.length}
            aria-label={`Project to grant to key ${short}`}
          >
            <option value="">
              {projectsState === 'loading'
                ? 'Loading projects…'
                : projectsState === 'failed'
                  ? 'Projects could not be loaded'
                  : available.length
                    ? 'Choose a project'
                    : 'No more projects to grant'}
            </option>
            {#each available as project (project.id)}
              <option value={project.id}>{project.name} · {projectRoleName(project.role)}</option>
            {/each}
          </select></label
        >
        {#if projectsState === 'failed'}
          <p class="repo-small repo-muted">
            Your projects could not be loaded. Reload the page to try again.
          </p>
        {/if}
        {#if isAgent}
          <div
            class="scopes"
            role="group"
            aria-label={`Permission for the new project on key ${short}`}
          >
            <p class="scope-name">Permission: {AGENT_SCOPES[0].label}</p>
            <p class="repo-muted repo-small">{AGENT_SCOPES[0].help}</p>
          </div>
        {:else}
          {@render scopePicker(
            `${uid}-add`,
            `Permissions for the new project on key ${short}`,
            addScopes,
            (scope) => {
              addScopes = toggle(addScopes, scope);
            }
          )}
          <p class="repo-small repo-muted">
            The command line needs Read history to pull and Write history to push. Review is only
            needed to record approvals.
          </p>
        {/if}
        {#if chosen && addUnusable.length}<p class="repo-small repo-muted">
            Your role in {chosen.name} ({projectRoleName(chosen.role)}) does not include {names(
              addUnusable
            )}, so {addUnusable.length === 1 ? 'that permission' : 'those permissions'} would have no
            effect.
          </p>{/if}
        <div class="actions">
          <button class="repo-primary" disabled={busy || !chosen || !addScopes.length}
            >{busy ? 'Saving…' : 'Save grant'}</button
          >
          <button type="button" disabled={busy} onclick={() => void cancelAdd()}>Cancel</button>
        </div>
      </form>
    {:else if full}
      <p class="repo-small repo-muted">
        This key has reached the limit of {grantLimit} project grants. Revoke one to add another.
      </p>
    {:else if !(fresh && grants.length === 0)}
      <div class="actions">
        <button
          type="button"
          bind:this={addButton}
          disabled={busy}
          aria-label={`Add project to key ${short}`}
          onclick={() => void startAdd()}><Plus size={14} aria-hidden="true" />Add project</button
        >
      </div>
    {/if}
  {/if}

  {#if !isAgent}
    <div class="mode-switch">
      {#if mode === 'granted'}
        {#if confirmingAll}
          <div class="confirm" role="group" aria-label={`Confirm all projects for key ${short}`}>
            <p>
              <strong>Allow every project you can access?</strong> A copy of this key file could then
              open any of them, with any permission your role allows, including review decisions. Grants
              you saved are kept but not used.
            </p>
            <div class="actions">
              <button
                type="button"
                bind:this={cancelAllButton}
                disabled={busy}
                onclick={() => void cancelAll()}>Cancel</button
              >
              <button type="button" class="danger" disabled={busy} onclick={() => void allowAll()}
                >{busy ? 'Saving…' : 'Allow all projects'}</button
              >
            </div>
          </div>
        {:else}
          <button
            type="button"
            bind:this={allowButton}
            disabled={busy}
            aria-label={`Allow all projects for key ${short}`}
            onclick={() => void askAll()}>Allow all projects</button
          >
        {/if}
      {:else}
        <button
          type="button"
          class="repo-primary"
          disabled={busy}
          aria-label={`Use only granted projects for key ${short}`}
          onclick={() => void limitToGranted()}>Use only projects I grant</button
        >
      {/if}
    </div>
  {/if}
</div>

<style>
  .access {
    margin: 10px 0 12px;
    padding: 12px;
    border: 1px solid var(--rp-border-muted);
    border-radius: 6px;
    background: var(--rp-subtle);
  }
  .access:focus {
    outline: none;
  }
  .access-head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 12px;
    margin-bottom: 8px;
  }
  .mode {
    color: var(--rp-fg);
    border-color: var(--rp-success-border);
    background: var(--rp-success-bg);
  }
  .mode.broad {
    border-color: var(--rp-attention-border);
    background: var(--rp-attention-bg);
  }
  .live {
    min-height: 0;
  }
  .live :global(.repo-notice) {
    margin-bottom: 10px;
  }
  .agent-note {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 0 0 10px;
    padding: 8px 10px;
    border: 1px solid var(--rp-accent-border);
    border-radius: 6px;
    background: var(--rp-accent-bg);
    color: var(--rp-fg);
  }
  .agent-note :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--rp-accent);
  }
  .broad-note {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 0 0 10px;
    padding: 8px 10px;
    border: 1px solid var(--rp-attention-border);
    border-radius: 6px;
    background: var(--rp-attention-bg);
    color: var(--rp-fg);
  }
  .broad-note :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--rp-attention);
  }
  .next-step {
    margin: 0 0 10px;
    padding: 10px 12px;
    border: 1px solid var(--rp-accent-border);
    border-radius: 6px;
    background: var(--rp-accent-bg);
    color: var(--rp-fg);
  }
  .next-step p {
    margin: 0 0 8px;
  }
  .next-step .next-title {
    font-weight: 600;
  }
  .grants {
    margin: 0 0 10px;
    padding: 0;
    list-style: none;
    border: 1px solid var(--rp-border);
    border-radius: 6px;
    background: var(--rp-canvas);
  }
  .grant {
    padding: 10px 12px;
  }
  .grant + .grant {
    border-top: 1px solid var(--rp-border-muted);
  }
  .grant-head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 8px;
    margin-bottom: 6px;
  }
  .grant-name {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .grant-name.muted {
    color: var(--rp-muted);
    font-weight: 400;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0 0 8px;
    padding: 0;
    list-style: none;
  }
  .chip {
    padding: 0 8px;
    border: 1px solid var(--rp-border);
    border-radius: 999px;
    background: var(--rp-subtle);
    color: var(--rp-fg);
    font-size: 12px;
    line-height: 20px;
  }
  .chip.idle {
    border-style: dashed;
    color: var(--rp-muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 8px;
  }
  .grant .actions {
    margin-top: 4px;
  }
  .add {
    margin: 0 0 10px;
    padding: 12px;
    border: 1px solid var(--rp-border);
    border-radius: 6px;
    background: var(--rp-canvas);
  }
  .scopes {
    margin: 8px 0;
    padding: 0;
    border: 0;
    min-width: 0;
  }
  .scopes legend {
    padding: 0;
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .access label.scope {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 8px 0;
    font-weight: 400;
  }
  .scope input {
    flex: none;
    margin-top: 3px;
  }
  .scope-text {
    display: grid;
    min-width: 0;
  }
  .scope-name {
    font-weight: 600;
  }
  .mode-switch {
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--rp-border-muted);
  }
  .confirm {
    padding: 10px 12px;
    border: 1px solid var(--rp-attention-border);
    border-radius: 6px;
    background: var(--rp-attention-bg);
    color: var(--rp-fg);
  }
  .confirm p {
    margin: 0;
  }
  .danger {
    border-color: var(--rp-danger-border);
    background: var(--rp-danger-bg);
    color: var(--rp-fg);
  }
</style>
