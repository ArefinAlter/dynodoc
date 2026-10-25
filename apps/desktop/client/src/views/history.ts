// History: GitHub Desktop's History tab (commit list, the selected commit's diff), Dynodoc's
// hash chain per revision, revert and cherry-pick (each makes a recovery draft, as the web
// workspace's "Recover earlier content" does), Compare (a draft against main) and a graph.
import { h, s, cx } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, render } from '../state.ts';
import * as B from '../data/backend.ts';
import type { Commit, Draft } from '../data/types.ts';
import { stats } from '../lib/diff.ts';
import { button, empty, input, pill, select, segmented, tabs, avatar, notice, hash } from '../ui/controls.ts';
import { diffView } from '../ui/diffView.ts';
import { confirmDialog, openMenu } from '../ui/layer.ts';
import { formatDate, formatTime } from '../lib/format.ts';
import { act, branchLabel, commitBase, currentBranch, docOf, fileIcon, isAgent, personName, rev, runVerify, short, verifyIcon, verifyPill, draftWord, draftTone } from './common.ts';
import { rebaseDialog, sendForReviewDialog, switchBranch } from './actions.ts';

function commitsForView(): Commit[] {
  const ui = state.ui.history;
  const branch = currentBranch();
  const docs = new Set(B.docsOf(state.db, state.ui.projectId).map((d) => d.id));
  let list: Commit[];
  if (branch === 'main') list = state.db.commits.filter((c) => docs.has(c.docId) && c.branch === 'main');
  else {
    const d = B.draft(state.db, branch);
    list = d ? [...state.db.commits.filter((c) => c.branch === d.id), ...state.db.commits.filter((c) => c.docId === d.docId && c.branch === 'main' && c.seq <= d.baseSeq)] : [];
  }
  if (ui.docFilter) list = list.filter((c) => c.docId === ui.docFilter);
  const q = ui.filter.toLowerCase();
  if (q) list = list.filter((c) => `${c.summary} ${personName(c.authorId)} ${c.chainHash.slice(0, 12)} ${rev(c)}`.toLowerCase().includes(q));
  return list.sort((a, b) => b.at - a.at);
}

function recover(c: Commit, mode: 'revert' | 'cherry_pick'): void {
  const run = () =>
    act(() => {
      const d = B.recoveryDraft(state.db, c.id, mode);
      state.ui.branchByProject[state.ui.projectId] = d.id;
      state.ui.history.selected = null;
      return `${mode === 'revert' ? 'Reverted' : 'Picked'} ${rev(c)} into the draft ${d.name}. Review it, then send it for review.`;
    });
  if (!state.prefs.prompts.recovery) return void run();
  confirmDialog(
    mode === 'revert' ? 'Revert this revision' : 'Cherry-pick this revision',
    h('p', null, mode === 'revert'
      ? `Dynodoc never rewrites history. Reverting ${rev(c)} "${c.summary}" makes a new draft of ${docOf(c.docId).filename} that undoes its changes; it reaches main only through a change request.`
      : `Cherry-picking ${rev(c)} "${c.summary}" makes a new draft of ${docOf(c.docId).filename} from the latest main with these changes applied again.`),
    mode === 'revert' ? 'Create revert draft' : 'Create draft',
    run,
  );
}

function commitMenu(anchor: HTMLElement, c: Commit): void {
  openMenu(anchor, [
    { label: 'Revert this revision…', icon: 'revert', disabled: c.local, note: c.local ? 'Push it first.' : undefined, onSelect: () => recover(c, 'revert') },
    { label: 'Cherry-pick onto a new draft…', icon: 'cherry', disabled: c.branch === 'main' && c.kind !== 'revert', note: c.branch === 'main' && c.kind !== 'revert' ? 'Already on main.' : undefined, onSelect: () => recover(c, 'cherry_pick') },
    { label: 'Verify hash chain of this file', icon: 'shield', onSelect: () => verifyFile(c.docId) },
    { label: '', separator: true },
    { label: 'Copy chain hash', icon: 'link', onSelect: () => void navigator.clipboard?.writeText(c.chainHash).catch(() => undefined) },
    { label: 'Reset to this revision', disabled: true, note: 'Dynodoc history is append-only; restore content through a draft instead.' },
    { label: 'Amend', disabled: true, note: 'Revisions cannot be changed after they are made.' },
  ]);
}

