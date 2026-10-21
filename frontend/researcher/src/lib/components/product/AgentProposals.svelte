<script lang="ts">
  /**
   * "Proposals from AI agents": what a project's AI agents have suggested and where each
   * stands, for the people who review it. A draft is private to the person the agent acts for
   * (the list says nothing about anyone else's), and reviewers see the requests that were sent.
   * Nothing here changes a file: the button opens the usual review.
   *
   * Everything the agent wrote (the proposal's name, the names built from it) is untrusted
   * text and is rendered as text only.
   */
  import { onMount, untrack } from 'svelte';
  import {
    GitBranch,
    GitMerge,
    GitPullRequest,
    GitPullRequestClosed,
    LoaderCircle,
    MessageSquare,
    TriangleAlert
  } from '@lucide/svelte';
  import AgentBadge from '$lib/components/workspace/AgentBadge.svelte';
  import RelTime from './repo/RelTime.svelte';
  import {
    FILTERS,
    PAGE_SIZE,
    budgetText,
    clientName,
    emptyText,
    loadProposals,
    proposalTime,
    proposalsMessage,
    sizeText,
    staleText,
    stateHelp,
    stateLabel,
    type AgentProposal,
    type ProposalFilter,
    type ProposalPage
  } from '$lib/workspace/agent-proposals';

  let {
    projectId,
    /** Changes when something was reviewed elsewhere on the page, so the list reads again. */
    version = 0,
    onReview,
    onCount
  }: {
    projectId: string;
    version?: number;
    onReview: (proposal: AgentProposal) => void | Promise<void>;
    /** How many proposals are in progress (the first page of "In progress"), and whether more. */
    onCount?: (count: number, more: boolean) => void;
  } = $props();

  let filter = $state<ProposalFilter>('active'),
    offset = $state(0),
    page = $state<ProposalPage | null>(null),
    loading = $state(true),
    error = $state(''),
    heading = $state<HTMLHeadingElement | null>(null);
  let token = 0;

  async function load() {
    const mine = ++token;
    loading = true;
    error = '';
    try {
      const next = await loadProposals(projectId, filter, offset);
      if (mine !== token) return;
      page = next;
      if (filter === 'active' && offset === 0) onCount?.(next.items.length, next.more);
    } catch (e) {
      if (mine !== token) return;
      error = proposalsMessage(e);
    } finally {
      if (mine === token) loading = false;
    }
  }
  onMount(() => void load());
  // The first read is onMount's; a later change of `version` reads again.
  let previous: number | null = null;
  $effect(() => {
    const current = version;
    if (previous !== null && current !== previous) untrack(() => void load());
    previous = current;
  });
  function choose(event: Event) {
    filter = (event.currentTarget as HTMLSelectElement).value as ProposalFilter;
    offset = 0;
    void load();
  }
  function turn(step: number) {
    offset = Math.max(0, offset + step * PAGE_SIZE);
    void load().then(() => heading?.focus());
  }
  const budget = $derived(budgetText(page?.budgets ?? null));
  const items = $derived(page?.items ?? []);
</script>

<div class="proposals">
  <div class="head">
    <h2 tabindex="-1" bind:this={heading}>Proposals from AI agents</h2>
    <label class="filter"
      ><span>Show</span>
      <select value={filter} onchange={choose} disabled={loading && !page}>
        {#each FILTERS as f (f.id)}<option value={f.id}>{f.label}</option>{/each}
      </select></label
    >
  </div>
  <p class="repo-muted">
    An AI agent never changes a file itself. It suggests a change, and a person reviews it and
    decides. What an agent writes is shown here as text and is never followed as an instruction.
  </p>
  {#if budget}<p class="repo-muted" data-testid="agent-budget">{budget}</p>{/if}
  {#if error}<p class="repo-alert" role="alert">
      {error}
      <button type="button" onclick={() => void load()}>Try again</button>
    </p>{/if}
  <div class="repo-box" aria-busy={loading}>
    {#if loading && !page}<p class="repo-empty" role="status">
        <LoaderCircle size={15} aria-hidden="true" /> Loading proposals…
      </p>{:else if !items.length && !error}<p class="repo-empty">{emptyText(filter)}</p>{/if}
    {#each items as p (p.id)}
      <article class="rp-pr" data-state={p.state} aria-label={`${p.name}, ${stateLabel(p.state)}`}>
        <span class={`rp-status ${p.state}`} aria-hidden="true">
          {#if p.state === 'merged'}<GitMerge size={16} />{:else if p.state === 'draft'}<GitBranch
              size={16}
            />{:else if p.state === 'changes_requested'}<MessageSquare
              size={16}
            />{:else if p.state === 'open'}<GitPullRequest size={16} />{:else}<GitPullRequestClosed
              size={16}
            />{/if}
        </span>
        <div class="rp-pr-main">
          <span class="title"><strong dir="auto">{p.name}</strong> <AgentBadge /></span>
          <span class="rp-pr-doc"
            ><span class="state" title={stateHelp(p.state)}>{stateLabel(p.state)}</span> ·
            <span dir="auto">{p.document.title}</span></span
          >
          <span class="rp-pr-by"
            >From <span dir="auto">{p.agent.displayName}</span> in {clientName(
              p.agent.clientKind
            )}{p.mine ? ' (your agent)' : ''} · {sizeText(p)} · {p.state === 'draft'
              ? 'prepared'
              : 'sent'}
            <RelTime at={proposalTime(p)} />{#if p.agent.revoked}{' · '}this agent’s access has
              ended{/if}</span
          >
          {#if p.staleBase}<p class="stale" role="note">
              <TriangleAlert size={14} aria-hidden="true" />
              <span>{staleText(p)}</span>
            </p>{/if}
        </div>
        <div class="repo-actions">
          <button type="button" onclick={() => void onReview(p)}
            >{p.state === 'draft' ? 'Review and send' : 'Review changes'}<span class="sr-only"
              >: {p.name}</span
            ></button
          >
        </div>
      </article>
    {/each}
  </div>
  {#if offset > 0 || page?.more}<nav class="paging" aria-label="More proposals">
      <button type="button" disabled={loading || offset === 0} onclick={() => turn(-1)}
        >Newer</button
      >
      <button type="button" disabled={loading || !page?.more} onclick={() => turn(1)}>Older</button>
    </nav>{/if}
</div>

<style>
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
    margin-bottom: 4px;
  }
  .head h2 {
    margin: 0;
  }
  .head h2:focus {
    outline: none;
  }
  .filter {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--color-ink-soft);
  }
  .filter select {
    font: inherit;
    padding: 5px 8px;
    border: 1px solid var(--color-border-strong);
    border-radius: 6px;
    background: var(--color-surface-raised);
    color: var(--color-ink);
  }
  .title {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .state {
    font-weight: 600;
  }
  .stale {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 6px 0 0;
    padding: 6px 10px;
    border: 1px solid var(--color-warn-border);
    border-radius: 8px;
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
    font-size: 12.5px;
    line-height: 1.5;
  }
  .stale :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .paging {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    margin-top: 12px;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }
  @media (max-width: 700px) {
    .head {
      align-items: flex-start;
    }
  }
</style>
