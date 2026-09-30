<script lang="ts">
  import { onMount } from 'svelte';
  import {
    X,
    GitPullRequest,
    GitMerge,
    AlertCircle,
    LoaderCircle,
    Download,
    CircleX,
    GitBranch,
    RefreshCw
  } from '@lucide/svelte';
  import DiffText from './DiffText.svelte';
  import type { DocumentState, MaterializedNode } from '$lib/api/types';
  import { applyLocalChanges } from '$lib/workspace/changes';
  import { textOf, type TextBlock } from '$lib/workspace/files';
  import {
    declineRequest,
    mergeRequest,
    requestDetail,
    withdrawDraft,
    type Conflict,
    type RequestDetail
  } from '$lib/workspace/reviews';
  let {
    documentId,
    requestId,
    team,
    teamSeq,
    canMerge,
    onClose,
    onChanged,
    onOpenDraft
  }: {
    documentId: string;
    requestId: string;
    team: DocumentState;
    teamSeq: number;
    canMerge: boolean;
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
    choice = $state<Record<string, 'team' | 'draft'>>({});

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
  function merge() {
    if (!detail || detail.revision === undefined) return;
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
        note
      });
      notice = result.completed
        ? 'Merged into the team version. This request is now closed.'
        : `Merged ${nodes.length} change${nodes.length === 1 ? '' : 's'}. ${result.remaining} change${result.remaining === 1 ? '' : 's'} remain open for later.`;
      onChanged();
      await load();
    });
  }
  const person = (p?: { name: string | null; email: string } | null) =>
    p ? p.name || p.email.split('@')[0] : 'Someone';
</script>

<div class="shade" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="review-title">
    <header>
      <span class="icon"><GitPullRequest size={20} /></span>
      <div class="heading">
        <h2 id="review-title">{detail?.request.name || 'Change request'}</h2>
        {#if detail}<p>
            <span class={`status ${detail.request.status}`}>{detail.request.status}</span>
            {person(detail.request.author)} · {new Date(
              detail.request.submitted_at || detail.request.created_at
            ).toLocaleString()}{#if detail.request.source.filename}{' · from '}“{detail.request
                .source.filename}”{/if}{' · started from v'}{detail.request.source.start_seq ??
              detail.request.base_seq}
          </p>{/if}
      </div>
      <button class="close" aria-label="Close review" disabled={!!busy} onclick={onClose}
        ><X size={18} /></button
      >
    </header>
    {#if detail}
      {#if detail.request.note}<blockquote>{detail.request.note}</blockquote>{/if}
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
        {#if stale}<p class="info">
            The team version changed while you were reviewing.
            <button class="link" onclick={() => void load()}
              ><RefreshCw size={13} /> Refresh comparison</button
            >
          </p>{/if}
        {#if rows.length}
          <div class="toolbar">
            <strong>{rows.length} change{rows.length === 1 ? '' : 's'}</strong>
            {#if canMerge}<button class="link" onclick={() => (included = rows.map((r) => r.id))}
                >Select all</button
              ><button class="link" onclick={() => (included = [])}>Select none</button>{/if}
          </div>
          <ol class="rows">
            {#each rows as row (row.id)}<li class:conflict={row.conflicts.length}>
                <div class="row-head">
                  {#if canMerge}<input
                      type="checkbox"
                      aria-label="Include this change"
                      value={row.id}
                      bind:group={included}
                    />{/if}
                  <span class={`badge ${row.conflicts.length ? 'overlap' : row.kind}`}
                    >{row.conflicts.length
                      ? 'Overlaps a team change'
                      : row.formatting
                        ? 'Formatting'
                        : row.kind}</span
                  >
                </div>
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
                        disabled={!canMerge}
                        bind:group={choice[row.id]}
                      /><small>Keep the team's current version</small>
                      <p dir="auto">{readable(row.conflicts[0].team)}</p></label
                    >
                    <label class:chosen={choice[row.id] === 'draft'}
                      ><input
                        type="radio"
                        name={`c-${row.id}`}
                        value="draft"
                        disabled={!canMerge}
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
      <footer>
        {#if detail.request.source.upload_id}<a
            class="secondary"
            href={`/api/engine/documents/${documentId}/uploads/${detail.request.source.upload_id}`}
            download><Download size={14} /> Their file</a
          >{/if}
        {#if detail.request.mine && detail.request.status !== 'merged'}<button
            class="secondary"
            onclick={() => onOpenDraft(detail!.request.id)}
            ><GitBranch size={14} /> Open my draft</button
          >{/if}
        {#if detail.request.status === 'open'}
          {#if canMerge}<textarea
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
          {#if canMerge}<button
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
              disabled={!!busy || stale || !included.length || unresolved > 0}
              title={unresolved ? 'Choose a version for each overlapping change' : undefined}
              onclick={merge}><GitMerge size={15} /> Merge {included.length} selected</button
            >{:else}<p class="hint">An editor of this document reviews and merges changes.</p>{/if}
        {/if}
      </footer>
      {#if unresolved && canMerge}<p class="hint">
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
  .shade {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
    backdrop-filter: blur(4px);
    z-index: 100;
    display: grid;
    place-items: center;
    padding: 24px;
  }
  .dialog {
    background: var(--color-surface-raised);
    width: 900px;
    max-width: 100%;
    max-height: 92vh;
    overflow: auto;
    padding: 24px 28px;
    border-radius: 12px;
    box-shadow: var(--shadow-dialog);
    font-family: var(--font-ui);
    color: var(--color-ink);
  }
  header {
    display: flex;
    gap: 14px;
    align-items: start;
  }
  .heading {
    min-width: 0;
  }
  header h2 {
    margin: 2px 0 6px;
    font-size: 19px;
    font-weight: 600;
    color: var(--color-ink-strong);
    overflow-wrap: anywhere;
  }
  header p {
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
  footer {
    display: flex;
    flex-wrap: wrap;
    justify-content: end;
    align-items: center;
    gap: 10px;
    margin-top: 18px;
  }
  footer textarea {
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
