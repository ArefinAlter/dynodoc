<script lang="ts">
  import { tick } from 'svelte';
  import { GitGraph, GitCommitHorizontal } from '@lucide/svelte';
  import type { MemberLite } from '$lib/workspace/activity';
  import { absoluteTime, plural } from '$lib/workspace/repo-ui';
  import {
    BRANCH_CAP,
    REVIEW_CAP,
    approvalsRequired,
    groupReviews,
    historyMessage,
    isAbort,
    loadBranches,
    loadCommit,
    loadReviews,
    type Branch,
    type CommitSummary,
    type Review
  } from '$lib/workspace/project-history';
  import BranchRow from './BranchRow.svelte';
  import HashChip from './HashChip.svelte';
  import ReceiptCard from './ReceiptCard.svelte';
  import RelTime from './RelTime.svelte';
  import ReviewList from './ReviewList.svelte';

  let {
    projectId,
    requiredApprovals = 0,
    members = []
  }: { projectId: string; requiredApprovals?: number; members?: MemberLite[] } = $props();

  let phase = $state<'loading' | 'ready' | 'failed'>('loading'),
    problem = $state(''),
    branches = $state<Branch[]>([]),
    next = $state<string | null>(null),
    more = $state(false),
    skipped = $state(0),
    paging = $state(false),
    selected = $state<string | null>(null),
    commit = $state<CommitSummary | null>(null),
    commitProblem = $state(''),
    commitLoading = $state(false),
    reviews = $state<Review[]>([]),
    reviewsNext = $state<number | null>(null),
    reviewsMore = $state(false),
    reviewsSkipped = $state(0),
    reviewsProblem = $state(''),
    reviewsLoading = $state(false),
    unavailable = $state<Record<string, boolean>>({}),
    attempt = $state(0);
  let panel = $state<HTMLElement | null>(null);
  let listController: AbortController | null = null,
    detailController: AbortController | null = null;

  const current = $derived(branches.find((b) => b.name === selected) ?? null),
    groups = $derived(current ? groupReviews(reviews, current.name) : []),
    ordered = $derived([...branches].sort((a, b) => Number(b.isDefault) - Number(a.isDefault)));

  // One initial read per project (or retry); leaving the tab aborts anything in flight.
  $effect(() => {
    const project = projectId,
      controller = new AbortController();
    void attempt;
    listController = controller;
    phase = 'loading';
    problem = '';
    branches = [];
    selected = null;
    void (async () => {
      try {
        const page = await loadBranches(project, { signal: controller.signal });
        branches = page.branches;
        next = page.next;
        more = page.more;
        skipped = page.skipped;
        phase = 'ready';
      } catch (error) {
        if (isAbort(error)) return;
        problem = historyMessage(error, 'branches');
        phase = 'failed';
      }
    })();
    return () => {
      controller.abort();
      listController?.abort();
      detailController?.abort();
    };
  });

  async function showMore() {
    if (paging || !more || !next) return;
    paging = true;
    const controller = new AbortController();
    listController = controller;
    try {
      const page = await loadBranches(projectId, { after: next, signal: controller.signal });
      const known = new Set(branches.map((b) => b.name));
      branches = [...branches, ...page.branches.filter((b) => !known.has(b.name))].slice(
        0,
        BRANCH_CAP
      );
      next = page.next;
      more = page.more && branches.length < BRANCH_CAP;
      skipped += page.skipped;
    } catch (error) {
      if (!isAbort(error)) problem = historyMessage(error, 'branches');
    } finally {
      paging = false;
    }
  }

  async function toggle(branch: Branch) {
    detailController?.abort();
    if (selected === branch.name) {
      selected = null;
      return;
    }
    const controller = new AbortController(),
      { signal } = controller;
    detailController = controller;
    selected = branch.name;
    commit = null;
    commitProblem = '';
    commitLoading = true;
    reviews = [];
    reviewsNext = null;
    reviewsMore = false;
    reviewsSkipped = 0;
    reviewsProblem = '';
    reviewsLoading = true;
    void tick().then(() => {
      panel?.focus({ preventScroll: true });
      panel?.scrollIntoView?.({ block: 'nearest' });
    });
    await Promise.all([
      loadCommit(projectId, branch.commit, signal).then(
        (value) => {
          if (signal.aborted) return;
          commit = value;
          commitLoading = false;
        },
        (error) => {
          if (signal.aborted || isAbort(error)) return;
          commitProblem = historyMessage(error, 'commit');
          commitLoading = false;
          // The commit envelope is gone or out of reach; say so on the row.
          if (error?.status === 409 || error?.status === 404)
            unavailable = { ...unavailable, [branch.name]: true };
        }
      ),
      loadReviews(projectId, branch.commit, { signal }).then(
        (page) => {
          if (signal.aborted) return;
          reviews = page.items;
          reviewsNext = page.next;
          reviewsMore = page.more;
          reviewsSkipped = page.skipped;
          reviewsLoading = false;
        },
        (error) => {
          if (signal.aborted || isAbort(error)) return;
          reviewsProblem = historyMessage(error, 'reviews');
          reviewsLoading = false;
        }
      )
    ]);
  }

  async function moreReviews() {
    if (!current || !reviewsMore || reviewsNext === null || reviewsLoading) return;
    const controller = new AbortController();
    detailController = controller;
    reviewsLoading = true;
    try {
      const page = await loadReviews(projectId, current.commit, {
        after: reviewsNext,
        signal: controller.signal
      });
      reviews = [...reviews, ...page.items].slice(0, REVIEW_CAP);
      reviewsNext = page.next;
      reviewsMore = page.more && reviews.length < REVIEW_CAP;
      reviewsSkipped += page.skipped;
    } catch (error) {
      if (!isAbort(error)) reviewsProblem = historyMessage(error, 'reviews');
    } finally {
      reviewsLoading = false;
    }
  }
