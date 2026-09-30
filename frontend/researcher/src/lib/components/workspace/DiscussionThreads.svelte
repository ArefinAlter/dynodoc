<script lang="ts">
  import { untrack } from 'svelte';
  import { Check, RotateCcw, MessageSquare, RefreshCw } from '@lucide/svelte';
  import { workspaceRequest as request } from '$lib/workspace/api';
  import type { EventRow } from '$lib/api/types';
  type Reply = { id: string; body: string; author: string; created_at: string };
  type Thread = Reply & {
    node_id: string | null;
    seq: number;
    revision: number;
    resolved: boolean;
    replies: Reply[];
  };
  let {
    documentId,
    refreshSeq,
    selected,
    author,
    canReply,
    onEvents
  }: {
    documentId: string;
    refreshSeq: number;
    selected: string | null;
    author: boolean;
    canReply: boolean;
    onEvents: (events: EventRow[]) => void;
  } = $props();
  let threads = $state<Thread[]>([]),
    next = $state<number | null>(null),
    error = $state(''),
    busy = $state(false),
    showResolved = $state(false);
  let replies = $state<Record<string, string>>({});
  let requestId = 0;
  async function load(more = false) {
    const id = ++requestId;
    const query = new URLSearchParams();
    if (more && next) query.set('before', String(next));
    if (selected) query.set('node', selected);
    try {
      const result = await request<{ items: Thread[]; next_before: number | null }>(
        `/documents/${documentId}/discussions?${query}`
      );
      if (id !== requestId) return;
      threads = more ? [...threads, ...result.items] : result.items;
      next = result.next_before;
      error = '';
    } catch (e) {
      if (id === requestId) error = e instanceof Error ? e.message : 'Could not load discussions';
    }
  }
  $effect(() => {
    void refreshSeq;
    void selected;
    untrack(() => {
      void load();
    });
  });
  async function change(thread: Thread, resolved?: boolean) {
    busy = true;
    error = '';
    try {
      const result = await request<{ events: EventRow[] }>(
        `/documents/${documentId}/discussions/${thread.id}`,
        resolved === undefined
          ? { revision: thread.revision, body: replies[thread.id] || '' }
          : { revision: thread.revision, resolved }
      );
      onEvents(result.events);
      replies[thread.id] = '';
      await load();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not update discussion';
    } finally {
      busy = false;
    }
  }
</script>

<div class="discussion-tools">
  <label><input type="checkbox" bind:checked={showResolved} /> Show resolved</label><button
    title="Refresh discussions"
    aria-label="Refresh discussions"
    onclick={() => void load()}><RefreshCw size={14} /></button
  >
</div>
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#each threads.filter((t) => showResolved || !t.resolved) as thread (thread.id)}
  <article class="thread" class:resolved={thread.resolved}>
    <header>
      <span class="avatar">{thread.author?.slice(0, 1).toUpperCase() || 'D'}</span>
      <div>
        <strong>{thread.author || 'Deleted account'}</strong><time
          >{new Date(thread.created_at).toLocaleString()}</time
        >
      </div>
      {#if thread.resolved}<span class="resolved-label"><Check size={13} /> Resolved</span>{/if}
    </header>
    <p class="message">{thread.body}</p>
    {#each thread.replies as reply (reply.id)}<div class="reply">
        <strong>{reply.author || 'Deleted account'}</strong>
        <p class="message">{reply.body}</p>
        <time>{new Date(reply.created_at).toLocaleString()}</time>
      </div>{/each}
    {#if canReply && !thread.resolved}<form
        onsubmit={(e) => {
          e.preventDefault();
          void change(thread);
        }}
      >
        <label class="sr-only" for={`reply-${thread.id}`}>Reply to {thread.author}</label><textarea
          id={`reply-${thread.id}`}
          bind:value={replies[thread.id]}
          placeholder="Keep the conversation going…"
          maxlength="10000"
          rows="2"
          required
        ></textarea><button class="send" disabled={busy || !(replies[thread.id] || '').trim()}
          ><MessageSquare size={13} /> Reply</button
        >
      </form>{/if}
    {#if author}<button
        class="resolve"
        disabled={busy}
        onclick={() => void change(thread, !thread.resolved)}
        >{#if thread.resolved}<RotateCcw size={13} /> Reopen{:else}<Check size={13} /> Resolve discussion{/if}</button
      >{/if}
  </article>
{/each}
{#if !threads.some((t) => showResolved || !t.resolved)}<p class="empty">
    {selected
      ? 'No discussions on this selection.'
      : 'No open discussions. Add a comment to start a conversation.'}
  </p>{/if}
{#if next}<button class="more" disabled={busy} onclick={() => void load(true)}
    >Load earlier discussions</button
  >{/if}

<style>
  .discussion-tools {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 0 18px;
    font-size: 12px;
    color: var(--color-ink-soft);
  }
  .discussion-tools label {
    display: flex;
    gap: 7px;
    align-items: center;
  }
  button {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    cursor: pointer;
    font: inherit;
    font-size: 12px;
    border: 0;
    background: none;
    color: var(--color-accent);
    padding: 7px 9px;
    border-radius: 8px;
  }
  button:hover {
    background: var(--color-surface-hover);
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .thread {
    padding: 18px;
    border: 1px solid var(--color-border-soft);
    border-radius: 16px;
    background: var(--color-surface-raised);
    margin: 0 0 14px;
    box-shadow: var(--shadow-panel);
  }
  .thread.resolved {
    background: var(--color-surface-sunken);
  }
  .thread header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .avatar {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    background: var(--color-accent-bg);
    color: var(--color-accent);
    border-radius: 50%;
    font-size: 12px;
    flex-shrink: 0;
  }
  strong {
    font-size: 12px;
    font-weight: 650;
  }
  time {
    display: block;
    font-size: 10px;
    color: var(--color-ink-soft);
    margin-top: 3px;
  }
  .message {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    color: var(--color-ink);
    font-size: 13px;
    line-height: 1.7;
    margin: 12px 0;
  }
  .reply {
    border-left: 2px solid var(--color-border);
    padding: 8px 0 8px 13px;
    margin: 14px 0;
  }
  .reply p {
    margin: 4px 0;
  }
  textarea {
    resize: vertical;
    width: 100%;
    border: 1px solid var(--color-border);
    background: var(--color-surface-sunken);
    border-radius: 10px;
    padding: 11px;
    font: inherit;
    font-size: 13px;
    color: var(--color-ink);
    min-height: 64px;
  }
  .send {
    background: var(--color-accent-bg);
    margin-top: 8px;
  }
  .resolve {
    margin-top: 12px;
  }
  .resolved-label {
    margin-left: auto;
    display: flex;
    gap: 4px;
    color: var(--color-success);
    font-size: 10px;
  }
  .error {
    color: var(--color-danger);
    font-size: 13px;
  }
  .empty {
    color: var(--color-muted);
    font-size: 13px;
    line-height: 1.7;
  }
  .more {
    width: 100%;
    justify-content: center;
    background: var(--color-surface-sunken);
  }
</style>
