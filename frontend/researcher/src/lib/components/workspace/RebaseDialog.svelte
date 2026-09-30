<script lang="ts">
  import { onMount } from 'svelte';
  import { X, GitBranch, AlertCircle, LoaderCircle } from '@lucide/svelte';
  import type { DocumentState, EventPayload, MaterializedNode } from '$lib/api/types';
  import { workspaceRequest } from '$lib/workspace/api';
  import { textOf, type TextBlock } from '$lib/workspace/files';
  import type { Conflict } from '$lib/workspace/reviews';
  let {
    documentId,
    draftId,
    draft,
    team,
    onClose,
    onDone
  }: {
    documentId: string;
    draftId: string;
    draft: DocumentState;
    team: DocumentState;
    onClose: () => void;
    onDone: (result: { state: DocumentState; revision: number; team_seq: number }) => void;
  } = $props();
  interface Preview {
    conflicts: Conflict[];
    own: EventPayload[];
    team_seq: number;
    base_seq: number;
    revision: number;
  }
  let preview = $state<Preview | null>(null),
    busy = $state(''),
    error = $state(''),
    keep = $state<Record<string, 'mine' | 'drop'>>({});
  const sync = $derived(`/documents/${documentId}/drafts/${draftId}/sync`);
  const text = (node?: MaterializedNode | null) =>
    node && !node.deleted
      ? node.current_fields.content
        ? textOf(node.current_fields.content as TextBlock)
        : String(node.current_fields.label ?? '')
      : '(removed)';
  const rows = $derived(
    preview
      ? [
          ...new Set([
            ...preview.own.flatMap((op) => ('node_id' in op && op.node_id ? [op.node_id] : [])),
            ...preview.conflicts.map((c) => c.node_id)
          ])
        ].map((id) => ({
          id,
          overlap: preview!.conflicts.some((c) => c.node_id === id),
          mine: text(draft.nodes[id]),
          theirs: text(team.nodes[id])
        }))
      : []
  );
  onMount(async () => {
    busy = 'Checking your draft against the latest team version…';
    try {
      preview = await workspaceRequest<Preview>(sync, { action: 'preview' });
      keep = Object.fromEntries(rows.map((r) => [r.id, 'mine']));
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not compare this draft.';
    } finally {
      busy = '';
    }
  });
  async function rebase() {
    if (!preview) return;
    busy = 'Replaying your changes on the latest version…';
    error = '';
    try {
      const resolutions = Object.fromEntries(
        preview.conflicts.filter((c) => keep[c.node_id] === 'mine').map((c) => [c.key, 'draft'])
      );
      const result = await workspaceRequest<{
        state: DocumentState;
        revision: number;
        team_seq: number;
      }>(sync, {
        action: 'rebase',
        team_seq: preview.team_seq,
        revision: preview.revision,
        resolutions,
        dropped_nodes: rows.filter((r) => keep[r.id] === 'drop').map((r) => r.id)
      });
      onDone(result);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not rebase this draft.';
    } finally {
      busy = '';
    }
  }
</script>

<div class="shade" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="rebase-title">
    <header>
      <span class="icon"><GitBranch size={20} /></span>
      <div>
        <h2 id="rebase-title">Rebase onto the latest team version</h2>
        <p>
          Your draft started from an older version. Rebasing moves its starting point to the newest
          team version and replays your changes on top, so reviewers see only what you changed. Drop
          any change you no longer want — it stays in the draft's history.
        </p>
      </div>
      <button class="close" aria-label="Close" disabled={!!busy} onclick={onClose}
        ><X size={18} /></button
      >
    </header>
    {#if preview}
      <p class="meta">
        Starting point v{preview.base_seq} → v{preview.team_seq}.
        {preview.conflicts.length
          ? `${new Set(preview.conflicts.map((c) => c.node_id)).size} of your changes overlap newer team edits.`
          : 'None of your changes overlap newer team edits.'}
      </p>
      {#if rows.length}<ol class="rows">
          {#each rows as row (row.id)}<li class:overlap={row.overlap}>
              <div class="texts">
                <div>
                  <small>Your version</small>
                  <p dir="auto">{row.mine}</p>
                </div>
                {#if row.overlap}<div>
                    <small>Team's newer version</small>
                    <p dir="auto">{row.theirs}</p>
                  </div>{/if}
              </div>
              <div class="choice" role="radiogroup" aria-label="Keep or drop this change">
                <label
                  ><input type="radio" value="mine" bind:group={keep[row.id]} />
                  {row.overlap ? 'Keep mine' : 'Keep'}</label
                ><label
                  ><input type="radio" value="drop" bind:group={keep[row.id]} />
                  {row.overlap ? "Use the team's" : 'Drop'}</label
                >
              </div>
            </li>{/each}
        </ol>{:else}<p class="meta">Your draft has no changes of its own yet.</p>{/if}
      <div class="actions">
        <button class="secondary" disabled={!!busy} onclick={onClose}>Cancel</button><button
          class="primary"
          disabled={!!busy}
          onclick={() => void rebase()}><GitBranch size={15} /> Rebase draft</button
        >
      </div>
    {/if}
    {#if busy}<p class="busy" role="status"><LoaderCircle size={15} />{busy}</p>{/if}
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
    --rebase-solid: #3d6b4d;
  }
  .dialog {
    background: var(--color-surface-raised);
    width: 760px;
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
  h2 {
    margin: 2px 0 6px;
    font-size: 19px;
    font-weight: 600;
    color: var(--color-ink-strong);
  }
  header p,
  .meta {
    margin: 0;
    font-size: 13px;
    line-height: 1.65;
    color: var(--color-ink-soft);
  }
  .meta {
    margin: 16px 0 10px;
  }
  .icon {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--color-success-bg);
    color: var(--color-success);
  }
  .close {
    margin-left: auto;
    border: 0;
    background: none;
    color: var(--color-ink-soft);
    cursor: pointer;
    padding: 4px;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    max-height: 380px;
    overflow: auto;
  }
  .rows li {
    display: flex;
    gap: 14px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--color-border-soft);
  }
  .rows li:last-child {
    border-bottom: 0;
  }
  .rows li.overlap {
    background: var(--color-warn-bg);
  }
  .texts {
    flex: 1;
    display: grid;
    gap: 8px;
    min-width: 0;
  }
  small {
    font-size: 12px;
    color: var(--color-ink-soft);
  }
  .texts p {
    margin: 3px 0 0;
    font-size: 13px;
    line-height: 1.6;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .choice {
    display: grid;
    gap: 6px;
    align-content: start;
    font-size: 13px;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    justify-content: end;
    gap: 10px;
    margin-top: 18px;
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
  }
  .primary {
    border: 0;
    background: var(--rebase-solid);
    color: var(--color-on-accent);
  }
  .secondary {
    border: 1px solid var(--color-border-strong);
    background: var(--color-surface-raised);
    color: var(--color-ink);
  }
  button:disabled {
    opacity: 0.5;
  }
  .busy,
  .error {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 13px;
    margin: 12px 0 0;
  }
  .busy {
    color: var(--color-success);
  }
  .error {
    color: var(--color-danger);
  }
  @media (max-width: 600px) {
    .rows li {
      flex-direction: column;
    }
    .choice {
      display: flex;
      gap: 14px;
    }
  }
  @media screen {
    :global([data-theme='dark']) .shade {
      --rebase-solid: #178548;
    }
  }
</style>
