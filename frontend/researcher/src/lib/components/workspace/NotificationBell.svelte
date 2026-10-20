<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import {
    Bell,
    Copy,
    FileWarning,
    GitPullRequest,
    GitMerge,
    CircleCheck,
    CircleX,
    UserPlus,
    Crown,
    Send,
    LoaderCircle
  } from '@lucide/svelte';
  import { dismissable } from '$lib/actions/dismissable';
  import {
    describe,
    listNotifications,
    markRead,
    respondToCopy,
    unreadCount,
    type NotificationItem
  } from '$lib/workspace/collaboration';

  let { size = 18 }: { size?: number } = $props();
  let open = $state(false),
    items = $state<NotificationItem[]>([]),
    unread = $state(0),
    more = $state(false),
    loading = $state(false),
    error = $state(''),
    button = $state<HTMLButtonElement | null>(null);

  async function count() {
    try {
      unread = (await unreadCount()).unread;
    } catch {
      /* Signed out or offline; the bell stays as it was. */
    }
  }
  async function load(before?: string) {
    loading = true;
    error = '';
    try {
      const page = await listNotifications(before);
      items = before ? [...items, ...page.items] : page.items;
      more = page.more;
      unread = page.unread;
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not load notifications.';
    } finally {
      loading = false;
    }
  }
  onMount(() => {
    void count();
    const timer = setInterval(() => void count(), 60000);
    const focus = () => void count();
    window.addEventListener('focus', focus);
    return () => {
      clearInterval(timer);
      window.removeEventListener('focus', focus);
    };
  });
  async function toggle() {
    open = !open;
    if (open) await load();
  }
  async function read(item: NotificationItem) {
    if (item.read) return;
    item.read = true;
    unread = Math.max(0, unread - 1);
    await markRead([item.id]).catch(() => {});
  }
  async function readAll() {
    await markRead('all').catch(() => {});
    items = items.map((i) => ({ ...i, read: true }));
    unread = 0;
  }
  async function visit(item: NotificationItem, query = '') {
    await read(item);
    if (!item.document?.role) {
      error = 'You no longer have access to this document.';
      return;
    }
    open = false;
    await goto(`/documents/${item.document.id}${query}`);
  }
  async function answer(item: NotificationItem, action: 'invite' | 'dismiss') {
    if (!item.match) return;
    loading = true;
    error = '';
    try {
      const result = await respondToCopy(item.match.id, action);
      item.match = { ...item.match, status: result.status };
      await read(item);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not update this copy.';
    } finally {
      loading = false;
    }
  }
  function ago(date: string) {
    const minutes = Math.round((Date.now() - new Date(date).getTime()) / 60000);
    if (minutes < 1) return 'just now';
    if (minutes < 60) return `${minutes} min ago`;
    const hours = Math.round(minutes / 60);
    if (hours < 24) return `${hours} h ago`;
    return new Date(date).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  }
</script>

<div
  class="bell"
  use:dismissable={{ open, onDismiss: () => (open = false), returnFocus: () => button }}
