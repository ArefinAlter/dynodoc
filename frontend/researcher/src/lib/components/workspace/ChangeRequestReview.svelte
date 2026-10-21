<script lang="ts">
  import { onMount, tick } from 'svelte';
  import {
    X,
    GitPullRequest,
    GitMerge,
    AlertCircle,
    LoaderCircle,
    Download,
    CircleX,
    GitBranch,
    RefreshCw,
    CircleCheck,
    MessageSquare,
    TriangleAlert,
    Bot,
    Send
  } from '@lucide/svelte';
  import DiffText from './DiffText.svelte';
  import AgentBadge from './AgentBadge.svelte';
  import type { DocumentState, MaterializedNode } from '$lib/api/types';
  import { applyLocalChanges } from '$lib/workspace/changes';
  import { textOf, type TextBlock } from '$lib/workspace/files';
  import {
    declineRequest,
    mergeRequest,
    requestDetail,
    submitDraft,
    withdrawDraft,
    type Conflict,
    type RequestDetail
  } from '$lib/workspace/reviews';
  import { percent, submitReview, type Review } from '$lib/workspace/collaboration';
  import {
    changedWhileReading,
    clientName,
    isRequestChanged
  } from '$lib/workspace/agent-proposals';
  let {
    documentId,
    requestId,
    team,
    teamSeq,
    canMerge,
    canComment = true,
    onClose,
    onChanged,
    onOpenDraft
  }: {
    documentId: string;
    requestId: string;
    team: DocumentState;
    teamSeq: number;
    canMerge: boolean;
    /** Contributors and above comment; viewers only read. */
    canComment?: boolean;
    onClose: () => void;
    onChanged: () => void;
    onOpenDraft: (id: string) => void;
  } = $props();
  let detail = $state<RequestDetail | null>(null),
    busy = $state(''),
    error = $state(''),
    notice = $state(''),
    note = $state(''),
    included = $state<string[]>([]),
    choice = $state<Record<string, 'team' | 'draft'>>({}),
    reviewText = $state(''),
    adoptNote = $state(''),
    commentOn = $state<string | null>(null),
    reviewBox = $state<HTMLTextAreaElement | null>(null),
    // Set when the request moved while the person was reading it; says what they were doing.
    moved = $state<'' | 'approve' | 'adopt' | 'review'>(''),
    movedBox = $state<HTMLElement | null>(null);
  const review = $derived(detail?.review);
  // The document's review rules can reserve merging or require approvals first.
  const mayMerge = $derived(canMerge && (review?.can_merge ?? true));
  const needsApproval = $derived(!!review && !review.satisfied);
  const unrelated = $derived(detail?.request.source.unrelated === true);
  const agent = $derived(detail?.request.agent);
  const statement = $derived(detail?.agent_statement);
  // An AI agent's draft that its person has not sent for review yet: only they can open it.
  const isDraft = $derived(
    !!detail && detail.request.status === 'open' && detail.request.submitted_at === null
  );
  const canAdopt = $derived(isDraft && !!agent && detail?.request.mine === true);
  // The stored file's id comes from the request, which its author wrote: only an id (a UUID)
  // may become part of the address, never a path, a query or a scheme.
  const uploadId = $derived.by(() => {
    const id = detail?.request.source.upload_id;
    return typeof id === 'string' &&
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id)
      ? id
      : '';
  });
  const statusWord = $derived(isDraft ? 'draft' : (detail?.request.status ?? ''));

  interface Row {
    id: string;
    kind: 'added' | 'edited' | 'removed' | 'moved';
    before: string;
    after: string;
    formatting: boolean;
    conflicts: Conflict[];
  }
  const text = (node?: MaterializedNode | null) =>
    node
      ? node.current_fields.content
        ? textOf(node.current_fields.content as TextBlock)
        : String(node.current_fields.label ?? '')
      : '';
  function readable(value: unknown): string {
    if (value === null || value === undefined) return '(not present)';
    if (typeof value === 'string') return value;
    if (typeof value === 'object' && value && 'current_fields' in value)
      return (value as MaterializedNode).deleted
        ? '(removed)'
        : text(value as MaterializedNode) || '(empty)';
    if (typeof value === 'object' && value && 'type' in value) return textOf(value as TextBlock);
    return JSON.stringify(value);
  }
  const rows = $derived.by((): Row[] => {
    if (!detail?.ops) return [];
    let merged: DocumentState = team;
    try {
      merged = applyLocalChanges(team, detail.ops);
    } catch {
      merged = team;
    }
    const ids = [
      ...new Set([
        ...detail.ops.flatMap((op) => ('node_id' in op && op.node_id ? [op.node_id] : [])),
        ...(detail.conflicts || []).map((c) => c.node_id)
      ])
    ];
    return ids.map((id) => {
      const before = team.nodes[id],
        after = merged.nodes[id];
      const conflicts = (detail!.conflicts || []).filter((c) => c.node_id === id);
      const draftNode = detail!.nodes?.[id]?.draft;
      const kind: Row['kind'] =
        (!before || before.deleted) && after && !after.deleted
          ? 'added'
          : before && !before.deleted && (!after || after.deleted)
            ? 'removed'
            : text(before) === text(after) &&
                JSON.stringify(before?.current_fields.content) ===
                  JSON.stringify(after?.current_fields.content) &&
                (before?.pos !== after?.pos || before?.parent_id !== after?.parent_id)
              ? 'moved'
              : 'edited';
      return {
        id,
        kind: conflicts.length && draftNode?.deleted ? 'removed' : kind,
        before: text(before),
        after: conflicts.length ? text(draftNode) : text(after),
        formatting: kind === 'edited' && text(before) === text(after),
        conflicts
      };
    });
  });
  const unresolved = $derived(
    rows.filter((r) => r.conflicts.length && included.includes(r.id) && !choice[r.id]).length
  );
  const stale = $derived(!!detail && detail.team_seq !== teamSeq);
  /** Decide every overlapping paragraph at once, e.g. for a different file uploaded under this name. */
  function chooseAll(side: 'team' | 'draft') {
    const next = { ...choice };
    for (const row of rows) if (row.conflicts.length) next[row.id] = side;
    choice = next;
    included = [
      ...new Set([...included, ...rows.filter((r) => r.conflicts.length).map((r) => r.id)])
    ];
  }
  function commentOnRow(id: string) {
    commentOn = id;
    reviewBox?.focus();
  }
  const rowComments = (id: string) =>
    (review?.reviews || []).filter((r: Review) => r.node_id === id && r.note);
  /**
   * The request changed after the version this person was reading (the server says so and
   * records nothing). Show them the new version, keep what they typed, and say so calmly.
   */
  async function showMoved(doing: 'approve' | 'adopt' | 'review') {
    moved = doing;
    await load();
    await tick();
    movedBox?.focus();
  }
  function sendReview(verdict: Review['verdict']) {
    // The review is given for the version on screen, and for no other.
    const seen = detail?.revision;
    if (seen === undefined) return;
    void run(verdict === 'approved' ? 'Approving…' : 'Sending your review…', async () => {
      try {
        await submitReview(documentId, requestId, {
          verdict,
          note: reviewText.trim(),
          node_id: commentOn,
          revision: seen
        });
      } catch (e) {
        if (!isRequestChanged(e)) throw e;
        await showMoved(verdict === 'approved' ? 'approve' : 'review');
        return;
      }
      moved = '';
      reviewText = '';
      commentOn = null;
      onChanged();
      await load();
      notice =
        verdict === 'approved'
          ? 'Approved. The author has been notified.'
          : 'Review sent. The author has been notified.';
    });
  }
  async function load() {
    busy = 'Loading the comparison…';
    error = '';
    try {
      detail = await requestDetail(documentId, requestId);
      included = rows.map((r) => r.id);
      choice = {};
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not load this change request.';
    } finally {
      busy = '';
    }
  }
  onMount(load);
  async function run(label: string, fn: () => Promise<void>) {
    busy = label;
    error = '';
    notice = '';
    try {
      await fn();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Something went wrong. Please try again.';
    } finally {
      busy = '';
    }
  }
  /** The person an AI agent acts for sends its draft for review as themselves. */
  function adopt() {
    const seen = detail?.revision;
    if (seen === undefined) return;
    void run('Sending it for review…', async () => {
      try {
        await submitDraft(documentId, requestId, adoptNote.trim(), seen);
      } catch (e) {
        if (!isRequestChanged(e)) throw e;
        await showMoved('adopt');
        return;
      }
      moved = '';
      adoptNote = '';
      onChanged();
      await load();
      notice =
        'Sent for review. It stays marked as the AI agent’s work, so you cannot approve it yourself; another reviewer decides.';
    });
  }
  /** What the agent called one of its changes, from the number it was given in its call. */
  const changeLabel = (r: { call: number; i: number | null }) =>
    `${(statement?.calls ?? 1) > 1 ? `Round ${r.call}, change` : 'Change'} ${r.i === null ? '' : r.i + 1}`.trim();
  function merge() {
    if (!detail || detail.revision === undefined) return;
    const override = needsApproval && !!review?.can_override;
    if (
      override &&
      !confirm(
        `This request doesn't have the ${review!.required_approvals} approval(s) the review rules require. Merge anyway as the owner or a manager? This is recorded in the audit log.`
      )
    )
      return;
    const resolutions: Record<string, string> = {};
    for (const row of rows)
      for (const c of row.conflicts)
        resolutions[c.key] = included.includes(row.id) ? choice[row.id] || 'team' : 'team';
    const nodes = included.filter((id) => {
      const row = rows.find((r) => r.id === id);
      return !row?.conflicts.length || choice[id] === 'draft';
    });
    void run('Merging the selected changes…', async () => {
      const result = await mergeRequest(documentId, requestId, {
        team_seq: detail!.team_seq,
        revision: detail!.revision!,
        resolutions,
        included_nodes: nodes,
        note,
        override_rules: override
      });
      notice = result.completed
        ? 'Merged into the team version. This request is now closed.'
        : `Merged ${nodes.length} change${nodes.length === 1 ? '' : 's'}. ${result.remaining} change${result.remaining === 1 ? '' : 's'} remain open for later.`;
      onChanged();
      await load();
    });
  }
  const person = (p?: { name: string | null; email: string | null } | null) =>
    p ? p.name || p.email?.split('@')[0] || 'Deleted account' : 'Someone';
