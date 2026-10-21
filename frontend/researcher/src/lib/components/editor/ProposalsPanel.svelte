<script lang="ts">
  import type { DocumentStore } from '$lib/stores/documentStore.svelte';
  import type { SuggestionState } from '$lib/api/types';

  interface Props {
    store: DocumentStore;
    open: boolean;
    onClose: () => void;
  }

  let { store, open, onClose }: Props = $props();

  interface Row {
    id: string;
    suggestion: SuggestionState;
  }

  // Pending proposals first, then resolved ones (accepted / rejected).
  const rows = $derived<Row[]>(
    Object.entries(store.state.suggestions)
      .map(([id, suggestion]) => ({ id, suggestion }))
      .sort((a, b) => Number(isPending(b.suggestion)) - Number(isPending(a.suggestion)))
  );

  function isPending(s: SuggestionState): boolean {
    return !s.accepted && !s.rejected_reason;
  }

  function describe(s: SuggestionState): string {
    const d = s.detail as { type?: string; field?: string; value?: unknown } | null;
    if (d && d.type === 'FieldEdited') {
      return `Edit "${d.field}" → ${JSON.stringify(d.value)}`;
    }
    if (d && typeof d.type === 'string') return d.type;
    return 'Proposed change';
  }

  let busyId = $state('');

  async function accept(id: string) {
    busyId = id;
    try {
      await store.accept(id);
    } finally {
      busyId = '';
    }
  }

  async function reject(id: string) {
    busyId = id;
    try {
      await store.reject(id, 'rejected by author');
    } finally {
      busyId = '';
    }
  }
</script>

<aside class:open class="proposals" aria-label="Proposals queue">
  <div class="header">
    <h2>Proposals</h2>
    <button type="button" class="close" onclick={onClose} aria-label="Close proposals">×</button>
  </div>

  <div class="body">
    {#if rows.length === 0}
      <p class="empty">
        No suggestions yet. Turn on suggesting mode and edit a field to propose one.
      </p>
    {:else}
      {#each rows as row (row.id)}
        <div class="proposal" class:resolved={!isPending(row.suggestion)}>
          <div class="meta">
            <code class="sid">{row.id.slice(0, 8)}</code>
            {#if row.suggestion.accepted}
              <span class="badge accepted">accepted</span>
            {:else if row.suggestion.rejected_reason}
              <span class="badge rejected">rejected</span>
            {:else}
              <span class="badge pending">pending</span>
            {/if}
          </div>
          <div class="desc">{describe(row.suggestion)}</div>
          {#if row.suggestion.target_node_id}
            <div class="target">on {row.suggestion.target_node_id.slice(0, 8)}</div>
          {/if}
          {#if isPending(row.suggestion)}
            <div class="actions">
              <button
                type="button"
                class="accept"
                disabled={busyId === row.id}
                onclick={() => accept(row.id)}>Accept</button
              >
              <button
                type="button"
                class="reject"
                disabled={busyId === row.id}
                onclick={() => reject(row.id)}>Reject</button
              >
            </div>
          {:else if row.suggestion.rejected_reason}
            <div class="reason">{row.suggestion.rejected_reason}</div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</aside>

<style>
  .proposals {
    position: fixed;
    right: 1rem;
    top: 5.5rem;
    width: min(26rem, calc(100vw - 2rem));
    max-height: calc(100vh - 7rem);
    max-height: calc(100dvh - 7rem);
    transform: translateX(calc(100% + 2rem));
    transition: transform 0.2s ease;
    border: 1px solid var(--color-border);
    border-radius: 1rem;
    background: var(--color-surface);
    box-shadow: var(--shadow-menu);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    z-index: 30;
    --proposal-accept: var(--color-suggest);
  }

  /* The night suggest green is too light behind on-accent text, so the filled button keeps the day shade. */
  @media screen {
    :global([data-theme='dark']) .proposals {
      --proposal-accept: #107c41;
    }
  }

  .proposals.open {
    transform: translateX(0);
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1rem;
    border-bottom: 1px solid var(--color-border);
  }

  h2 {
    margin: 0;
    font-family: var(--font-display);
    font-weight: 500;
  }

  .close {
    border: 0;
    background: transparent;
    font-size: 1.4rem;
    line-height: 1;
    color: var(--color-muted);
    cursor: pointer;
  }

  .body {
    overflow: auto;
    padding: 0.5rem;
  }

  .empty {
    color: var(--color-muted);
    font-family: var(--font-ui);
    font-size: 0.86rem;
    padding: 0.8rem;
  }

  .proposal {
    border: 1px solid var(--color-border);
    border-radius: 0.7rem;
    padding: 0.8rem;
    margin: 0.4rem;
    font-family: var(--font-ui);
  }

  .proposal.resolved {
    opacity: 0.7;
  }

  .meta {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-bottom: 0.4rem;
  }

  .sid {
    font-family: var(--font-mono);
    font-size: 0.74rem;
    color: var(--color-subtle);
  }

  .badge {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 0.1rem 0.45rem;
    border-radius: 999px;
  }

  .badge.pending {
    background: var(--color-suggest-bg);
    color: var(--color-suggest);
  }

  .badge.accepted {
    background: var(--color-accent-bg);
    color: var(--color-accent);
  }

  .badge.rejected {
    background: var(--color-surface-muted);
    color: var(--color-muted);
  }

  .desc {
    font-size: 0.92rem;
  }

  .target,
  .reason {
    color: var(--color-muted);
    font-size: 0.78rem;
    margin-top: 0.25rem;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.6rem;
  }

  .actions button {
    border: 1px solid var(--color-border);
    border-radius: 0.55rem;
    padding: 0.4rem 0.7rem;
    background: var(--color-surface);
    cursor: pointer;
  }

  .actions .accept {
    background: var(--proposal-accept);
    color: var(--color-on-accent);
    border-color: var(--proposal-accept);
  }

  .actions button:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
