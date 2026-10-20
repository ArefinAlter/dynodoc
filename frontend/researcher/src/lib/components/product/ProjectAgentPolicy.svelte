<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { Bot, OctagonX } from '@lucide/svelte';
  import {
    CALLS_PER_MINUTE,
    KIND_OPTIONS,
    MB_RANGE,
    ROLE_FLOORS,
    agentMessage,
    dailyTotals,
    draftOf,
    draftProblems,
    fileLabel,
    floorHelp,
    formatBytes,
    formatDay,
    hasProblems,
    isConflict,
    isUnavailable,
    killAgents,
    killMessage,
    loadPolicy,
    loadUsage,
    overrideLabel,
    savePolicy,
    savedMessage,
    sameDraft,
    type AgentKind,
    type AgentPolicy,
    type DayTotal,
    type PolicyDraft
  } from '$lib/workspace/agent-policy';

  let {
    projectId,
    canManage,
    fileTitles = {}
  }: {
    projectId: string;
    /** Owner or manager. The server refuses everyone else, so nothing is asked of it for them. */
    canManage: boolean;
    /** File names by id, for the list of per-file settings; an unknown file shows its id. */
    fileTitles?: Record<string, string>;
  } = $props();
  const uid = $props.id();

  let policy = $state<AgentPolicy | null>(null);
  let loadState = $state<'loading' | 'ready' | 'failed' | 'unavailable'>('loading');
  let draft = $state<PolicyDraft>({
    enabled: false,
    minRole: 'viewer',
    kinds: KIND_OPTIONS.map((k) => k.id),
    calls: CALLS_PER_MINUTE.fallback,
    megabytes: 20
  });
  let attempted = $state(false);
  let busy = $state(false);
  let error = $state('');
  let notice = $state('');
  let confirming = $state(false);
  let days = $state<DayTotal[]>([]);
  let usageTruncated = $state(false);
  let usageState = $state<'loading' | 'ready' | 'failed'>('loading');
  let usageError = $state('');
  let kindBoxes = $state<HTMLInputElement[]>([]),
    callsInput = $state<HTMLInputElement | null>(null),
    megabytesInput = $state<HTMLInputElement | null>(null),
    stopButton = $state<HTMLButtonElement | null>(null),
    cancelButton = $state<HTMLButtonElement | null>(null);

  const problems = $derived(draftProblems(draft));
  const dirty = $derived(policy !== null && !sameDraft(draft, draftOf(policy)));

  /** Fetch the policy and make the form show it. */
  async function refresh() {
    policy = await loadPolicy(projectId);
    draft = draftOf(policy);
    attempted = false;
  }
  async function loadUsageTable() {
    usageState = 'loading';
    try {
      const usage = await loadUsage(projectId);
      days = dailyTotals(usage.items);
      usageTruncated = usage.truncated;
      usageState = 'ready';
    } catch (e) {
      usageError = agentMessage(e, 'usage');
      usageState = 'failed';
    }
  }
  async function load() {
    loadState = 'loading';
    error = '';
    try {
      await refresh();
      loadState = 'ready';
    } catch (e) {
      error = agentMessage(e, 'load');
      loadState = isUnavailable(e) ? 'unavailable' : 'failed';
    }
  }
  onMount(() => {
    if (!canManage) return;
    void load();
    void loadUsageTable();
  });

  const toggleKind = (kind: AgentKind) => {
    draft.kinds = draft.kinds.includes(kind)
      ? draft.kinds.filter((k) => k !== kind)
      : [...draft.kinds, kind];
  };

  async function save() {
    if (busy || !policy) return;
    attempted = true;
    if (hasProblems(problems)) {
      await tick();
      (problems.kinds ? kindBoxes[0] : problems.calls ? callsInput : megabytesInput)?.focus();
      return;
    }
    busy = true;
    error = '';
    notice = '';
    try {
      const saved = await savePolicy(projectId, policy.revision, draft);
      policy = saved;
      draft = draftOf(saved);
      attempted = false;
      notice = savedMessage(saved);
    } catch (e) {
      error = agentMessage(e, 'save');
      // Someone else saved first: show what is saved now rather than a stale form.
      if (isConflict(e)) {
        try {
          await refresh();
        } catch {
          error += ' The latest settings could not be loaded. Reload the page to see them.';
        }
      }
    } finally {
      busy = false;
    }
  }

  async function askKill() {
    error = '';
    notice = '';
    confirming = true;
    await tick();
    cancelButton?.focus();
  }
  async function cancelKill() {
    confirming = false;
    await tick();
    stopButton?.focus();
  }
  async function stopAgents() {
    if (busy) return;
    busy = true;
    error = '';
    notice = '';
    try {
      const result = await killAgents(projectId);
      confirming = false;
      notice = killMessage(result);
      try {
        // Keep any edits the person had made, except that agents are now off.
        const fresh = await loadPolicy(projectId);
        draft = policy ? { ...draft, enabled: fresh.enabled } : draftOf(fresh);
        policy = fresh;
        loadState = 'ready';
      } catch {
        notice += ' The settings below could not be refreshed. Reload the page to see them.';
      }
    } catch (e) {
      error = agentMessage(e, 'kill');
    } finally {
      busy = false;
    }
    await tick();
    stopButton?.focus();
  }
  function escape(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.stopPropagation();
      void cancelKill();
    }
  }
  const describedBy = (help: string, problem: string | null) =>
    problem && attempted ? `${help} ${help}-problem` : help;