export function verifyFile(docId: string): void {
  const r = runVerify(docId);
  act(() => (r.bad ? `${r.bad} revision${r.bad === 1 ? '' : 's'} of ${docOf(docId).filename} do not verify` : `Verified: the hash chain of ${docOf(docId).filename} is intact (${r.ok} revisions)`));
}

function commitRow(c: Commit, selected: boolean): HTMLElement {
  const d = docOf(c.docId);
  return h(
    'div',
    {
      class: cx('commit-row', selected && 'is-selected'),
      role: 'option',
      tabindex: '0',
      'aria-selected': String(selected),
      onClick: () => { state.ui.history.selected = c.id; render(); },
      onKeydown: (e: KeyboardEvent) => {
        if (e.key === 'Enter') { state.ui.history.selected = c.id; render(); }
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          e.preventDefault();
          const next = (e.currentTarget as HTMLElement)[e.key === 'ArrowDown' ? 'nextElementSibling' : 'previousElementSibling'] as HTMLElement | null;
          next?.click();
          requestAnimationFrame(() => (document.querySelector('.commit-row.is-selected') as HTMLElement | null)?.focus());
        }
      },
      onContextmenu: (e: MouseEvent) => { e.preventDefault(); commitMenu(e.currentTarget as HTMLElement, c); },
    },
    h('div', { class: 'commit-line1' }, c.kind === 'merge' ? icon('merge', 13, 'icon commit-kind') : c.kind === 'revert' ? icon('revert', 13, 'icon commit-kind') : null, h('span', { class: 'commit-summary' }, c.summary)),
    h(
      'div',
      { class: 'commit-line2' },
      avatar(personName(c.authorId), isAgent(c.authorId)),
      h('span', { class: 'commit-author' }, personName(c.authorId)),
      h('span', { class: 'muted' }, short(c.at)),
      h('span', { class: 'spacer' }),
      fileIcon(d.kind, 12),
      h('span', { class: 'commit-rev mono' }, rev(c)),
      c.local ? h('span', { class: 'local-badge', title: 'On this computer only: push to share it' }, icon('up', 11)) : null,
      verifyIcon(c.id),
    ),
  );
}

