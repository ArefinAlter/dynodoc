<script lang="ts">
  import { Check, CircleAlert } from '@lucide/svelte';
  import type { ReviewGroup } from '$lib/workspace/project-history';
  import type { MemberLite } from '$lib/workspace/activity';
  import { displayName, plural, shortHash } from '$lib/workspace/repo-ui';
  import RelTime from './RelTime.svelte';
  let {
    groups,
    branch,
    required,
    members = [],
    loading = false,
    problem = '',
    more = false,
    skipped = 0,
    onMore
  }: {
    groups: ReviewGroup[];
    branch: string;
    /** Approvals the project rules ask for on this branch; null when none are needed. */
    required: number | null;
    members?: MemberLite[];
    loading?: boolean;
    problem?: string;
    more?: boolean;
    skipped?: number;
    onMore?: () => void;
  } = $props();
  const people = $derived(new Map(members.map((m) => [m.id, m]))),
    who = (id: string) => {
      const member = people.get(id);
      return member ? displayName(member.name, member.email) : `Reviewer ${id.slice(0, 8)}`;
    };
</script>

<section class="reviews" aria-labelledby="reviews-title" aria-busy={loading}>
  <h4 id="reviews-title">Reviews of this commit</h4>
  {#if problem}
    <p class="problem" aria-live="polite">{problem}</p>
  {:else if loading}
    <p class="faint">Loading reviews…</p>
  {:else if !groups.length}
    <p class="faint">No reviews have been recorded for this commit on {branch}.</p>
  {:else}
    {#each groups as group (`${group.generation}:${group.commit ?? ''}`)}
      <div class="group">
        <p class="base">
          Reviewed against <strong>generation {group.generation.toLocaleString()}</strong
          >{#if group.commit}{' '}<code>{shortHash(group.commit)}</code>{:else}, a branch with no
            earlier head{/if}
        </p>
        <p class="tally">
          {#if required === null}
            {plural(group.approvals, 'approval')} · {plural(group.changeRequests, 'change request')}
            · approvals are not required to move this branch
          {:else}
            {group.approvals} of {required} required {required === 1 ? 'approval' : 'approvals'}
            · {plural(group.changeRequests, 'change request')}
          {/if}
        </p>
        <ul>
          {#each group.rows as row (row.id)}
            <li class:old={!row.current}>
              <span class={`mark ${row.decision}`} aria-hidden="true">
                {#if row.decision === 'approve'}<Check size={14} />{:else}<CircleAlert
                    size={14}
                  />{/if}
              </span>
              <span class="line">
                {#if people.has(row.reviewer)}<a href={`/profiles/${row.reviewer}`}
                    >{who(row.reviewer)}</a
                  >{:else}{who(row.reviewer)}{/if}
                {row.decision === 'approve' ? 'approved' : 'requested changes'}
                <RelTime at={row.createdAt} />{#if !row.current}<span class="faint"
                    >{' '}· replaced by a later decision</span
                  >{/if}
              </span>
            </li>
          {/each}
        </ul>
      </div>
    {/each}
    <p class="faint small">
      Counts are the recorded decisions. When a branch moves, the server also checks that each
      reviewer is independent of the publisher and still holds a reviewing role.
    </p>
    {#if skipped}<p class="faint small">
        {plural(skipped, 'review')} could not be displayed.
      </p>{/if}
    {#if more && onMore}<button type="button" onclick={onMore}>Show more reviews</button>{/if}
  {/if}
</section>

<style>
  .reviews {
    padding: 12px 16px;
    border: 1px solid var(--rp-border);
    border-radius: 6px;
    background: var(--rp-canvas);
  }
  h4 {
    margin: 0 0 8px;
    font-size: 14px;
  }
  p {
    margin: 0 0 6px;
  }
  .faint {
    color: var(--rp-muted);
  }
  .small {
    font-size: 12px;
  }
  .problem {
    padding: 8px 10px;
    border: 1px solid var(--rp-attention-border);
    border-radius: 6px;
    background: var(--rp-attention-bg);
  }
  .group + .group {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--rp-border-muted);
  }
  .base {
    margin-bottom: 2px;
  }
  .tally {
    margin-bottom: 8px;
    color: var(--rp-muted);
  }
  code {
    font-family: var(--rp-mono);
    font-size: 12px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 4px 0;
  }
  li.old .line {
    color: var(--rp-muted);
  }
  .line {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .line a {
    text-decoration: underline;
  }
  .mark {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
  }
  .mark.approve {
    background: var(--rp-success-bg);
    color: var(--rp-success);
  }
  .mark.request_changes {
    background: var(--rp-attention-bg);
    color: var(--rp-attention);
  }
  button {
    margin-top: 8px;
  }
</style>