</script>

<section class="agents" id="ai-agents" aria-labelledby={`${uid}-title`}>
  <h3 id={`${uid}-title`}>AI agents</h3>
  <div class="repo-box repo-panel-body repo-form">
    {#if !canManage}
      <p class="repo-muted">
        Only a project owner or manager can see or change this project’s AI agent settings.
      </p>
    {:else}
      <p>
        An AI agent is an assistant, such as Claude, that a person connects with a key made for it
        (Profile, then <a href="/profile#signing-keys">Signing keys</a>). It can read this project’s
        files only while you have turned agents on here, and only through a key that has been given
        this project. An AI agent can only ever propose changes, which a person reviews; in this
        release it can only read.
      </p>
      {#if policy}
        <p class="gateway" class:off={!policy.gateway}>
          <Bot size={16} aria-hidden="true" />
          <span
            >{#if policy.gateway}AI agents are turned on in this installation.{:else}AI agents are
              not available on this installation yet. You can prepare these settings now; they have
              no effect until your operator turns the agent connector on.{/if}</span
          >
        </p>
      {/if}

      <!-- No role here: the project page already has its own status region, and a second one
           would be announced twice. -->
      <div aria-live="polite" class="live">
        {#if notice}<p class="repo-notice">{notice}</p>{/if}
      </div>
      {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}

      {#if loadState === 'loading'}
        <p class="repo-muted">Loading AI agent settings…</p>
      {:else if loadState !== 'unavailable'}
        {#if loadState === 'ready' && policy}
          <form
            novalidate
            aria-label="AI agent settings"
            onsubmit={(event) => {
              event.preventDefault();
              void save();
            }}
          >
            <label class="switch">
              <input type="checkbox" role="switch" bind:checked={draft.enabled} disabled={busy} />
              <span class="switch-text"
                ><span class="switch-name"
                  >Let AI agents read this project
                  <span class="switch-state" aria-hidden="true">{draft.enabled ? 'On' : 'Off'}</span
                  ></span
                ><span class="repo-small repo-muted"
                  >Off until an owner or manager turns it on. While it is off, every request from an
                  AI agent for this project is refused.</span
                ></span
              >
            </label>

            <label
              >Lowest role that can use an AI agent<select
                bind:value={draft.minRole}
                disabled={busy}
                aria-describedby={`${uid}-floor`}
              >
                {#each ROLE_FLOORS as role (role.id)}
                  <option value={role.id}>{role.label}</option>
                {/each}
              </select></label
            >
            <p class="repo-small repo-muted" id={`${uid}-floor`}>{floorHelp(draft.minRole)}</p>

            <fieldset class="kinds" disabled={busy}>
              <legend>Kinds of file an AI agent may read</legend>
              {#each KIND_OPTIONS as kind, index (kind.id)}
                <label>
                  <input
                    type="checkbox"
                    checked={draft.kinds.includes(kind.id)}
                    bind:this={kindBoxes[index]}
                    aria-describedby={attempted && problems.kinds
                      ? `${uid}-kinds-problem`
                      : undefined}
                    onchange={() => toggleKind(kind.id)}
                  />
                  {kind.label}
                </label>
              {/each}
              {#if attempted && problems.kinds}
                <p class="repo-small field-problem" id={`${uid}-kinds-problem`}>{problems.kinds}</p>
              {/if}
            </fieldset>

            <label
              >Reads per minute for each AI agent key<input
                type="number"
                inputmode="numeric"
                min={CALLS_PER_MINUTE.min}
                max={CALLS_PER_MINUTE.max}
                step="1"
                bind:value={draft.calls}
                bind:this={callsInput}
                disabled={busy}
                aria-invalid={attempted && !!problems.calls}
                aria-describedby={describedBy(`${uid}-calls`, problems.calls)}
              /></label
            >
            <p class="repo-small repo-muted" id={`${uid}-calls`}>
              From {CALLS_PER_MINUTE.min} to {CALLS_PER_MINUTE.max}. Past the limit an agent is told
              to wait, and the wait ends by itself.
            </p>
            {#if attempted && problems.calls}
              <p class="repo-small field-problem" id={`${uid}-calls-problem`}>{problems.calls}</p>
            {/if}

            <label
              >Data read per day for each AI agent key, in MB<input
                type="number"
                inputmode="decimal"
                min={MB_RANGE.min}
                max={MB_RANGE.max}
                step="any"
                bind:value={draft.megabytes}
                bind:this={megabytesInput}
                disabled={busy}
                aria-invalid={attempted && !!problems.megabytes}
                aria-describedby={describedBy(`${uid}-mb`, problems.megabytes)}
              /></label
            >
            <p class="repo-small repo-muted" id={`${uid}-mb`}>
              From {MB_RANGE.min} MB to {MB_RANGE.max.toLocaleString('en')} MB a day (UTC), counted as
              the size of what is sent back. The default is 20 MB.
            </p>
            {#if attempted && problems.megabytes}
              <p class="repo-small field-problem" id={`${uid}-mb-problem`}>{problems.megabytes}</p>
            {/if}

            <div class="actions">
              <button class="repo-primary" disabled={busy}
                >{busy && !confirming ? 'Saving…' : 'Save AI agent settings'}</button
              >
              {#if dirty}<span class="repo-small repo-muted">You have unsaved changes.</span>{/if}
            </div>
          </form>

          <h4>Files with their own setting</h4>
          {#if policy.fileOverrides.length}
            <ul class="overrides" aria-label="Files with their own AI agent setting">
              {#each policy.fileOverrides as override (override.documentId)}
                <li>
                  <a href={`/projects/${projectId}/files/${override.documentId}`}
                    >{fileLabel(override.documentId, fileTitles)}</a
                  >
                  <span class="repo-small repo-muted">{overrideLabel(override.access)}</span>
                </li>
              {/each}
            </ul>
          {/if}
          <p class="repo-small repo-muted">
            {policy.fileOverrides.length
              ? policy.overridesTruncated
                ? 'The newest 100 are listed. '
                : ''
              : 'No file has its own setting. '}A single file can be hidden from AI agents, but
            changing a file’s setting is not available on this screen yet.
          </p>
        {:else}
          <!-- The usage report and the stop button below do not depend on the settings loading. -->
          <div class="actions">
            <button type="button" onclick={() => void load()}>Try again</button>
          </div>
        {/if}

        <h4 id={`${uid}-usage`}>Usage, last 7 days</h4>
        {#if usageState === 'loading'}
          <p class="repo-muted">Loading usage…</p>
        {:else if usageState === 'failed'}
          <p class="repo-alert" role="alert">{usageError}</p>
        {:else if !days.length}
          <p class="repo-muted">No AI agent activity in the last 7 days.</p>
        {:else}
          <!-- A table that scrolls sideways on a narrow screen must be reachable by keyboard. -->
          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <div class="table-wrap" role="region" aria-labelledby={`${uid}-usage`} tabindex="0">
            <table>
              <thead>
                <tr>
                  <th scope="col">Day (UTC)</th>
                  <th scope="col" class="num">Calls</th>
                  <th scope="col" class="num">Data read</th>
                  <th scope="col" class="num">Refused</th>
                </tr>
              </thead>
              <tbody>
                {#each days as total (total.day)}
                  <tr>
                    <th scope="row">{formatDay(total.day)}</th>
                    <td class="num">{total.calls.toLocaleString('en')}</td>
                    <td class="num">{formatBytes(total.bytesOut)}</td>
                    <td class="num">{total.refused.toLocaleString('en')}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
          <p class="repo-small repo-muted">
            Refused counts requests turned away because agents were off, a key had no access or a
            limit was used up. A burst of refusals from one key is counted once a minute.
            {#if usageTruncated}Only the first 500 rows were returned, so these totals may be
              incomplete.{/if}
          </p>
        {/if}

        <h4>Stop all AI agents now</h4>
        {#if confirming}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div class="confirm" role="group" aria-labelledby={`${uid}-confirm`} onkeydown={escape}>
            <p id={`${uid}-confirm`}><strong>Stop all AI agents in this project now?</strong></p>
            <p>This takes effect at once and does the following:</p>
            <ul>
              <li>Turns AI agents off for this project.</li>
              <li>
                Ends every AI agent key’s access to this project, and every open agent session.
              </li>
              <li>Leaves people’s own keys untouched.</li>
            </ul>
            <p>
              Turning AI agents on again does not bring the old access back. Each AI agent key must
              be given this project again, and its agent must start a new session.
            </p>
            <div class="actions">
              <button
                type="button"
                bind:this={cancelButton}
                disabled={busy}
                onclick={() => void cancelKill()}>Cancel</button
              >
              <button type="button" class="danger" disabled={busy} onclick={() => void stopAgents()}
                >{busy ? 'Stopping…' : 'Yes, stop all AI agents'}</button
              >
            </div>
          </div>
        {:else}
          <p>
            Use this if an AI agent is doing something you do not want. Your own work and other
            people’s keys are not affected.
          </p>
          <button
            type="button"
            class="danger"
            bind:this={stopButton}
            disabled={busy}
            onclick={() => void askKill()}
            ><OctagonX size={14} aria-hidden="true" />Stop all AI agents now</button
          >
        {/if}
      {/if}
    {/if}
  </div>
</section>

<style>
  .agents {
    margin: 0 0 16px;
  }
  .live :global(.repo-notice) {
    margin-bottom: 10px;
  }
  .gateway {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 0 0 12px;
    padding: 8px 10px;
    border: 1px solid var(--rp-success-border);
    border-radius: 6px;
    background: var(--rp-success-bg);
    color: var(--rp-fg);
  }
  .gateway.off {
    border-color: var(--rp-attention-border);
    background: var(--rp-attention-bg);
  }
  .gateway :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .agents :global(label.switch) {
    align-items: flex-start;
  }
  .switch-text {
    display: grid;
    min-width: 0;
  }
  .switch-name {
    font-weight: 600;
  }
  .switch-state {
    margin-left: 6px;
    padding: 0 7px;
    border: 1px solid var(--rp-border);
    border-radius: 999px;
    font-size: 12px;
    font-weight: 500;
    line-height: 20px;
  }
  .kinds {
    margin: 12px 0;
    padding: 0;
    border: 0;
    min-width: 0;
  }
  .kinds legend {
    padding: 0;
    font-weight: 600;
  }
  .kinds :global(label) {
    margin: 6px 0;
  }
  .field-problem {
    margin: -6px 0 10px;
    color: var(--rp-danger);
    font-weight: 600;
  }
  .actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 8px;
  }
  .agents h4 {
    margin: 20px 0 8px;
  }
  .overrides {
    margin: 0 0 8px;
    padding: 0;
    list-style: none;
  }
  .overrides li {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 10px;
    padding: 6px 0;
    border-top: 1px solid var(--rp-border-muted);
    overflow-wrap: anywhere;
  }
  .table-wrap {
    overflow-x: auto;
    margin: 0 0 8px;
    border: 1px solid var(--rp-border);
    border-radius: 6px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    padding: 6px 10px;
    text-align: left;
    white-space: nowrap;
  }
  thead th {
    background: var(--rp-subtle);
    border-bottom: 1px solid var(--rp-border);
    font-weight: 600;
  }
  tbody tr + tr > * {
    border-top: 1px solid var(--rp-border-muted);
  }
  tbody th {
    font-weight: 400;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .confirm {
    padding: 10px 12px;
    border: 1px solid var(--rp-attention-border);
    border-radius: 6px;
    background: var(--rp-attention-bg);
    color: var(--rp-fg);
  }
  .confirm ul {
    margin: 0 0 10px;
    padding-left: 20px;
  }
  .danger {
    border-color: var(--rp-danger-border);
    background: var(--rp-danger-bg);
    color: var(--rp-fg);
  }
</style>