function commitDetail(c: Commit): HTMLElement {
  const d = docOf(c.docId);
  const base = commitBase(c);
  const st = stats(c.edits);
  const p = state.prefs;
  return h(
    'div',
    { class: 'pane pane-main' },
    h(
      'div',
      { class: 'commit-head' },
      h('div', { class: 'commit-head-title' }, h('h2', null, c.summary), c.description ? h('p', { class: 'commit-desc' }, c.description) : null),
      h(
        'div',
        { class: 'commit-meta' },
        avatar(personName(c.authorId), isAgent(c.authorId), 'md'),
        h('span', null, h('strong', null, personName(c.authorId)), isAgent(c.authorId) ? h('span', { class: 'muted' }, ' (agent key)') : null),
        h('span', { class: 'muted' }, `${formatDate(c.at, p)} ${formatTime(c.at, p)}`),
        h('span', { class: 'meta-sep' }),
        fileIcon(d.kind, 13),
        h('span', null, d.filename),
        h('span', { class: 'mono' }, rev(c)),
        c.kind !== 'revision' ? pill(c.kind.replace('_', '-'), c.kind === 'merge' ? 'primary' : 'info') : null,
        c.local ? pill('not pushed', 'warning') : null,
        h('span', { class: 'spacer' }),
        button('', { icon: 'more', variant: 'ghost', size: 'sm', title: 'More', onClick: (e: MouseEvent) => commitMenu(e.currentTarget as HTMLElement, c) }),
      ),
      h(
        'dl',
        { class: 'chain' },
        h('dt', null, 'Chain hash'), h('dd', null, hash(c.chainHash, 64)),
        h('dt', null, 'Previous'), h('dd', null, hash(c.prevChainHash, 64)),
        h('dt', null, 'Content hash'), h('dd', null, hash(c.contentHash, 64)),
        h('dt', null, 'Verification'), h('dd', null, verifyPill(c.id), ' ', button('Verify hash chain', { size: 'sm', icon: 'shield', onClick: () => verifyFile(c.docId) })),
      ),
      h(
        'div',
        { class: 'commit-actions' },
        h('span', { class: 'diff-stat' }, `${st.edited} edited · ${st.added} added · ${st.removed} removed`),
        h('span', { class: 'spacer' }),
        segmented(p.diffLayout, [['unified', 'Unified'], ['split', 'Split']], (v) => { p.diffLayout = v; render(); }, 'Diff layout'),
        button('Revert', { size: 'sm', icon: 'revert', disabled: c.local, title: 'Make a draft that undoes this revision', onClick: () => recover(c, 'revert') }),
        c.branch !== 'main' || c.kind === 'revert' ? button('Cherry-pick', { size: 'sm', icon: 'cherry', onClick: () => recover(c, 'cherry_pick') }) : null,
      ),
    ),
    h('div', { class: 'pane-scroll diff-scroll', 'data-scroll-key': 'history-diff' }, diffView(base, c.edits, { emptyText: 'This revision changes nothing.' })),
  );
}

// ---- compare ---------------------------------------------------------------------------------------

function compareView(): HTMLElement {
  const ui = state.ui.history;
  const drafts = state.db.drafts.filter((d) => d.projectId === state.ui.projectId && (d.state === 'private' || d.state === 'in_review'));
  if (!ui.compareBranch || !drafts.some((d) => d.id === ui.compareBranch)) ui.compareBranch = (drafts.find((d) => d.id === currentBranch()) ?? drafts[0])?.id ?? '';
  const d = B.draft(state.db, ui.compareBranch);
  const head = h(
    'div',
    { class: 'compare-head' },
    h('span', { class: 'muted' }, 'Compare'),
    select(ui.compareBranch, drafts.map((x) => [x.id, `${x.name} (${docOf(x.docId).filename})`] as [string, string]), (v) => { ui.compareBranch = v; render(); }, { label: 'Draft to compare' }),
    h('span', { class: 'muted' }, 'with'),
    h('span', { class: 'branch-chip' }, icon('branch', 13), 'main'),
  );
  if (!d) return h('div', { class: 'pane pane-main' }, head, empty('compare', 'No open drafts', 'Start a draft from the Branch menu to compare it with main.'));
  const ahead = B.branchCommits(state.db, d.docId, d.id).length;
  const behind = B.behind(state.db, d);
  const edits = B.draftEdits(state.db, d);
  const conflicts = B.conflictsOf(state.db, d);
  const mine = d.authorId === state.db.me;
  return h(
    'div',
    { class: 'pane pane-main' },
    head,
    h(
      'div',
      { class: 'compare-summary' },
      pill(`${ahead} ahead`, 'primary'),
      pill(`${behind} behind`, behind ? 'warning' : 'neutral'),
      pill(draftWord(d.state), draftTone(d.state)),
      conflicts.length ? pill(`${conflicts.length} conflict${conflicts.length === 1 ? '' : 's'}`, 'error') : null,
      h('span', { class: 'spacer' }),
      mine && behind ? button('Update from main…', { size: 'sm', onClick: () => rebaseDialog(d.id, 'update') }) : null,
      mine && d.state === 'private' ? button('Send for review', { size: 'sm', variant: 'primary', onClick: () => sendForReviewDialog(d.id) }) : null,
      currentBranch() !== d.id ? button('Switch to this draft', { size: 'sm', onClick: () => switchBranch(d.id) }) : null,
    ),
    conflicts.length ? notice('warning', `main changed ${conflicts.map((c) => c.label).join(', ')} after this draft started. Rebase or resolve them in the review.`) : null,
    h('div', { class: 'pane-scroll diff-scroll', 'data-scroll-key': 'compare-diff' }, diffView(B.baseBlocks(state.db, d), edits, { emptyText: 'The draft has no changes against main.' })),
  );
}