</script>

<div class="branches">
  <h2>Branches</h2>
  <p class="repo-muted">
    Project-wide branches of signed shared history, with the receipt the server issued for each
    head. This view is read-only: branches are published and moved from the command line.
  </p>

  {#if phase === 'loading'}
    <div class="repo-box" aria-busy="true">
      {#each [0, 1, 2] as n (n)}<div class="repo-skel-row" aria-hidden="true">
          <span class="repo-skel" style:width="16px"></span><span
            class="repo-skel"
            style:width="{24 + ((n * 17) % 30)}%"
          ></span><span class="repo-skel" style:width="{12 + ((n * 11) % 20)}%"></span>
        </div>{/each}
    </div>
  {:else if phase === 'failed'}
    <p class="notice" aria-live="polite">{problem}</p>
    <button type="button" onclick={() => attempt++}>Try again</button>
  {:else if !branches.length}
    <div class="repo-box">
      <div class="repo-empty">
        <GitGraph size={30} aria-hidden="true" />
        <h3>No shared history yet</h3>
        <p>
          Shared history is a project-wide record of signed commits, branches and reviews that team
          members publish from their own computers, separate from the web editors.
        </p>
        <p>
          Nothing has been published for this project yet. History is published from the operator
          command line with <code>engine-cli sync …</code>, and its branches appear here once the
          first one is pushed.
        </p>
      </div>
    </div>
  {:else}
    <!-- A scrollable area must be reachable by keyboard so its overflow can be read (WCAG 2.1.1). -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="scroll" role="region" aria-label="Project branches" tabindex="0">
      <table>
        <caption class="sr-only">
          Project branches with their head commit, generation, declared update time and state
        </caption>
        <thead>
          <tr>
            <th scope="col">Branch</th>
            <th scope="col">Head commit</th>
            <th scope="col" class="num">Generation</th>
            <th scope="col">Updated</th>
            <th scope="col">State</th>
            <th scope="col"><span class="sr-only">Details</span></th>
          </tr>
        </thead>
        <tbody>
          {#each ordered as branch (branch.name)}
            <BranchRow
              {branch}
              expanded={selected === branch.name}
              unavailable={!!unavailable[branch.name]}
              controls="branch-details"
              onToggle={() => void toggle(branch)}
            />
          {/each}
        </tbody>
      </table>
    </div>
    <p class="foot">
      {plural(branches.length, 'branch', 'branches')}{more ? ' loaded so far' : ''}. Updated times
      are declared in the signed receipts and are not independently trusted timestamps.
      {#if skipped}{plural(skipped, 'branch', 'branches')} could not be displayed.{/if}
    </p>
    {#if more}
      <div class="repo-pagination">
        <button type="button" disabled={paging} onclick={() => void showMore()}
          >{paging ? 'Loading…' : 'Show more branches'}</button
        >
      </div>
    {/if}
    {#if problem}<p class="notice" aria-live="polite">{problem}</p>{/if}

    <div id="branch-details">
      {#if current}
        <section class="details" aria-labelledby="branch-details-title">
          <h3 id="branch-details-title" tabindex="-1" bind:this={panel}>
            Branch details: {current.name}
          </h3>
          <div class="pair">
            <ReceiptCard receipt={current.receipt} />
            <section class="card" aria-labelledby="commit-title" aria-busy={commitLoading}>
              <h4 id="commit-title">
                <GitCommitHorizontal size={15} aria-hidden="true" />Head commit
              </h4>
              <p class="hash">
                <HashChip hash={current.commit} label="commit hash" />
                <code class="hex">{current.commit}</code>
              </p>
              {#if commitProblem}
                <p class="notice" aria-live="polite">{commitProblem}</p>
              {:else if commitLoading || !commit}
                <p class="faint">Loading commit…</p>
              {:else}
                <p class="message">
                  {commit.message || 'No message.'}{commit.messageCut ? ' …' : ''}
                </p>
                <dl>
                  <dt>Signing key</dt>
                  <dd>
                    <code class="hex">{commit.authorKey}</code>
                    <small>Account names for keys are not part of this record.</small>
                  </dd>
                  <dt>Declared time</dt>
                  <dd>
                    {#if commit.authoredAt}{absoluteTime(commit.authoredAt)}
                      <span class="faint">(<RelTime at={commit.authoredAt} />)</span>{:else}Not
                      provided{/if}
                  </dd>
                  <dt>Parents</dt>
                  <dd>
                    {commit.parents === 0
                      ? 'None. This is a root commit.'
                      : commit.parents === 1
                        ? '1 parent'
                        : `${commit.parents} parents (a merge)`}{commit.recovery
                      ? ', recorded as a recovery of earlier history'
                      : ''}
                  </dd>
                  <dt>Files</dt>
                  <dd>
                    {plural(commit.fileCount, 'file')}
                    {#if commit.files.length}
                      <ul class="paths">
                        {#each commit.files as path (path)}<li><code>{path}</code></li>{/each}
                      </ul>
                      {#if commit.fileCount > commit.files.length}<small
                          >and {commit.fileCount - commit.files.length} more</small
                        >{/if}
                    {/if}
                  </dd>
                </dl>
              {/if}
            </section>
          </div>
          <ReviewList
            {groups}
            branch={current.name}
            required={approvalsRequired(current.name, requiredApprovals)}
            {members}
            loading={reviewsLoading}
            problem={reviewsProblem}
            more={reviewsMore}
            skipped={reviewsSkipped}
            onMore={() => void moreReviews()}
          />
        </section>
      {/if}
    </div>
  {/if}
</div>

<style>
  .branches h2 {
    margin-bottom: 4px;
  }
  .branches h3 {
    font-size: 16px;
  }
  .scroll {
    position: relative;
    overflow-x: auto;
    margin: 0 0 8px;
    border: 1px solid var(--rp-border);
    border-radius: 6px;
    background: var(--rp-canvas);
  }
  table {
    width: 100%;
    min-width: 640px;
    border-collapse: collapse;
    font-size: 14px;
  }
  thead th {
    padding: 8px 12px;
    background: var(--rp-subtle);
    border-bottom: 1px solid var(--rp-border);
    color: var(--rp-muted);
    font-size: 12px;
    font-weight: 600;
    text-align: left;
    white-space: nowrap;
  }
  thead th.num {
    text-align: left;
  }
  .repo-empty h3 {
    margin: 8px 0 4px;
    color: var(--rp-fg);
  }
  .foot {
    margin: 0 0 12px;
    color: var(--rp-muted);
    font-size: 12px;
  }
  .notice {
    margin: 0 0 12px;
    padding: 10px 14px;
    border: 1px solid var(--rp-attention-border);
    border-radius: 6px;
    background: var(--rp-attention-bg);
    color: var(--rp-fg);
    overflow-wrap: anywhere;
  }
  .details {
    margin-top: 16px;
  }
  .details h3 {
    margin: 0 0 10px;
    overflow-wrap: anywhere;
  }
  .details h3:focus-visible {
    outline: 2px solid var(--rp-accent);
    outline-offset: 2px;
  }
  .pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    margin-bottom: 12px;
  }
  .card {
    min-width: 0;
    padding: 12px 16px;
    border: 1px solid var(--rp-border);
    border-radius: 6px;
    background: var(--rp-canvas);
  }
  .card h4 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 8px;
    font-size: 14px;
  }
  .hash {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0 0 8px;
  }
  code {
    font-family: var(--rp-mono);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .hex {
    display: inline-block;
    max-width: 32.5ch;
    word-break: break-all;
  }
  .message {
    margin: 0 0 10px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .faint {
    color: var(--rp-muted);
  }
  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 14px;
    margin: 0;
  }
  dt {
    color: var(--rp-muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  small {
    display: block;
    color: var(--rp-muted);
    font-size: 12px;
  }
  .paths {
    margin: 4px 0 0;
    padding: 0;
    list-style: none;
  }
  @media (max-width: 860px) {
    .pair {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  @media (max-width: 480px) {
    dl {
      grid-template-columns: minmax(0, 1fr);
      gap: 0;
    }
    dd {
      margin-bottom: 8px;
    }
  }
</style>
