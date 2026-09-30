<script lang="ts">
  import { listEvents, verifyChain } from '$lib/api/client';
  import type { EventRow, VerifyResult } from '$lib/api/types';

  interface Props {
    documentId: string;
    open: boolean;
    onClose: () => void;
    /** The store's current seq — re-loads the log when it advances. */
    seq: number;
  }

  let { documentId, open, onClose, seq }: Props = $props();

  let events = $state<EventRow[]>([]);
  let verify = $state<VerifyResult | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      const [page, v] = await Promise.all([
        listEvents(documentId, { limit: 200 }),
        verifyChain(documentId)
      ]);
      events = page.items.slice().reverse();
      verify = v;
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not load history';
    } finally {
      loading = false;
    }
  }

  // Reload whenever the panel opens or the store's seq advances while open.
  $effect(() => {
    if (open) {
      void seq;
      void load();
    }
  });
</script>

<aside class:open class="history" aria-label="History and audit">
  <div class="header">
    <div>
      <h2>History</h2>
      {#if verify}
        <span class="verify {verify.ok ? 'ok' : 'broken'}">
          {verify.ok ? '✓ chain verified' : `✗ broken at seq ${verify.broken_at_seq}`}
        </span>
      {/if}
    </div>
    <button type="button" class="close" onclick={onClose} aria-label="Close history">×</button>
  </div>

  <div class="body">
    {#if loading}
      <p class="muted">Loading event log…</p>
    {:else if error}
      <p class="muted">{error}</p>
    {:else if events.length === 0}
      <p class="muted">No events yet.</p>
    {:else}
      <ol class="events">
        {#each events as event (event.id)}
          <li>
            <div class="row">
              <span class="seq">#{event.seq}</span>
              <span class="type">{event.type}</span>
            </div>
            <div class="sub">
              <span class="actor">{event.actor_id.slice(0, 8)}</span>
              {#if event.target_node_id}
                <span class="node">→ {event.target_node_id.slice(0, 8)}</span>
              {/if}
            </div>
          </li>
        {/each}
      </ol>
    {/if}
  </div>
</aside>

<style>
  .history {
    position: fixed;
    right: 1rem;
    top: 5.5rem;
    width: min(26rem, calc(100vw - 2rem));
    max-height: calc(100vh - 7rem);
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
  }

  .history.open {
    transform: translateX(0);
  }

  .header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 1rem;
    border-bottom: 1px solid var(--color-border);
  }

  h2 {
    margin: 0;
    font-family: var(--font-display);
    font-weight: 500;
  }

  .verify {
    display: inline-block;
    margin-top: 0.3rem;
    font-family: var(--font-ui);
    font-size: 0.74rem;
    padding: 0.1rem 0.5rem;
    border-radius: 999px;
  }

  .verify.ok {
    background: var(--color-suggest-bg);
    color: var(--color-suggest);
  }

  .verify.broken {
    background: color-mix(in srgb, var(--color-terracotta) 12%, var(--color-surface));
    color: var(--color-terracotta);
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
    padding: 0.5rem 0.5rem 1rem;
    font-family: var(--font-ui);
  }

  .muted {
    color: var(--color-muted);
    padding: 0.8rem;
  }

  .events {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .events li {
    padding: 0.6rem 0.8rem;
    border-bottom: 1px solid var(--color-border);
  }

  .row {
    display: flex;
    gap: 0.6rem;
    align-items: baseline;
  }

  .seq {
    font-family: var(--font-mono);
    color: var(--color-subtle);
    font-size: 0.78rem;
  }

  .type {
    font-weight: 600;
    font-size: 0.9rem;
  }

  .sub {
    display: flex;
    gap: 0.6rem;
    color: var(--color-muted);
    font-size: 0.76rem;
    margin-top: 0.2rem;
  }

  .actor,
  .node {
    font-family: var(--font-mono);
  }
</style>