>
  <button
    bind:this={button}
    type="button"
    class="trigger"
    aria-haspopup="dialog"
    aria-expanded={open}
    aria-label={unread ? `Notifications, ${unread} unread` : 'Notifications'}
    title="Notifications"
    onclick={() => void toggle()}
    ><Bell {size} aria-hidden="true" />{#if unread}<span class="count" aria-hidden="true"
        >{unread > 99 ? '99+' : unread}</span
      >{/if}</button
  >
  {#if open}<div class="panel" role="dialog" aria-label="Notifications">
      <header>
        <h2>Notifications</h2>
        {#if unread}<button class="link" onclick={() => void readAll()}>Mark all read</button>{/if}
      </header>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <ul>
        {#each items as item (item.id)}<li class:unread={!item.read}>
            <span class={`kind ${item.kind}`} aria-hidden="true">
              {#if item.kind === 'copy_detected'}<Copy
                  size={16}
                />{:else if item.kind === 'same_name_detected'}<FileWarning
                  size={16}
                />{:else if item.kind === 'change_request_merged'}<GitMerge
                  size={16}
                />{:else if item.kind === 'change_request_declined'}<CircleX
                  size={16}
                />{:else if item.kind === 'change_request_reviewed'}<CircleCheck
                  size={16}
                />{:else if item.kind === 'access_granted'}<UserPlus
                  size={16}
                />{:else if item.kind === 'ownership_transferred'}<Crown
                  size={16}
                />{:else if item.kind === 'copy_invitation'}<Send size={16} />{:else}<GitPullRequest
                  size={16}
                />{/if}
            </span>
            <div class="body">
              <p>{describe(item)}</p>
              <small>{ago(item.created_at)}</small>
              <div class="actions">
                {#if item.kind === 'copy_detected' || item.kind === 'same_name_detected'}
                  {#if item.match?.status === 'open'}
                    {#if item.kind === 'copy_detected'}<button
                        disabled={loading}
                        onclick={() => void answer(item, 'invite')}
                        >Invite them to send changes</button
                      >{/if}<button
                      class="quiet"
                      disabled={loading}
                      onclick={() => void answer(item, 'dismiss')}>Dismiss</button
                    >
                  {:else if item.match}<span class="status"
                      >{item.match.status === 'invited'
                        ? 'Invited to send changes'
                        : item.match.status === 'dismissed'
                          ? 'Dismissed'
                          : 'Marked as known'}</span
                    >{/if}
                  <button class="quiet" onclick={() => void visit(item, '?panel=copies')}
                    >See copies</button
                  >
                {:else if item.kind === 'copy_invitation'}
                  {#if item.source}<button
                      onclick={() => void visit(item, `?push_from=${item.source!.id}`)}
                      >Send my copy as changes</button
                    >{/if}<button class="quiet" onclick={() => void visit(item)}>Open</button>
                {:else if item.request}<button
                    onclick={() => void visit(item, `?request=${item.request!.id}`)}
                    >Open request</button
                  >
                {:else}<button class="quiet" onclick={() => void visit(item)}>Open</button>{/if}
              </div>
            </div>
            {#if !item.read}<button
                class="dot"
                aria-label="Mark as read"
                title="Mark as read"
                onclick={() => void read(item)}
              ></button>{/if}
          </li>{/each}
      </ul>
      {#if loading}<p class="state" role="status"><LoaderCircle size={15} /> Loading…</p>
      {:else if !items.length}<p class="state">
          Nothing yet. You’ll hear here when someone sends changes for review, answers yours, or
          uploads a copy of a document you own.
        </p>{/if}
      {#if more && !loading}<button
          class="more link"
          onclick={() => void load(items.at(-1)?.created_at)}>Show older</button
        >{/if}
    </div>{/if}
</div>

<style>
  .bell {
    position: relative;
    display: inline-flex;
  }
  .trigger {
    position: relative;
    display: inline-grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--color-ink-soft);
    cursor: pointer;
  }
  .trigger:hover {
    background: var(--color-surface-hover);
    color: var(--color-ink-strong);
  }
  .count {
    position: absolute;
    top: 2px;
    right: 1px;
    min-width: 17px;
    height: 17px;
    padding: 0 4px;
    border-radius: 9px;
    background: var(--color-danger);
    color: var(--color-on-accent);
    font-size: 10.5px;
    font-weight: 700;
    line-height: 17px;
  }
  /* The night danger colour is light, so the count uses the dark page colour. */
  :global([data-theme='dark']) .count {
    color: var(--color-parchment);
  }
  .panel {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    z-index: 120;
    width: min(400px, calc(100vw - 24px));
    max-height: min(560px, 75vh);
    max-height: min(560px, 75dvh);
    overflow: auto;
    background: var(--color-surface-raised);
    color: var(--color-ink);
    border: 1px solid var(--color-border);
    border-radius: 14px;
    box-shadow: var(--shadow-menu);
    font-family: var(--font-ui);
    text-align: left;
  }
  header {
    position: sticky;
    top: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px 10px;
    background: var(--color-surface-raised);
    border-bottom: 1px solid var(--color-border-soft);
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 650;
    color: var(--color-ink-strong);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    gap: 12px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--color-border-soft);
  }
  li.unread {
    background: var(--color-accent-bg);
  }
  .kind {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: var(--color-surface-sunken);
    color: var(--color-accent);
  }
  .kind.copy_detected,
  .kind.same_name_detected {
    background: var(--color-warn-bg);
    color: var(--color-warn);
  }
  .kind.change_request_merged {
    background: var(--color-purple-bg);
    color: var(--color-purple);
  }
  .body {
    flex: 1;
    min-width: 0;
  }
  .body p {
    margin: 0;
    font-size: 13.5px;
    line-height: 1.5;
    overflow-wrap: anywhere;
  }
  small {
    color: var(--color-muted);
    font-size: 12px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }
  .actions button {
    border: 1px solid var(--color-accent-solid);
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
    border-radius: 7px;
    padding: 4px 10px;
    font-size: 12.5px;
    cursor: pointer;
  }
  .actions button.quiet {
    background: transparent;
    color: var(--color-accent);
    border-color: var(--color-border-strong);
  }
  .actions button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .status {
    align-self: center;
    font-size: 12px;
    color: var(--color-ink-soft);
  }
  .dot {
    flex-shrink: 0;
    width: 10px;
    height: 10px;
    margin-top: 6px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--color-accent);
    cursor: pointer;
  }
  .link {
    border: 0;
    background: none;
    color: var(--color-accent);
    font-size: 13px;
    cursor: pointer;
    padding: 0;
  }
  .more {
    display: block;
    margin: 10px auto 14px;
  }
  .state,
  .error {
    margin: 0;
    padding: 18px 16px;
    font-size: 13px;
    line-height: 1.6;
    color: var(--color-ink-soft);
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .error {
    color: var(--color-danger);
  }
</style>