// ---- graph -------------------------------------------------------------------------------------------

function graphView(): HTMLElement {
  const ui = state.ui.history;
  const docs = B.docsOf(state.db, state.ui.projectId);
  const docId = ui.docFilter || docs[0]?.id;
  const head = h(
    'div',
    { class: 'compare-head' },
    h('span', { class: 'muted' }, 'History graph of'),
    select(docId, docs.map((x) => [x.id, x.filename] as [string, string]), (v) => { ui.docFilter = v; render(); }, { label: 'File' }),
    h('span', { class: 'spacer' }),
    h('span', { class: 'legend' }, h('span', { class: 'lg lg-main' }), 'main', h('span', { class: 'lg lg-draft' }), 'drafts'),
  );
  if (!docId) return h('div', { class: 'pane pane-main' }, head);
  const drafts: Draft[] = state.db.drafts.filter((d) => d.docId === docId && B.branchCommits(state.db, docId, d.id).length > 0);
  const lanes = new Map<string, number>([['main', 0], ...drafts.map((d, i) => [d.id, i + 1] as [string, number])]);
  const commits = state.db.commits.filter((c) => c.docId === docId && lanes.has(c.branch)).sort((a, b) => b.at - a.at);
  const ROW = 44, LANE = 22, X0 = 18;
  const y = (i: number) => i * ROW + ROW / 2;
  const x = (lane: number) => X0 + lane * LANE;
  const idx = new Map(commits.map((c, i) => [c.id, i]));
  const width = X0 + (drafts.length + 1) * LANE;
  const height = commits.length * ROW;
  const paths: SVGElement[] = [];
  const mainRows = commits.filter((c) => c.branch === 'main').map((c) => idx.get(c.id)!);
  if (mainRows.length) paths.push(s('path', { d: `M${x(0)} ${y(Math.min(...mainRows))} V${y(Math.max(...mainRows))}`, class: 'g-main' }));
  for (const d of drafts) {
    const lane = lanes.get(d.id)!;
    const rows = commits.filter((c) => c.branch === d.id).map((c) => idx.get(c.id)!);
    const top = Math.min(...rows), bottom = Math.max(...rows);
    paths.push(s('path', { d: `M${x(lane)} ${y(top)} V${y(bottom)}`, class: 'g-draft' }));
    const base = commits.find((c) => c.branch === 'main' && c.seq === d.baseSeq);
    if (base) {
      const by = y(idx.get(base.id)!);
      paths.push(s('path', { d: `M${x(lane)} ${y(bottom)} C${x(lane)} ${by - 10}, ${x(0)} ${by - 14}, ${x(0)} ${by}`, class: 'g-draft' }));
    }
    const merge = commits.find((c) => c.branch === 'main' && c.mergedFrom === d.id);
    if (merge) {
      const my = y(idx.get(merge.id)!);
      paths.push(s('path', { d: `M${x(lane)} ${y(top)} C${x(lane)} ${my + 10}, ${x(0)} ${my + 14}, ${x(0)} ${my}`, class: 'g-draft' }));
    } else if (d.state === 'in_review' || d.state === 'private') {
      paths.push(s('path', { d: `M${x(lane)} ${y(top)} V${Math.max(4, y(top) - ROW / 2)}`, class: 'g-draft g-open' }));
    }
  }
  const dots = commits.map((c, i) => s('circle', { cx: x(lanes.get(c.branch)!), cy: y(i), r: c.kind === 'merge' ? 5.5 : 4.5, class: cx('g-dot', c.branch === 'main' ? 'g-dot-main' : 'g-dot-draft', c.kind === 'merge' && 'g-dot-merge') }));
  const svg = s('svg', { class: 'graph-svg', width, height, viewBox: `0 0 ${width} ${height}`, 'aria-hidden': 'true' }, paths, dots);
  return h(
    'div',
    { class: 'pane pane-main' },
    head,
    h(
      'div',
      { class: 'pane-scroll', 'data-scroll-key': 'graph' },
      h(
        'div',
        { class: 'graph', css: { '--graph-w': `${width}px` } },
        svg,
        h(
          'div',
          { class: 'graph-rows' },
          commits.map((c) =>
            h(
              'button',
              { type: 'button', class: cx('graph-row', state.ui.history.selected === c.id && 'is-selected'), onClick: () => { state.ui.history.selected = c.id; state.ui.history.mode = 'list'; if (c.branch !== currentBranch() && c.branch !== 'main') switchBranch(c.branch); render(); } },
              h('span', { class: 'graph-summary' }, c.summary),
              c.branch !== 'main' ? h('span', { class: 'branch-chip' }, icon('branch', 11), B.branchName(state.db, c.branch)) : null,
              h('span', { class: 'spacer' }),
              h('span', { class: 'muted' }, personName(c.authorId)),
              h('span', { class: 'mono muted' }, rev(c)),
              h('span', { class: 'mono muted' }, c.chainHash.slice(0, 7)),
              h('span', { class: 'muted' }, short(c.at)),
            ),
          ),
        ),
      ),
    ),
  );
}

