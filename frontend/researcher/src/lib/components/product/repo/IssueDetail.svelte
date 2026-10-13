<script lang="ts">
  import { tick } from 'svelte';
  import { ArrowLeft, CircleCheck, CircleDot, MessageSquare, Pencil } from '@lucide/svelte';
  import type { MemberLite } from '$lib/workspace/activity';
  import {
    BODY_MAX,
    COMMENT_CAP,
    COMMENT_MAX,
    LABELS_PER_ISSUE,
    TITLE_MAX,
    addComment,
    getIssue,
    isAbort,
    issueMessage,
    personName,
    updateIssue,
    type Issue,
    type IssueChange,
    type IssueComment,
    type IssueState,
    type LabelRow
  } from '$lib/workspace/issues';
  import { projectRoleName } from '$lib/workspace/projects';
  import { absoluteTime, displayName, plural } from '$lib/workspace/repo-ui';
  import Avatar from './Avatar.svelte';
  import LabelPill from './LabelPill.svelte';
  import RelTime from './RelTime.svelte';

  let {
    projectId,
    number,
    members = [],
    labels = [],
    labelsFailed = false,
    onBack,
    onChanged
  }: {
    projectId: string;
    number: number;
    members?: MemberLite[];
    labels?: LabelRow[];
    labelsFailed?: boolean;
    onBack: () => void;
    /** The server's copy of the issue after a change, so the list and counters follow. */
    onChanged: (issue: Issue, previous: IssueState) => void;
  } = $props();

  let phase = $state<'loading' | 'ready' | 'failed'>('loading'),
    problem = $state(''),
    issue = $state<Issue | null>(null),
    comments = $state<IssueComment[]>([]),
    nextAfter = $state<number | null>(null),
    attempt = $state(0),
    paging = $state(false),
    busy = $state(false),
    actionProblem = $state(''),
    conflict = $state(false),
    notice = $state(''),
    editing = $state(false),
    titleDraft = $state(''),
    bodyDraft = $state(''),
    draft = $state(''),
    heading = $state<HTMLElement | null>(null),
    titleInput = $state<HTMLInputElement | null>(null),
    assigneeSelect = $state<HTMLSelectElement | null>(null),
    composer = $state<HTMLTextAreaElement | null>(null);
  let controller: AbortController | null = null;

  // One read per issue (or retry); leaving the view aborts anything in flight.
  $effect(() => {
    const project = projectId,
      wanted = number,
      mine = new AbortController();
    void attempt;
    controller = mine;
    phase = 'loading';
    problem = '';
    void (async () => {
      try {
        const thread = await getIssue(project, wanted, { signal: mine.signal });
        issue = thread.issue;
        comments = thread.comments;
        nextAfter = thread.nextAfter;
        phase = 'ready';
        await tick();
        heading?.focus({ preventScroll: true });
      } catch (error) {
        if (isAbort(error)) return;
        problem = issueMessage(error, 'issue');
        phase = 'failed';
      }
    })();
    return () => mine.abort();
  });

  const canEdit = $derived(!!issue?.viewer.canEdit),
    canComment = $derived(!!issue?.viewer.canComment),
    // Labels the picker offers: the project's labels plus any the issue already carries.
    choices = $derived.by(() => {
      const known = new Set(labels.map((l) => l.id));
      const extra = (issue?.labels ?? []).filter((l) => !known.has(l.id));
      return [...labels, ...extra.map((l) => ({ ...l, openIssueCount: 0 }))];
    }),
    candidates = $derived.by(() => {
      const list = members.map((m) => ({ id: m.id, name: displayName(m.name, m.email) }));
      const current = issue?.assignee;
      if (current && !list.some((p) => p.id === current.id))
        list.push({
          id: current.id,
          name: current.member ? current.name : `${current.name} (left the project)`
        });
      return list;
    });

  async function change(patch: IssueChange, done = '') {
    if (!issue || busy) return false;
    busy = true;
    actionProblem = '';
    conflict = false;
    notice = '';
    try {
      const previous = issue.state;
      issue = await updateIssue(projectId, number, patch);
      notice = done;
      onChanged(issue, previous);
      return true;
    } catch (error) {
      actionProblem = issueMessage(error, 'edit');
      conflict = (error as { status?: number })?.status === 409;
      return false;
    } finally {
      busy = false;
    }
  }
  function startEdit() {
    if (!issue) return;
    titleDraft = issue.title;
    bodyDraft = issue.body;
    editing = true;
    actionProblem = '';
    void tick().then(() => titleInput?.focus());
  }
  async function saveEdit() {
    if (!issue) return;
    const patch: IssueChange = { revision: issue.revision };
    const title = titleDraft.trim();
    if (title !== issue.title) patch.title = title;
    if (bodyDraft.trim() !== issue.body) patch.body = bodyDraft;
    if (patch.title === undefined && patch.body === undefined) {
      editing = false;
      return;
    }
    if (await change(patch, 'Issue updated.')) {
      editing = false;
      await tick();
      heading?.focus({ preventScroll: true });
    }
  }
  async function toggleLabel(label: { id: string }, event: Event) {
    if (!issue) return;
    const box = event.currentTarget as HTMLInputElement,
      current = issue.labels.map((l) => l.id),
      next = box.checked ? [...current, label.id] : current.filter((id) => id !== label.id);
    if (!(await change({ labels: next }, 'Labels updated.'))) box.checked = !box.checked;
  }
  async function assign(value: string) {
    if (!(await change({ assigneeId: value || null }, 'Assignee updated.')) && assigneeSelect)
      assigneeSelect.value = issue?.assignee?.id ?? '';
  }
  async function toggleState() {
    if (!issue) return;
    const closing = issue.state === 'open';
    await change(
      { state: closing ? 'closed' : 'open' },
      closing ? 'Issue closed.' : 'Issue reopened.'
    );
  }
  async function submitComment() {
    const body = draft.trim();
    if (!issue || !body || busy) return;
    busy = true;
    actionProblem = '';
    conflict = false;
    notice = '';
    try {
      const result = await addComment(projectId, number, body);
      draft = '';
      issue = { ...issue, commentCount: result.commentCount };
      // The new comment is last; show it only when the whole thread is on screen.
      if (nextAfter === null && comments.length < COMMENT_CAP)
        comments = [...comments, result.comment];
      notice = 'Comment added.';
      onChanged(issue, issue.state);
    } catch (error) {
      actionProblem = issueMessage(error, 'comment');
    } finally {
      busy = false;
      composer?.focus({ preventScroll: true });
    }
  }
  async function moreComments() {
    if (paging || nextAfter === null) return;
    paging = true;
    const mine = new AbortController();
    controller?.abort();
    controller = mine;
    try {
      const thread = await getIssue(projectId, number, { after: nextAfter, signal: mine.signal });
      const known = new Set(comments.map((c) => c.seq));
      comments = [...comments, ...thread.comments.filter((c) => !known.has(c.seq))].slice(
        0,
        COMMENT_CAP
      );
      nextAfter = comments.length >= COMMENT_CAP ? null : thread.nextAfter;
      issue = { ...thread.issue };
    } catch (error) {
      if (!isAbort(error)) actionProblem = issueMessage(error, 'issue');
    } finally {
      paging = false;
    }
  }
