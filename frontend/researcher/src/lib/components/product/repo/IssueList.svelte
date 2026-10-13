<script lang="ts">
  import { tick, untrack } from 'svelte';
  import {
    Check,
    ChevronDown,
    CircleCheck,
    CircleDot,
    MessageSquare,
    Plus,
    Search,
    Tag
  } from '@lucide/svelte';
  import type { MemberLite } from '$lib/workspace/activity';
  import {
    DEFAULT_FILTERS,
    ISSUE_CAP,
    appendIssues,
    canOpenIssues,
    hasFilters,
    isAbort,
    isUnavailable,
    issueMessage,
    listIssues,
    listLabels,
    personName,
    type Issue,
    type IssueCounts,
    type IssueFilters,
    type IssuesRequest,
    type IssueState,
    type IssueSummary,
    type LabelRow
  } from '$lib/workspace/issues';
  import { projectRoleName } from '$lib/workspace/projects';
  import { absoluteTime, displayName, plural } from '$lib/workspace/repo-ui';
  import Avatar from './Avatar.svelte';
  import IssueDetail from './IssueDetail.svelte';
  import IssueForm from './IssueForm.svelte';
  import LabelPill from './LabelPill.svelte';
  import LabelsManager from './LabelsManager.svelte';
  import Menu from './Menu.svelte';
  import RelTime from './RelTime.svelte';

  type View = 'list' | 'new' | 'issue' | 'labels';
  let {
    projectId,
    role,
    members = [],
    request = null,
    onCounts,
    onNavigate
  }: {
    projectId: string;
    role: string;
    members?: MemberLite[];
    request?: IssuesRequest | null;
    onCounts?: (counts: IssueCounts) => void;
    /** Tell the page which issue is open, so the address can be shared. */
    onNavigate?: (number: number | null) => void;
  } = $props();

  let view = $state<View>('list'),
    current = $state<number | null>(null),
    filters = $state<IssueFilters>({ ...DEFAULT_FILTERS }),
    search = $state(''),
    items = $state<IssueSummary[]>([]),
    counts = $state<IssueCounts>({ open: 0, closed: 0 }),
    nextBefore = $state<number | null>(null),
    skipped = $state(0),
    phase = $state<'loading' | 'ready' | 'failed' | 'unavailable'>('loading'),
    refreshing = $state(false),
    paging = $state(false),
    problem = $state(''),
    attempt = $state(0),
    labels = $state<LabelRow[]>([]),
    canManage = $state(false),
    labelsFailed = $state(false),
    labelsAttempt = $state(0),
    heading = $state<HTMLElement | null>(null);
  let controller: AbortController | null = null,
    generation = 0,
    handled = 0,
    loadedOnce = false;

  const canOpen = $derived(canOpenIssues(role)),
    labelName = $derived(labels.find((l) => l.id === filters.label)?.name ?? ''),
    personFilterName = (id: string) =>
      id === 'me'
        ? 'me'
        : id === 'none'
          ? 'nobody'
          : displayName(
              members.find((m) => m.id === id)?.name,
              members.find((m) => m.id === id)?.email
            ),
    people = $derived(members.map((m) => ({ id: m.id, name: displayName(m.name, m.email) })));

  // The parent asks for a view (the palette's "New issue", or a shared issue address).
  $effect(() => {
    const asked = request;
    if (!asked || asked.at === handled) return;
    handled = asked.at;
    if (asked.view === 'issue' && asked.number) {
      current = asked.number;
      view = 'issue';
    } else if (asked.view === 'new') {
      current = null;
      view = 'new';
    } else {
      current = null;
      view = 'list';
    }
  });

  // The list is read again when its filters change and whenever the list comes back into view.
  $effect(() => {
    const project = projectId,
      showing = view === 'list';
    void attempt;
    if (!showing && loadedOnce) return;
    const snapshot = { ...filters },
      mine = new AbortController(),
      stamp = ++generation;
    loadedOnce = true;
    controller = mine;
    refreshing = true;
    void (async () => {
      try {
        const page = await listIssues(project, snapshot, { signal: mine.signal });
        if (mine.signal.aborted) return;
        items = page.items;
        nextBefore = page.nextBefore;
        skipped = page.skipped;
        counts = page.counts;
        problem = '';
        phase = 'ready';
        onCounts?.(page.counts);
      } catch (error) {
        if (isAbort(error)) return;
        if (isUnavailable(error)) {
          phase = 'unavailable';
          return;
        }
        problem = issueMessage(error, 'list');
        phase = 'failed';
      } finally {
        if (stamp === generation) refreshing = false;
      }
    })();
    return () => mine.abort();
  });

  // Labels feed the filter, the new-issue form and the pickers; read once per project.
  async function readLabels(signal?: AbortSignal) {
    try {
      const list = await listLabels(projectId, signal);
      labels = list.items;
      canManage = list.canManage;
      labelsFailed = false;
      if (filters.label && !list.items.some((l) => l.id === filters.label)) filters.label = '';
    } catch (error) {
      if (isAbort(error)) return;
      labelsFailed = true;
    }
  }
  $effect(() => {
    void projectId;
    void labelsAttempt;
    const mine = new AbortController();
    void readLabels(mine.signal);
    return () => mine.abort();
  });

  // Typing waits a moment; Enter applies the search at once.
  $effect(() => {
    const value = search.trim();
    if (value === untrack(() => filters.q)) return;
    const timer = setTimeout(() => (filters.q = value), 350);
    return () => clearTimeout(timer);
  });

  async function showMore() {
    if (paging || nextBefore === null) return;
    paging = true;
    const mine = new AbortController(),
      stamp = generation;
    controller?.abort();
    controller = mine;
    try {
      const page = await listIssues(
        projectId,
        { ...filters },
        {
          before: nextBefore,
          signal: mine.signal
        }
      );
      if (stamp !== generation) return;
      items = appendIssues(items, page.items);
      nextBefore = page.nextBefore;
      counts = page.counts;
      onCounts?.(page.counts);
    } catch (error) {
      if (!isAbort(error)) problem = issueMessage(error, 'list');
    } finally {
      paging = false;
    }
  }

  function setState(state: IssueState) {
    filters.state = state;
  }
  function clearFilters() {
    search = '';
    filters = { ...DEFAULT_FILTERS, state: filters.state };
  }
  function openIssue(number: number) {
    current = number;
    view = 'issue';
    onNavigate?.(number);
  }
  async function toList() {
    view = 'list';
    current = null;
    onNavigate?.(null);
    await tick();
    heading?.focus({ preventScroll: true });
  }
  function changed(issue: Issue, previous: IssueState) {
    if (issue.state === previous) return;
    const delta = issue.state === 'closed' ? -1 : 1;
    counts = {
      open: Math.max(0, counts.open + delta),
      closed: Math.max(0, counts.closed - delta)
    };
    onCounts?.(counts);
  }
  function follow(event: MouseEvent, number: number) {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
      return;
    event.preventDefault();
    openIssue(number);
  }
  const stateFilters = $derived([
    { key: 'open' as const, label: 'Open', icon: CircleDot, count: counts.open },
    { key: 'closed' as const, label: 'Closed', icon: CircleCheck, count: counts.closed }
  ]);
