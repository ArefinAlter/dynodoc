<script lang="ts">
  import { onMount } from 'svelte';
  import type { MemberLite } from '$lib/workspace/activity';
  import {
    BODY_MAX,
    LABELS_PER_ISSUE,
    TITLE_MAX,
    canOpenIssues,
    createIssue,
    issueMessage,
    type Issue,
    type LabelRow
  } from '$lib/workspace/issues';
  import { displayName } from '$lib/workspace/repo-ui';
  import { projectRoleName } from '$lib/workspace/projects';
  import LabelPill from './LabelPill.svelte';

  let {
    projectId,
    role,
    members = [],
    labels = [],
    labelsFailed = false,
    onCreated,
    onCancel
  }: {
    projectId: string;
    role: string;
    members?: MemberLite[];
    labels?: LabelRow[];
    labelsFailed?: boolean;
    onCreated: (issue: Issue) => void;
    onCancel: () => void;
  } = $props();

  let title = $state(''),
    body = $state(''),
    picked = $state<string[]>([]),
    assignee = $state(''),
    busy = $state(false),
    problem = $state(''),
    titleInput = $state<HTMLInputElement | null>(null),
    heading = $state<HTMLElement | null>(null);
  const allowed = $derived(canOpenIssues(role));

  onMount(() => {
    (allowed ? titleInput : heading)?.focus({ preventScroll: true });
  });

  async function submit() {
    if (busy || !title.trim()) return;
    busy = true;
    problem = '';
    try {
      onCreated(
        await createIssue(projectId, {
          title,
          body,
          labels: picked,
          assigneeId: assignee || null
        })
      );
    } catch (error) {
      problem = issueMessage(error, 'create');
    } finally {
      busy = false;
    }
  }
</script>

<section class="iss-new" aria-labelledby="iss-new-title">
  <div class="rp-section-head">
    <h2 id="iss-new-title" tabindex="-1" bind:this={heading}>New issue</h2>
  </div>
  {#if !allowed}
    <div class="repo-box">
      <p class="repo-empty">
        Your role ({projectRoleName(role)}) can read issues but not open them. Ask a manager for
        reviewer access to open one.
      </p>
    </div>
    <div class="repo-actions">
      <button type="button" onclick={onCancel}>Back to issues</button>
    </div>
  {:else}
    <p class="repo-muted">
      Track a question or a task about this project: a review follow-up, missing evidence or
      assigned work. Text is shown as typed, with line breaks kept.
    </p>
    <form
      class="repo-box repo-panel-body repo-form iss-form"
      onsubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <label
        >Title<input
          bind:this={titleInput}
          bind:value={title}
          required
          maxlength={TITLE_MAX}
          autocomplete="off"
          disabled={busy}
        /></label
      >
      <label
        >Description (optional)<textarea
          bind:value={body}
          maxlength={BODY_MAX}
          rows="7"
          disabled={busy}
        ></textarea></label
      >
      <label
        >Assignee<select bind:value={assignee} disabled={busy}>
          <option value="">Unassigned</option>
          {#each members as m (m.id)}<option value={m.id}>{displayName(m.name, m.email)}</option
            >{/each}
        </select></label
      >
      <fieldset class="iss-fieldset" disabled={busy}>
        <legend>Labels</legend>
        {#if labelsFailed}
          <p class="repo-muted repo-small">Labels could not be loaded, so none can be chosen.</p>
        {:else if !labels.length}
          <p class="repo-muted repo-small">This project has no labels yet.</p>
        {:else}
          <div class="iss-checklist">
            {#each labels as label (label.id)}
              <label class="iss-check">
                <input
                  type="checkbox"
                  value={label.id}
                  bind:group={picked}
                  disabled={!picked.includes(label.id) && picked.length >= LABELS_PER_ISSUE}
                />
                <LabelPill name={label.name} color={label.color} />
                {#if label.description}<span class="repo-muted repo-small">{label.description}</span
                  >{/if}
              </label>
            {/each}
          </div>
          {#if picked.length >= LABELS_PER_ISSUE}<p class="repo-muted repo-small">
              An issue can have at most {LABELS_PER_ISSUE} labels.
            </p>{/if}
        {/if}
      </fieldset>
      <div aria-live="assertive">
        {#if problem}<p class="repo-alert">{problem}</p>{/if}
      </div>
      <div class="repo-actions">
        <button class="repo-primary" disabled={busy || !title.trim()}
          >{busy ? 'Creating…' : 'Create issue'}</button
        >
        <button type="button" disabled={busy} onclick={onCancel}>Cancel</button>
      </div>
    </form>
  {/if}
</section>