export function historyView(): HTMLElement {
  const ui = state.ui.history;
  const docs = B.docsOf(state.db, state.ui.projectId);
  const list = commitsForView();
  if (ui.mode === 'list' && (!ui.selected || !list.some((c) => c.id === ui.selected))) ui.selected = list[0]?.id ?? null;
  const sel = list.find((c) => c.id === ui.selected);
  const left = h(
    'div',
    { class: 'pane pane-list' },
    h('div', { class: 'pane-tabs' }, tabs(ui.mode, [['list', 'History'], ['compare', 'Compare'], ['graph', 'Graph']], (v) => { ui.mode = v; render(); }, 'History mode')),
    h('div', { class: 'pane-filter' }, icon('search', 14), input(ui.filter, (v) => { ui.filter = v; render(); }, { id: 'history-filter', placeholder: 'Filter by summary, author, hash' })),
    h('div', { class: 'pane-filter' }, select(ui.docFilter, [['', 'All files'], ...docs.map((d) => [d.id, d.filename] as [string, string])], (v) => { ui.docFilter = v; render(); }, { label: 'File', class: 'select-block' })),
    h('div', { class: 'pane-sub muted small' }, `${list.length} revision${list.length === 1 ? '' : 's'} on ${branchLabel()}${currentBranch() !== 'main' ? ' (and main before it)' : ''}`),
    h('div', { class: 'pane-scroll', role: 'listbox', 'aria-label': 'Revisions', 'data-scroll-key': 'history-list' }, list.length ? list.map((c) => commitRow(c, c.id === ui.selected)) : h('div', { class: 'change-none' }, 'No revisions match.')),
    h('div', { class: 'pane-foot' }, button('Verify every file', { icon: 'shield', size: 'sm', onClick: () => { let bad = 0, ok = 0; for (const d of docs) { const r = runVerify(d.id); bad += r.bad; ok += r.ok; } act(() => (bad ? `${bad} revisions do not verify` : `Verified ${ok} revisions in ${docs.length} files: every chain is intact`)); } })),
  );
  let right: HTMLElement;
  if (ui.mode === 'compare') right = compareView();
  else if (ui.mode === 'graph') right = graphView();
  else right = sel ? commitDetail(sel) : h('div', { class: 'pane pane-main' }, empty('history', 'No revisions', 'Nothing has been committed here yet.'));
  return h('div', { class: 'split-view' }, left, right);
}