</script>

<section class="iss-detail" aria-label={`Issue #${number}`} aria-busy={phase === 'loading'}>
  <div class="iss-back">
    <button type="button" class="repo-linkbtn" onclick={onBack}
      ><ArrowLeft size={14} aria-hidden="true" />All issues</button
    >
  </div>

  {#if phase === 'loading'}
    <h2 tabindex="-1" bind:this={heading}>Issue #{number}</h2>
    <div class="repo-box">
      {#each [0, 1, 2] as n (n)}<div class="repo-skel-row" aria-hidden="true">
          <span class="repo-skel" style:width="{40 + ((n * 17) % 30)}%"></span>
        </div>{/each}
    </div>
  {:else if phase === 'failed' || !issue}
    <h2 tabindex="-1" bind:this={heading}>Issue #{number}</h2>
    <p class="repo-alert">{problem || 'This issue could not be loaded.'}</p>
    <div class="repo-actions">
      <button type="button" onclick={() => attempt++}>Try again</button>
      <button type="button" onclick={onBack}>Back to issues</button>
    </div>
  {:else}
    <header class="iss-header">
      {#if editing}
        <h2 class="sr-only">Edit issue #{issue.number}</h2>
        <form
          class="repo-form iss-form iss-edit-form"
          aria-label="Edit issue"
          onsubmit={(e) => {
            e.preventDefault();
            void saveEdit();
          }}
        >
          <label
            >Title<input
              bind:this={titleInput}
              bind:value={titleDraft}
              required
              maxlength={TITLE_MAX}
              autocomplete="off"
              disabled={busy}
            /></label
          >
          <label
            >Description<textarea
              bind:value={bodyDraft}
              maxlength={BODY_MAX}
              rows="7"
              disabled={busy}
            ></textarea></label
          >
          <div class="repo-actions">
            <button class="repo-primary" disabled={busy || !titleDraft.trim()}>Save</button>
            <button type="button" disabled={busy} onclick={() => (editing = false)}>Cancel</button>
            {#if conflict}<button
                type="button"
                disabled={busy}
                onclick={() => {
                  editing = false;
                  attempt++;
                }}>Reload issue</button
              >{/if}
          </div>
        </form>
      {:else}
        <div class="iss-titlebar">
          <h2 class="iss-title" tabindex="-1" bind:this={heading}>
            {issue.title} <span class="repo-muted iss-number">#{issue.number}</span>
          </h2>
          {#if canEdit}<button type="button" disabled={busy} onclick={startEdit}
              ><Pencil size={14} aria-hidden="true" />Edit</button
            >{/if}
        </div>
      {/if}
      <p class="iss-subhead">
        <span class={`iss-state-pill ${issue.state}`}>
          {#if issue.state === 'open'}<CircleDot
              size={14}
              aria-hidden="true"
            />Open{:else}<CircleCheck size={14} aria-hidden="true" />Closed{/if}
        </span>
        <span class="repo-muted">
          <strong>{personName(issue.author)}</strong> opened this issue
          <RelTime at={issue.createdAt} />
          {#if issue.state === 'closed' && issue.closedAt}
            · closed {#if issue.closedBy}by {personName(issue.closedBy)}{/if}
            <RelTime at={issue.closedAt} />
          {/if}
          · {plural(issue.commentCount, 'comment')}
        </span>
      </p>
    </header>

    <div aria-live="polite">
      {#if notice}<p class="repo-notice">{notice}</p>{/if}
    </div>
    <div aria-live="assertive">
      {#if actionProblem}<p class="repo-alert">
          {actionProblem}
          {#if conflict && !editing}<button
              type="button"
              class="repo-linkbtn"
              onclick={() => attempt++}>Reload issue</button
            >{/if}
        </p>{/if}
    </div>

    <div class="iss-grid">
      <aside class="iss-aside" aria-label="Issue details">
        <section>
          <h3>Labels</h3>
          {#if issue.labels.length}
            <p class="iss-pills">
              {#each issue.labels as label (label.id)}<LabelPill
                  name={label.name}
                  color={label.color}
                  description={label.description}
                />{/each}
            </p>
          {:else}<p class="repo-muted">None yet</p>{/if}
          {#if canEdit}
            <details class="iss-edit">
              <summary>Edit labels</summary>
              {#if labelsFailed}
                <p class="repo-muted repo-small">Labels could not be loaded.</p>
              {:else if !choices.length}
                <p class="repo-muted repo-small">This project has no labels yet.</p>
              {:else}
                <fieldset class="iss-fieldset" disabled={busy}>
                  <legend class="sr-only">Labels on this issue</legend>
                  <div class="iss-checklist">
                    {#each choices as label (label.id)}
                      {@const on = issue.labels.some((l) => l.id === label.id)}
                      <label class="iss-check">
                        <input
                          type="checkbox"
                          checked={on}
                          disabled={!on && issue.labels.length >= LABELS_PER_ISSUE}
                          onchange={(e) => void toggleLabel(label, e)}
                        />
                        <LabelPill name={label.name} color={label.color} />
                      </label>
                    {/each}
                  </div>
                </fieldset>
              {/if}
            </details>
          {/if}
        </section>
        <section>
          <h3>Assignee</h3>
          {#if canEdit}
            <select
              bind:this={assigneeSelect}
              aria-label="Assignee"
              value={issue.assignee?.id ?? ''}
              disabled={busy}
              onchange={(e) => void assign(e.currentTarget.value)}
            >
              <option value="">Unassigned</option>
              {#each candidates as person (person.id)}<option value={person.id}
                  >{person.name}</option
                >{/each}
            </select>
          {:else if issue.assignee}
            <p class="iss-person">
              <Avatar name={issue.assignee.name} size={20} />{issue.assignee
                .name}{#if !issue.assignee.member}<span class="repo-muted">
                  (left the project)</span
                >{/if}
            </p>
          {:else}<p class="repo-muted">Unassigned</p>{/if}
        </section>
        <section>
          <h3>Opened by</h3>
          <p class="iss-person">
            <Avatar name={personName(issue.author)} size={20} />{personName(issue.author)}
          </p>
          <p class="repo-muted repo-small">
            <time datetime={issue.createdAt} title={absoluteTime(issue.createdAt)}
              >{absoluteTime(issue.createdAt)}</time
            >
          </p>
        </section>
        <section>
          <h3>Your access</h3>
          <p class="repo-muted repo-small">
            {projectRoleName(issue.viewer.role)}:
            {canEdit
              ? 'you can edit, close and reopen this issue.'
              : canComment
                ? 'you can comment. Only the author, a manager or an owner can edit it.'
                : 'you can read this issue.'}
          </p>
        </section>
      </aside>

      <div class="iss-main">
        <article class="iss-comment iss-first" aria-label="Issue description">
          <header>
            <Avatar name={personName(issue.author)} size={24} />
            <strong>{personName(issue.author)}</strong>
            <span class="repo-muted">opened <RelTime at={issue.createdAt} /></span>
          </header>
          {#if issue.body}<div class="iss-text">{issue.body}</div>{:else}<p
              class="iss-text repo-muted"
            >
              No description provided.
            </p>{/if}
        </article>

        {#each comments as c (c.id)}
          <article class="iss-comment" aria-label={`Comment ${c.seq} by ${personName(c.author)}`}>
            <header>
              <Avatar name={personName(c.author)} size={24} />
              <strong>{personName(c.author)}</strong>
              <span class="repo-muted">commented <RelTime at={c.createdAt} /></span>
            </header>
            <div class="iss-text">{c.body}</div>
          </article>
        {/each}
        {#if nextAfter !== null}
          <div class="repo-pagination">
            <button type="button" disabled={paging} onclick={() => void moreComments()}
              >{paging ? 'Loading…' : 'Show more comments'}</button
            >
          </div>
        {:else if issue.commentCount > comments.length}
          <p class="repo-muted repo-small">
            Showing the first {plural(comments.length, 'comment')} of {issue.commentCount.toLocaleString()}.
          </p>
        {/if}

        {#if canComment}
          <form
            class="repo-box repo-panel-body repo-form iss-composer"
            aria-label="Comment on this issue"
            onsubmit={(e) => {
              e.preventDefault();
              void submitComment();
            }}
          >
            <label
              >Add a comment<textarea
                bind:this={composer}
                bind:value={draft}
                maxlength={COMMENT_MAX}
                rows="4"
                placeholder="Write a comment"
              ></textarea></label
            >
            <p class="repo-muted repo-small">
              Comments are plain text and cannot be edited or deleted afterwards.
            </p>
            <div class="repo-actions">
              {#if canEdit}
                <button type="button" disabled={busy} onclick={() => void toggleState()}
                  >{#if issue.state === 'open'}<CircleCheck size={15} aria-hidden="true" />Close
                    issue{:else}<CircleDot size={15} aria-hidden="true" />Reopen issue{/if}</button
                >
              {/if}
              <button class="repo-primary" disabled={busy || !draft.trim()}
                ><MessageSquare size={15} aria-hidden="true" />Comment</button
              >
            </div>
          </form>
        {:else}
          <div class="repo-box repo-panel-body">
            <p class="repo-muted">
              Your role ({projectRoleName(issue.viewer.role)}) can read this issue but not comment
              on it.
            </p>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</section>