</script>

<div class="shade" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="review-title">
    <div class="top">
      <span class="icon"><GitPullRequest size={20} /></span>
      <div class="heading">
        <h2 id="review-title">{detail?.request.name || 'Change request'}</h2>
        {#if detail}<p>
            <span class={`status ${statusWord}`}>{statusWord}</span>
            {#if agent}<AgentBadge />{' '}{/if}{person(detail.request.author)} · {new Date(
              detail.request.submitted_at || detail.request.created_at
            ).toLocaleString()}{#if detail.request.source.filename}{' · from '}“{detail.request
                .source.filename}”{/if}{' · started from v'}{detail.request.source.start_seq ??
              detail.request.base_seq}
          </p>{/if}
      </div>
      <button class="close" aria-label="Close review" disabled={!!busy} onclick={onClose}
        ><X size={18} /></button
      >
    </div>
    {#if detail}
      {#if detail.request.source.kind === 'history_recovery'}
        <p>
          {detail.request.source.mode === 'revert'
            ? `Revert of revisions ${Number(detail.request.source.from_seq) + 1}–${detail.request.source.through_seq}`
            : detail.request.source.mode === 'cherry_pick'
              ? `Cherry-pick of revisions ${Number(detail.request.source.from_seq) + 1}–${detail.request.source.through_seq}`
              : `Recovery of content from revision ${detail.request.source.through_seq}`}. Merging
          adds new changes and preserves the intervening history.
        </p>
      {/if}
      {#if agent}<section class="agent" aria-labelledby="agent-title">
          <h3 id="agent-title"><Bot size={16} aria-hidden="true" /> Proposed by an AI agent</h3>
          <p>
            <span dir="auto">{agent.display_name || 'An AI agent'}</span> proposed this through
            {clientName(agent.client_kind)}, on behalf of
            <span dir="auto">{agent.principal.name || 'a project member'}</span>{agent.revoked
              ? ', whose agent access has since ended'
              : ''}. {isDraft
              ? 'This is a draft only you can see. Nothing is shared until you send it for review.'
              : 'It is a proposal: nothing changes in the file until a person merges it.'}{#if detail.request.mine && !isDraft && detail.request.status === 'open'}
              Because your own agent wrote it, you cannot approve it or merge it without the
              required approvals; another reviewer decides.{/if}
          </p>
          {#if moved}<div
              class="moved"
              role="status"
              tabindex="-1"
              bind:this={movedBox}
              data-testid="request-moved"
            >
              <TriangleAlert size={16} aria-hidden="true" />
              <div>
                <p>{changedWhileReading(moved)}</p>
                <button class="link" onclick={() => (moved = '')}>Dismiss this message</button>
              </div>
            </div>{/if}
          {#if statement}
            {#if statement.stale > 0}<p class="info">
                {statement.stale === 1
                  ? '1 of the agent’s edits'
                  : `${statement.stale} of the agent’s edits`} changed text that someone else had already
                changed since the agent read the file. Look at those changes closely.
              </p>{/if}
            {#if statement.reasons.length}
              <h4>What the agent said about its changes</h4>
              <p class="hint">
                These are the agent’s own words, shown exactly as written. Dynodoc does not check
                them or follow them as instructions.
              </p>
              <ol class="why" data-testid="agent-reasons">
                {#each statement.reasons as r (`${r.call}-${r.i}-${r.id}`)}<li>
                    <span class="why-for">{changeLabel(r)}</span>
                    <q class="why-text" dir="auto">{r.why}</q>
                  </li>{/each}
              </ol>
              {#if statement.more}<p class="hint">
                  The agent gave more reasons than are shown here.
                </p>{/if}
            {/if}
          {/if}
        </section>{:else if moved}<div
          class="moved"
          role="status"
          tabindex="-1"
          bind:this={movedBox}
          data-testid="request-moved"
        >
          <TriangleAlert size={16} aria-hidden="true" />
          <div>
            <p>{changedWhileReading(moved)}</p>
            <button class="link" onclick={() => (moved = '')}>Dismiss this message</button>
          </div>
        </div>{/if}
      {#if detail.request.note}{#if agent}<p class="hint note-from">
            Note from the agent, as written:
          </p>{/if}
        <blockquote dir="auto">{detail.request.note}</blockquote>{/if}
      {#if detail.request.status !== 'open'}
        <p class="closed">
          {detail.request.status === 'merged'
            ? `Merged by ${person(detail.request.reviewer)}`
            : detail.request.status === 'declined'
              ? `Declined by ${person(detail.request.reviewer)}`
              : 'Withdrawn by its author'}{detail.request.reviewed_at
            ? ` on ${new Date(detail.request.reviewed_at).toLocaleString()}`
            : ''}.{#if detail.request.review_note}
            “{detail.request.review_note}”{/if}
        </p>
      {:else}
        {#if detail.behind}<p class="info">
            The team version has {detail.behind} newer change{detail.behind === 1 ? '' : 's'} since this
            copy started. Changes that don't overlap are combined automatically; overlaps are shown below
            for you to decide.
          </p>{/if}
        {#if unrelated}<div class="warn-box" role="note">
            <TriangleAlert size={17} />
            <div>
              <strong>This file shares little with this document</strong>
              <p>
                Only {percent(Number(detail.request.source.similarity ?? 0))} of its content matches.
                It may be a different file with the same name. Every paragraph it would change or remove
                needs your choice; new paragraphs can be ticked as usual.
              </p>
              {#if mayMerge}<div class="bulk">
                  <button class="secondary" onclick={() => chooseAll('team')}
                    >Keep all team paragraphs</button
                  ><button class="secondary" onclick={() => chooseAll('draft')}
                    >Use all of theirs</button
                  >
                </div>{/if}
            </div>
          </div>{/if}
        {#if stale}<p class="info">
            The team version changed while you were reviewing.
            <button class="link" onclick={() => void load()}
              ><RefreshCw size={13} /> Refresh comparison</button
            >
          </p>{/if}
        {#if rows.length}
          <div class="toolbar">
            <strong>{rows.length} change{rows.length === 1 ? '' : 's'}</strong>
            {#if mayMerge}<button class="link" onclick={() => (included = rows.map((r) => r.id))}
                >Select all</button
              ><button class="link" onclick={() => (included = [])}>Select none</button>{/if}
          </div>
          <ol class="rows">
            {#each rows as row (row.id)}<li class:conflict={row.conflicts.length}>
                <div class="row-head">
                  {#if mayMerge}<input
                      type="checkbox"
                      aria-label="Include this change"
                      value={row.id}
                      bind:group={included}
                    />{/if}
                  <span class={`badge ${row.conflicts.length ? 'overlap' : row.kind}`}
                    >{row.conflicts.length
                      ? row.conflicts[0].field === '$block'
                        ? 'Replaces a team paragraph'
                        : 'Overlaps a team change'
                      : row.formatting
                        ? 'Formatting'
                        : row.kind}</span
                  >{#if canComment && detail.request.status === 'open'}<button
                      class="link row-comment"
                      onclick={() => commentOnRow(row.id)}
                      ><MessageSquare size={13} /> Comment</button
                    >{/if}
                </div>
                {#each rowComments(row.id) as c (c.id)}<p class="row-note">
                    <b>{person(c.reviewer)}:</b>
                    {c.note}
                  </p>{/each}
                {#if row.conflicts.length}
                  <div class="three">
                    <div>
                      <small>When they started</small>
                      <p dir="auto">{readable(row.conflicts[0].ancestor)}</p>
                    </div>
                    <label class:chosen={choice[row.id] === 'team'}
                      ><input
                        type="radio"
                        name={`c-${row.id}`}
                        value="team"
                        disabled={!mayMerge}
                        bind:group={choice[row.id]}
                      /><small>Keep the team's current version</small>
                      <p dir="auto">{readable(row.conflicts[0].team)}</p></label
                    >
                    <label class:chosen={choice[row.id] === 'draft'}
                      ><input
                        type="radio"
                        name={`c-${row.id}`}
                        value="draft"
                        disabled={!mayMerge}
                        bind:group={choice[row.id]}
                      /><small>Use {person(detail.request.author)}’s version</small>
                      <p dir="auto">{readable(row.conflicts[0].draft)}</p></label
                    >
                  </div>
                {:else if row.kind === 'added'}<p class="text added-text" dir="auto">
                    {row.after || 'Empty paragraph'}
                  </p>
                {:else if row.kind === 'removed'}<p class="text removed-text" dir="auto">
                    {row.before || 'Empty paragraph'}
                  </p>
                {:else}<DiffText before={row.before} after={row.after} />{/if}
              </li>{/each}
          </ol>
        {:else}<p class="info">
            Nothing left to merge — the team version already contains these changes.
          </p>{/if}
      {/if}
      {#if review}<section class="reviews" aria-labelledby="reviews-title">
          <h3 id="reviews-title">Reviews</h3>
          <p class="rule">
            {#if review.required_approvals}{review.approvals} of {review.required_approvals} required
              approval{review.required_approvals === 1 ? '' : 's'}{:else}No approvals required{#if review.approvals}
                · {review.approvals} approval{review.approvals === 1
                  ? ''
                  : 's'}{/if}{/if}{#if review.changes_requested_by.length}
              · changes requested{/if}{#if !review.can_merge && canMerge}
              · only the owner and managers merge here{/if}
          </p>
          <ul>
            {#each review.reviews as r (r.id)}<li class:stale={r.stale}>
                <span class={`verdict ${r.verdict}`}
                  >{r.verdict === 'approved'
                    ? 'Approved'
                    : r.verdict === 'changes_requested'
                      ? 'Changes requested'
                      : 'Comment'}</span
                >
                <b>{person(r.reviewer)}</b>
                <small
                  >{new Date(r.created_at).toLocaleString()}{r.stale
                    ? ' · before the latest edits'
                    : ''}{r.node_id ? ' · on one change' : ''}</small
                >
                {#if r.note}<p>{r.note}</p>{/if}
              </li>{/each}
          </ul>
          {#if detail.request.status === 'open' && !isDraft && (canComment || review.can_approve)}<div
              class="review-form"
            >
              {#if commentOn}<p class="hint">
                  Commenting on one change. <button class="link" onclick={() => (commentOn = null)}
                    >Comment on the whole request instead</button
                  >
                </p>{/if}
              <textarea
                bind:this={reviewBox}
                aria-label="Review comment"
                bind:value={reviewText}
                rows="2"
                maxlength="4000"
                placeholder={review.can_approve
                  ? 'Approve, or explain what should change'
                  : 'Add a comment for the author and reviewers'}
              ></textarea>
              <div class="review-actions">
                {#if review.can_approve}<button
                    class="primary"
                    disabled={!!busy}
                    onclick={() => sendReview('approved')}><CircleCheck size={14} /> Approve</button
                  ><button
                    class="secondary"
                    disabled={!!busy || !reviewText.trim()}
                    onclick={() => sendReview('changes_requested')}>Request changes</button
                  >{/if}<button
                  class="secondary"
                  disabled={!!busy || !reviewText.trim()}
                  onclick={() => sendReview('commented')}
                  ><MessageSquare size={14} /> Comment</button
                >
              </div>
            </div>{/if}
        </section>{/if}
      <div class="actions">
        {#if uploadId}<a
            class="secondary"
            href={`/api/engine/documents/${encodeURIComponent(documentId)}/uploads/${encodeURIComponent(uploadId)}`}
            download><Download size={14} /> Their file</a
          >{/if}
        {#if detail.request.mine && detail.request.status !== 'merged'}<button
            class="secondary"
            onclick={() => onOpenDraft(detail!.request.id)}
            ><GitBranch size={14} /> Open my draft</button
          >{/if}
        {#if canAdopt}
          <textarea
            aria-label="Note for the reviewers"
            bind:value={adoptNote}
            rows="1"
            maxlength="4000"
            placeholder="Optional note for the reviewers"
          ></textarea>
          <button
            class="secondary"
            disabled={!!busy}
            title="Closes the draft as withdrawn. It stays in the history and nothing is sent."
            onclick={() =>
              void run('Discarding the draft…', async () => {
                await withdrawDraft(documentId, requestId);
                onChanged();
                await load();
                notice =
                  'Discarded. The draft stays in the history as withdrawn; nothing was sent for review.';
              })}>Discard this draft</button
          >
          <button class="primary" disabled={!!busy} onclick={adopt}
            ><Send size={14} aria-hidden="true" /> Adopt and send for review</button
          >
        {:else if detail.request.status === 'open'}
          {#if mayMerge}<textarea
              aria-label="Review note"
              bind:value={note}
              rows="1"
              maxlength="4000"
              placeholder="Optional note to the author"
            ></textarea>{/if}
          {#if detail.request.mine}<button
              class="secondary"
              disabled={!!busy}
              onclick={() =>
                void run('Withdrawing…', async () => {
                  await withdrawDraft(documentId, requestId);
                  onChanged();
                  await load();
                })}>Withdraw</button
            >{/if}
          {#if mayMerge}<button
              class="secondary danger"
              disabled={!!busy}
              onclick={() =>
                void run('Declining…', async () => {
                  await declineRequest(documentId, requestId, note);
                  onChanged();
                  await load();
                })}><CircleX size={14} /> Decline</button
            ><button
              class="primary"
              disabled={!!busy ||
                stale ||
                !included.length ||
                unresolved > 0 ||
                (needsApproval && !review?.can_override)}
              title={unresolved
                ? 'Choose a version for each overlapping change'
                : needsApproval && !review?.can_override
                  ? 'Waiting for the approvals the review rules require'
                  : undefined}
              onclick={merge}
              ><GitMerge size={15} />
              {needsApproval && review?.can_override ? 'Merge without approvals' : 'Merge'}
              {included.length} selected</button
            >{:else}<p class="hint">
              {canMerge
                ? 'Only the owner and managers merge change requests in this document.'
                : 'An editor of this document reviews and merges changes.'}
            </p>{/if}
        {/if}
      </div>
      {#if unresolved && mayMerge}<p class="hint">
          Choose which version to keep for {unresolved} overlapping change{unresolved === 1
            ? ''
            : 's'}.
        </p>{/if}
    {/if}
    {#if busy}<p class="busy" role="status"><LoaderCircle size={15} />{busy}</p>{/if}
    {#if notice}<p class="notice" role="status">{notice}</p>{/if}
    {#if error}<p class="error" role="alert"><AlertCircle size={15} />{error}</p>{/if}
  </div>
</div>

<style>
  .agent {
    margin-top: 14px;
    padding: 12px 14px;
    border: 1px solid var(--color-accent-border);
    border-radius: 10px;
    background: var(--color-accent-bg);
    font-size: 13px;
    line-height: 1.6;
  }
  .agent h3 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 4px;
    font-size: 14px;
    color: var(--color-ink-strong);
  }
  .agent h4 {
    margin: 12px 0 2px;
    font-size: 13px;
    color: var(--color-ink-strong);
  }
  .agent p {
    margin: 4px 0;
    color: var(--color-ink);
  }
  .agent .info {
    background: var(--color-surface-raised);
  }
  .why {
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 6px;
  }
  .why li {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 10px;
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--color-surface-raised);
  }
  .why-for {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-ink-soft);
  }
  .why-text {
    flex: 1 1 14rem;
    min-width: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    color: var(--color-ink);
  }
  .moved {
    display: flex;
    gap: 10px;
    margin: 10px 0 0;
    padding: 10px 12px;
    border: 1px solid var(--color-warn-border);
    border-radius: 8px;
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
    font-size: 13px;
  }
  .moved :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .moved p {
    margin: 0 0 4px;
    line-height: 1.55;
    color: var(--color-ink);
  }
  .moved:focus {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }
  .note-from {
    margin: 12px 0 0;
  }
  .warn-box {
    display: flex;
    gap: 10px;
    margin-top: 14px;
    padding: 12px 14px;
    border: 1px solid var(--color-warn-border);
    border-radius: 10px;
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
    font-size: 13px;
  }
  .warn-box p {
    margin: 4px 0 0;
    line-height: 1.55;
    color: var(--color-ink);
  }
  .warn-box strong {
    color: var(--color-ink-strong);
  }
  .bulk {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 10px;
  }
  .row-comment {
    margin-left: auto;
    display: inline-flex;
    gap: 4px;
    align-items: center;
  }
  .row-note {
    margin: 6px 0 0;
    padding: 6px 10px;
    border-left: 3px solid var(--color-accent-border);
    background: var(--color-surface-sunken);
    font-size: 12.5px;
  }
  .reviews {
    margin-top: 18px;
    padding-top: 14px;
    border-top: 1px solid var(--color-border);
  }
  .reviews h3 {
    margin: 0 0 4px;
    font-size: 15px;
    color: var(--color-ink-strong);
  }
  .reviews .rule {
    margin: 0 0 8px;
    font-size: 13px;
    color: var(--color-ink-soft);
  }
  .reviews ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .reviews li {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 10px;
    align-items: baseline;
    font-size: 13px;
  }
  .reviews li.stale {
    opacity: 0.65;
  }
  .reviews li p {
    flex-basis: 100%;
    margin: 0;
    white-space: pre-wrap;
  }
  .reviews small {
    color: var(--color-muted);
  }
  .verdict {
    font-size: 11.5px;
    padding: 1px 8px;
    border-radius: 10px;
    background: var(--color-surface-sunken);
    color: var(--color-ink-soft);
  }
  .verdict.approved {
    background: var(--color-success-bg);
    color: var(--color-success);
  }
  .verdict.changes_requested {
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
  }
  .review-form {
    display: grid;
    gap: 8px;
    margin-top: 12px;
  }
  .review-form textarea {
    font: inherit;
    font-size: 13px;
    padding: 8px 10px;
    border: 1px solid var(--color-border-strong);
    border-radius: 8px;
    background: var(--color-surface-raised);
    color: var(--color-ink);
    resize: vertical;
  }
  .review-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .shade {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
    -webkit-backdrop-filter: blur(4px);
    backdrop-filter: blur(4px);
    z-index: 100;
    display: grid;
    place-items: center;
    grid-template-columns: minmax(0, 1fr);
    padding: 24px;
  }
  .dialog {
    background: var(--color-surface-raised);
    width: 900px;
    max-width: 100%;
    max-height: 92vh;
    max-height: 92dvh;
    overflow: auto;
    padding: 24px 28px;
    border-radius: 12px;
    box-shadow: var(--shadow-dialog);
    font-family: var(--font-ui);
    color: var(--color-ink);
  }
  .top {
    display: flex;
    gap: 14px;
    align-items: start;
  }
  .heading {
    min-width: 0;
  }
  .top h2 {
    margin: 2px 0 6px;
    font-size: 19px;
    font-weight: 600;
    color: var(--color-ink-strong);
    overflow-wrap: anywhere;
  }
  .top p {
    margin: 0;
    font-size: 13px;
    color: var(--color-ink-soft);
    line-height: 1.6;
  }
  .icon {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--color-accent-bg);
    color: var(--color-accent);
  }
  .close {
    margin-left: auto;
    border: 0;
    background: none;
    color: var(--color-ink-soft);
    cursor: pointer;
    padding: 4px;
  }
  .status {
    display: inline-block;
    font-size: 11px;
    text-transform: capitalize;
    padding: 1px 8px;
    border-radius: 10px;
    margin-right: 4px;
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
  }
  .status.merged {
    background: var(--color-purple-bg);
    color: var(--color-purple);
  }
  .status.declined,
  .status.withdrawn {
    background: var(--color-surface-sunken);
    color: var(--color-ink-soft);
  }
  blockquote {
    margin: 16px 0 0;
    padding: 10px 14px;
    border-left: 3px solid var(--color-accent-border);
    background: var(--color-surface-sunken);
    font-size: 13px;
    white-space: pre-wrap;
  }
  .info,
  .closed,
  .hint {
    font-size: 13px;
    line-height: 1.65;
    color: var(--color-ink-soft);
  }
  .info {
    background: var(--color-surface-sunken);
    padding: 10px 12px;
    border-radius: 8px;
  }
  .toolbar {
    display: flex;
    gap: 14px;
    align-items: baseline;
    margin: 18px 0 8px;
    font-size: 13px;
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 0;
    background: none;
    padding: 0;
    color: var(--color-accent);
    font: inherit;
    font-size: 13px;
    text-decoration: underline;
    text-underline-offset: 3px;
    cursor: pointer;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: 10px;
  }
  .rows li {
    padding: 12px 14px;
    border-bottom: 1px solid var(--color-border-soft);
    display: grid;
    gap: 8px;
  }
  .rows li:last-child {
    border-bottom: 0;
  }
  .rows li.conflict {
    background: var(--color-warn-bg);
  }
  .row-head {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .badge {
    font-size: 11px;
    text-transform: capitalize;
    padding: 2px 8px;
    border-radius: 10px;
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
  }
  .badge.added {
    background: var(--color-diff-add-bg);
    color: var(--color-diff-add-ink);
  }
  .badge.removed {
    background: var(--color-diff-del-bg);
    color: var(--color-diff-del-ink);
  }
  .badge.overlap {
    background: color-mix(in srgb, var(--color-warn-border) 50%, var(--color-warn-bg));
    color: var(--color-warn-ink);
    text-transform: none;
  }
  .text {
    margin: 0;
    font-size: 13px;
    line-height: 1.7;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .added-text {
    background: var(--color-diff-add-bg);
    color: var(--color-diff-add-ink);
    padding: 6px 8px;
    border-radius: 6px;
  }
  .removed-text {
    background: var(--color-diff-del-bg);
    color: var(--color-diff-del-ink);
    text-decoration: line-through;
    padding: 6px 8px;
    border-radius: 6px;
  }
  .three {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .three > div,
  .three label {
    border: 1px solid var(--color-warn-border);
    border-radius: 8px;
    padding: 9px 10px;
    font-size: 13px;
    background: var(--color-surface-raised);
  }
  .three > div {
    background: var(--color-surface-sunken);
  }
  .three label {
    cursor: pointer;
  }
  .three label.chosen {
    border-color: var(--color-accent);
    background: var(--color-surface-selected);
  }
  .three small {
    font-size: 12px;
    color: var(--color-ink-soft);
  }
  .three p {
    margin: 6px 0 0;
    line-height: 1.6;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: end;
    align-items: center;
    gap: 10px;
    margin-top: 18px;
  }
  .actions textarea {
    flex: 1;
    min-width: 200px;
    font: inherit;
    font-size: 13px;
    padding: 8px 10px;
    border: 1px solid var(--color-border-strong);
    border-radius: 8px;
    color: var(--color-ink-strong);
    background: var(--color-surface-raised);
    resize: vertical;
  }
  .primary,
  .secondary {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    border-radius: 8px;
    padding: 9px 14px;
    font: inherit;
    font-size: 13px;
    cursor: pointer;
    text-decoration: none;
  }
  .primary {
    border: 0;
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
  }
  .secondary {
    border: 1px solid var(--color-border-strong);
    background: var(--color-surface-raised);
    color: var(--color-accent-strong);
  }
  .secondary.danger {
    color: var(--color-danger);
    border-color: var(--color-danger-border);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .busy,
  .error,
  .notice {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 13px;
    margin: 12px 0 0;
  }
  .busy {
    color: var(--color-accent);
  }
  .notice {
    color: var(--color-success);
    background: var(--color-success-bg);
    padding: 9px 12px;
    border-radius: 8px;
  }
  .error {
    color: var(--color-danger);
  }
  @media (max-width: 700px) {
    .dialog {
      padding: 18px 14px;
    }
    .three {
      grid-template-columns: 1fr;
    }
  }
</style>
