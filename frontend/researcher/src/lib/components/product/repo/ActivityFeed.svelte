<script lang="ts">
  import {
    GitMerge,
    GitPullRequest,
    GitPullRequestClosed,
    GitCommitHorizontal
  } from '@lucide/svelte';
  import { activityTone, kindLabel, type ActivityItem } from '$lib/workspace/activity';
  import { groupByDay } from '$lib/workspace/repo-ui';
  import Avatar from './Avatar.svelte';
  import RelTime from './RelTime.svelte';
  type FeedItem = ActivityItem & { project_id?: string; project_name?: string };
  let {
    items,
    projectId = '',
    showActor = true
  }: { items: FeedItem[]; projectId?: string; showActor?: boolean } = $props();
  const groups = $derived(groupByDay(items, (i) => i.at));
  const fileHref = (item: FeedItem) => {
    const project = item.project_id || projectId;
    return project && item.document_id ? `/projects/${project}/files/${item.document_id}` : '';
  };
</script>

{#each groups as group (group.key)}
  <section class="day">
    <h3>{group.label}</h3>
    <ul>
      {#each group.items as item}
        {@const tone = activityTone(item.kind)}
        {@const href = fileHref(item)}
        <li>
          <span class={`icon ${tone}`} aria-hidden="true">
            {#if tone === 'merge'}<GitMerge
                size={15}
              />{:else if tone === 'closed'}<GitPullRequestClosed
                size={15}
              />{:else if tone === 'request'}<GitPullRequest size={15} />{:else}<GitCommitHorizontal
                size={15}
              />{/if}
          </span>
          <div class="text">
            <p>
              {#if showActor && item.actor_name}<strong>{item.actor_name}</strong
                >{' '}{/if}{item.summary ||
                kindLabel(item.kind)}{#if item.document_title}{' in '}{#if href}<a {href}
                    >{item.document_title}</a
                  >{:else}<strong>{item.document_title}</strong
                  >{/if}{/if}{#if item.project_name}{' · '}<span class="muted"
                  >{item.project_name}</span
                >{/if}
            </p>
            <p class="meta">
              {#if item.actor_name && showActor}<Avatar name={item.actor_name} size={16} />{/if}
              <RelTime at={item.at} />{#if item.seq !== null && href}{' · '}<a
                  href={`${href}?tab=history`}>r{item.seq}</a
                >{/if}
            </p>
          </div>
        </li>
      {/each}
    </ul>
  </section>
{/each}

<style>
  .day {
    margin: 0 0 18px;
  }
  .day h3 {
    margin: 0 0 6px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--rp-border);
    font-size: 13px;
    font-weight: 600;
    color: var(--rp-muted);
  }
  .day ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .day li {
    display: flex;
    gap: 12px;
    padding: 10px 6px;
    border-bottom: 1px solid var(--rp-border-muted);
    border-radius: 6px;
    transition: background-color 0.12s;
  }
  .day li:hover {
    background: var(--rp-subtle);
  }
  .day li:last-child {
    border-bottom: 0;
  }
  .icon {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--rp-neutral-bg);
    color: var(--rp-muted);
  }
  .icon.merge {
    background: var(--rp-done-bg);
    color: var(--rp-done);
  }
  .icon.request {
    background: var(--rp-success-bg);
    color: var(--rp-success);
  }
  .icon.closed {
    background: var(--rp-danger-bg);
    color: var(--rp-danger);
  }
  .text {
    min-width: 0;
  }
  .text p {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .text .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 2px;
    font-size: 12px;
    color: var(--rp-muted);
  }
  .muted {
    color: var(--rp-muted);
  }
</style>