</script>

{#if phase === 'unavailable'}
  <h2>Issues</h2>
  <div class="repo-box">
    <div class="repo-empty">
      <CircleDot size={30} aria-hidden="true" />
      <h3>Issues are not available on this server</h3>
      <p>
        This server does not offer project issues yet. Your files and pull requests are unaffected.
      </p>
    </div>
  </div>
{:else if view === 'new'}
  <IssueForm
    {projectId}
    {role}
    {members}
    {labels}
    {labelsFailed}
    onCancel={() => void toList()}
    onCreated={(issue) => {
      counts = { open: counts.open + 1, closed: counts.closed };
      onCounts?.(counts);
      openIssue(issue.number);
    }}
  />
{:else if view === 'issue' && current !== null}
  {#key current}
    <IssueDetail
      {projectId}
      number={current}
      {members}
      {labels}
      {labelsFailed}
      onBack={() => void toList()}
      onChanged={changed}
    />
  {/key}
{:else if view === 'labels'}
  <LabelsManager
    {projectId}
    {labels}
    {canManage}
    failed={labelsFailed}
    onReload={() => readLabels()}
    onBack={() => void toList()}
    onShowIssues={(id) => {
      filters = { ...DEFAULT_FILTERS, label: id };
      search = '';
      void toList();
    }}
  />
{:else}
  <div class="rp-section-head">
    <h2 id="issues-heading" tabindex="-1" bind:this={heading}>Issues</h2>
    <div class="repo-actions">
      <button type="button" onclick={() => (view = 'labels')}
        ><Tag size={14} aria-hidden="true" />Labels{#if labels.length}<span
            class="repo-counter"
            aria-hidden="true">{labels.length}</span
          >{/if}</button
      >
      {#if canOpen}<button type="button" class="repo-primary" onclick={() => (view = 'new')}
          ><Plus size={15} aria-hidden="true" />New issue</button
        >{/if}
    </div>
  </div>
  <p class="repo-muted">
    Track questions and tasks about this project: review follow-ups, missing evidence and assigned
    work.
  </p>
  <form
    class="rp-search iss-search"
    onsubmit={(e) => {
      e.preventDefault();
      filters.q = search.trim();
    }}
  >
    <Search size={15} aria-hidden="true" /><input
      type="search"
      aria-label="Search issues"
      placeholder="Search issue titles"
      maxlength="100"
      autocomplete="off"
      bind:value={search}
    />
  </form>
  <p class="sr-only" aria-live="polite">
    {phase === 'ready' && !refreshing ? `${plural(items.length, 'issue')} shown` : ''}
  </p>
  <div class="repo-box iss-box" aria-busy={phase === 'loading' || refreshing}>
    <div class="repo-box-head iss-head">
      <div class="repo-seg" role="group" aria-label="Show issues by state">
        {#each stateFilters as filter (filter.key)}
          <button
            type="button"
            aria-pressed={filters.state === filter.key}
            onclick={() => setState(filter.key)}
            ><filter.icon size={14} aria-hidden="true" />{filter.label}<span
              class="count"
              aria-hidden="true">{filter.count.toLocaleString()}</span
            ></button
          >
        {/each}
      </div>
      <span class="rp-spacer"></span>
      <Menu title="Filter by label">
        {#snippet trigger()}Label{labelName ? `: ${labelName}` : ''}<ChevronDown
            size={13}
            aria-hidden="true"
          />{/snippet}
        <div class="iss-menu-scroll" role="presentation">
          <button
            type="button"
            role="menuitemradio"
            aria-checked={!filters.label}
            onclick={() => (filters.label = '')}
            ><span class="menu-check-slot"
              >{#if !filters.label}<Check size={14} aria-hidden="true" />{/if}</span
            >All labels</button
          >
          {#each labels as label (label.id)}
            <button
              type="button"
              role="menuitemradio"
              aria-checked={filters.label === label.id}
              onclick={() => (filters.label = label.id)}
              ><span class="menu-check-slot"
                >{#if filters.label === label.id}<Check size={14} aria-hidden="true" />{/if}</span
              ><LabelPill name={label.name} color={label.color} /></button
            >
          {/each}
          {#if !labels.length}<p class="menu-head" role="presentation">No labels yet</p>{/if}
        </div>
      </Menu>
      <Menu title="Filter by author">
        {#snippet trigger()}Author{filters.author
            ? `: ${personFilterName(filters.author)}`
            : ''}<ChevronDown size={13} aria-hidden="true" />{/snippet}
        <div class="iss-menu-scroll" role="presentation">
          {#each [{ id: '', name: 'Anyone' }, { id: 'me', name: 'Opened by me' }, ...people] as person (person.id)}
            <button
              type="button"
              role="menuitemradio"
              aria-checked={filters.author === person.id}
              onclick={() => (filters.author = person.id)}
              ><span class="menu-check-slot"
                >{#if filters.author === person.id}<Check size={14} aria-hidden="true" />{/if}</span
              >{person.name}</button
            >
          {/each}
        </div>
      </Menu>
      <Menu title="Filter by assignee" align="right">
        {#snippet trigger()}Assignee{filters.assignee
            ? `: ${personFilterName(filters.assignee)}`
            : ''}<ChevronDown size={13} aria-hidden="true" />{/snippet}
        <div class="iss-menu-scroll" role="presentation">
          {#each [{ id: '', name: 'Anyone' }, { id: 'me', name: 'Assigned to me' }, { id: 'none', name: 'Unassigned' }, ...people] as person (person.id)}
            <button
              type="button"
              role="menuitemradio"
              aria-checked={filters.assignee === person.id}
              onclick={() => (filters.assignee = person.id)}
              ><span class="menu-check-slot"
                >{#if filters.assignee === person.id}<Check
                    size={14}
                    aria-hidden="true"
                  />{/if}</span
              >{person.name}</button
            >
          {/each}
        </div>
      </Menu>
    </div>

    {#if phase === 'loading'}
      {#each [0, 1, 2, 3] as n (n)}<div class="repo-skel-row" aria-hidden="true">
          <span class="repo-skel" style:width="16px"></span><span
            class="repo-skel"
            style:width="{30 + ((n * 17) % 35)}%"
          ></span>
        </div>{/each}
    {:else if phase === 'failed'}
      <div class="repo-empty" aria-live="polite">
        <p>{problem}</p>
        <button type="button" onclick={() => attempt++}>Try again</button>
      </div>
    {:else if !items.length}
      <div class="repo-empty">
        <CircleDot size={30} aria-hidden="true" />
        {#if hasFilters(filters)}
          <h3>No issues match these filters</h3>
          <p>Try fewer filters or a different search.</p>
          <button type="button" onclick={clearFilters}>Clear filters</button>
        {:else if counts.open + counts.closed === 0}
          <h3>No issues yet</h3>
          {#if canOpen}
            <p>Open an issue to track a question or a task about this project.</p>
            <button type="button" class="repo-primary" onclick={() => (view = 'new')}
              ><Plus size={15} aria-hidden="true" />New issue</button
            >
          {:else}
            <p>
              Nothing has been opened yet. Your role ({projectRoleName(role)}) can read issues but
              cannot open them.
            </p>
          {/if}
        {:else if filters.state === 'open'}
          <h3>No open issues</h3>
          <p>
            {plural(counts.closed, 'closed issue')} to look back on.
          </p>
          <button type="button" onclick={() => setState('closed')}>View closed issues</button>
        {:else}
          <h3>No closed issues</h3>
          <p>{plural(counts.open, 'issue')} still open.</p>
          <button type="button" onclick={() => setState('open')}>View open issues</button>
        {/if}
      </div>
    {:else}
      <ul class="iss-list" aria-label={filters.state === 'open' ? 'Open issues' : 'Closed issues'}>
        {#each items as issue (issue.number)}
          <li class="iss-row">
            <span class={`iss-state ${issue.state}`}>
              {#if issue.state === 'open'}<CircleDot
                  size={16}
                  aria-hidden="true"
                />{:else}<CircleCheck size={16} aria-hidden="true" />{/if}<span class="sr-only"
                >{issue.state === 'open' ? 'Open' : 'Closed'}:</span
              >
            </span>
            <div class="iss-main-cell">
              <p class="iss-row-title">
                <a
                  class="iss-link"
                  href={`/projects/${projectId}?tab=issues&issue=${issue.number}`}
                  onclick={(e) => follow(e, issue.number)}>{issue.title}</a
                >{#if issue.labels.length}<span class="iss-pills">
                    {#each issue.labels as label (label.id)}<LabelPill
                        name={label.name}
                        color={label.color}
                        description={label.description}
                      />{/each}</span
                  >{/if}
              </p>
              <p class="iss-meta">
                #{issue.number} opened <RelTime at={issue.createdAt} /> by {personName(
                  issue.author
                )}{#if issue.state === 'closed' && issue.closedAt}
                  · closed <RelTime at={issue.closedAt} />{/if}
              </p>
            </div>
            <div class="iss-side">
              {#if issue.assignee}<span
                  class="iss-assignee"
                  class:stale={!issue.assignee.member}
                  title={issue.assignee.member
                    ? `Assigned to ${issue.assignee.name}`
                    : `Assigned to ${issue.assignee.name}, who has left the project`}
                  ><Avatar name={issue.assignee.name} size={20} /><span class="sr-only"
                    >Assigned to {issue.assignee.name}{issue.assignee.member
                      ? ''
                      : ' (left the project)'}</span
                  ></span
                >{/if}
              {#if issue.commentCount > 0}<span
                  class="iss-comments"
                  title={`${plural(issue.commentCount, 'comment')}, last activity ${absoluteTime(issue.updatedAt)}`}
                  ><MessageSquare size={14} aria-hidden="true" />{issue.commentCount}<span
                    class="sr-only"
                  >
                    comments</span
                  ></span
                >{/if}
            </div>
          </li>
        {/each}
      </ul>
      {#if skipped}<p class="iss-foot repo-muted">
          {plural(skipped, 'issue')} could not be displayed.
        </p>{/if}
      {#if nextBefore !== null && items.length < ISSUE_CAP}
        <div class="repo-pagination iss-more">
          <button type="button" disabled={paging} onclick={() => void showMore()}
            >{paging ? 'Loading…' : 'Show more'}</button
          >
        </div>
      {:else if nextBefore !== null}
        <p class="iss-foot repo-muted">
          Showing the newest {ISSUE_CAP} issues. Search or filter to reach older ones.
        </p>
      {/if}
    {/if}
  </div>
  <div aria-live="polite">
    {#if problem && phase === 'ready'}<p class="repo-alert">{problem}</p>{/if}
  </div>
{/if}
