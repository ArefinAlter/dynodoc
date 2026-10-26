// Change requests: the list (open, merged, closed) and the review: the diff with a tick per
// change to include it, comments on a change, conflicts with a choice per block, the review state
// against the project's rules, and the buttons only a person presses: Approve, Request changes,
// Merge, Decline. An agent can propose a comment (Agent screen); it cannot press any of these.
import { h, cx, type Child } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, render } from '../state.ts';
import * as B from '../data/backend.ts';
import type { ChangeRequest } from '../data/types.ts';
import { button, empty, pill, tabs, textarea, avatar, notice, untrusted, badge } from '../ui/controls.ts';
import { diffView } from '../ui/diffView.ts';
import { openDialog, closeDialog } from '../ui/layer.ts';
import { act, docOf, fileIcon, isAgent, personName, requestTone, short, when, ROLE_LABEL } from './common.ts';
import { switchBranch } from './actions.ts';

function listFor(filter: 'open' | 'merged' | 'closed'): ChangeRequest[] {
  return state.db.requests
    .filter((r) => r.projectId === state.ui.projectId)
    .filter((r) => (filter === 'open' ? r.status === 'open' : filter === 'merged' ? r.status === 'merged' : r.status === 'declined' || r.status === 'withdrawn'))
    .sort((a, b) => b.createdAt - a.createdAt);
}

function reqRow(r: ChangeRequest): HTMLElement {
  const st = B.reviewState(state.db, r);
  const sel = state.ui.requests.selected === r.id;
  return h(
    'button',
    { type: 'button', class: cx('req-row', sel && 'is-selected'), 'aria-pressed': String(sel), onClick: () => { state.ui.requests.selected = r.id; render(); } },
    h('div', { class: 'req-line1' }, icon(r.status === 'merged' ? 'merge' : 'request', 14, `icon req-icon st-${r.status}`), h('span', { class: 'req-title' }, r.name), h('span', { class: 'muted mono' }, `#${r.number}`)),
    h(
      'div',
      { class: 'req-line2' },
      fileIcon(docOf(r.docId).kind, 12),
      h('span', { class: 'muted' }, docOf(r.docId).filename),
      h('span', { class: 'muted' }, '·'),
      avatar(personName(r.authorId), isAgent(r.authorId)),
      h('span', { class: 'muted' }, personName(r.authorId)),
      r.byAgent ? badge('AI proposal', 'info') : null,
      h('span', { class: 'spacer' }),
      r.status === 'open' ? h('span', { class: cx('req-approvals', st.satisfied && 'is-ok'), title: st.why }, icon('check', 12), `${st.approvals}/${st.required}`) : null,
      r.status === 'open' && r.conflicts.length ? h('span', { class: 'req-conflict', title: `${r.conflicts.length} conflicts` }, icon('warning', 12), String(r.conflicts.length)) : null,
      h('span', { class: 'muted' }, short(r.createdAt)),
    ),
  );
}

function mergeDialog(r: ChangeRequest): void {
  const included = r.edits.filter((e) => !r.excluded.includes(e.blockId));
  const target = B.mainSeq(state.db, r.docId);
  const run = () =>
    act(() => {
      const c = B.merge(state.db, r.id);
      return `Merged #${r.number} into main as r${c.seq}`;
    });
  if (!state.prefs.prompts.merge) return void run();
  openDialog({
    title: `Merge #${r.number}`,
    plain: true,
    width: 520,
    body: () =>
      h(
        'dl',
        { class: 'facts' },
        h('dt', null, 'Request'), h('dd', null, `#${r.number} ${r.name}`),
        h('dt', null, 'File'), h('dd', null, docOf(r.docId).filename),
        h('dt', null, 'Into'), h('dd', null, `main, r${target} becomes r${target + 1}`),
        h('dt', null, 'Changes'), h('dd', null, `${included.length} included${r.excluded.length ? `, ${r.excluded.length} left out` : ''}`),
        r.conflicts.length ? [h('dt', null, 'Conflicts'), h('dd', null, r.conflicts.map((c) => `${c.label}: ${c.choice === 'team' ? "team's version" : 'their version'}`).join('; '))] : null,
        h('dt', null, 'Approvals'), h('dd', null, `${B.reviewState(state.db, r).approvals} of ${B.reviewState(state.db, r).required} required`),
        h('dt', null, 'Merged by'), h('dd', null, `${state.prefs.authorName} (you), on this computer`),
      ),
    actions: () => [
      button('Cancel', { onClick: closeDialog }),
      button(`Merge ${included.length} change${included.length === 1 ? '' : 's'}`, { variant: 'primary', id: 'dialog-confirm', onClick: () => { closeDialog(); run(); } }),
    ],
  });
}

function declineDialog(r: ChangeRequest): void {
  let note = '';
  openDialog({
    title: `Decline #${r.number}`,
    plain: true,
    width: 520,
    body: () => [h('p', { class: 'dialog-text' }, 'The request is closed without merging. Its draft and comments stay in history.'), textarea(note, (v) => { note = v; }, { id: 'decline-note', placeholder: 'Why (shown to the author)', rows: 3 })],
    actions: () => [button('Cancel', { onClick: closeDialog }), button('Decline request', { variant: 'danger', onClick: () => { if (act(() => B.decline(state.db, r.id, note), `Declined #${r.number}`)) closeDialog(); } })],
  });
}

function commentDialog(r: ChangeRequest, blockId: string): void {
  let text = '';
  const label = B.blocksOf(state.db, r.docId, r.draftId).find((b) => b.id === blockId)?.label ?? blockId;
  openDialog({
    title: `Comment on ${label}`,
    width: 520,
    body: () => textarea(text, (v) => { text = v; }, { id: 'block-comment', rows: 4, placeholder: 'Leave a comment on this change' }),
    actions: () => [button('Cancel', { onClick: closeDialog }), button('Comment', { variant: 'primary', onClick: () => { if (act(() => B.comment(state.db, r.id, text, blockId), 'Comment added')) closeDialog(); } })],
  });
}

function commentItem(c: ChangeRequest['comments'][number]): HTMLElement {
  return h(
    'div',
    { class: 'comment' },
    avatar(personName(c.authorId), isAgent(c.authorId)),
    h('div', { class: 'comment-body' }, h('div', { class: 'comment-meta' }, h('strong', null, personName(c.authorId)), h('span', { class: 'muted' }, when(c.at)), c.proposedByAgent ? badge('drafted by the agent, posted by a person', 'info') : null), h('div', { class: 'comment-text' }, c.text)),
  );
}

function review(r: ChangeRequest): HTMLElement {
  const st = B.reviewState(state.db, r);
  const p = B.project(state.db, r.projectId);
  const d = B.draft(state.db, r.draftId);
  const doc = docOf(r.docId);
  const behind = d ? B.behind(state.db, d) : 0;
  const base = d ? B.baseBlocks(state.db, d) : doc.blocks;
  const byBlock: Record<string, Child[]> = {};
  for (const c of r.comments) if (c.blockId) (byBlock[c.blockId] ??= []).push(commentItem(c));
  const open = r.status === 'open';
  const mineReq = r.authorId === state.db.me;
  const latest = new Map<string, ChangeRequest['reviews'][number]>();
  for (const rv of r.reviews) latest.set(rv.reviewerId, rv);

  const conflicts = r.conflicts.length && open
    ? h(
        'div',
        { class: 'card conflicts-card' },
        h('div', { class: 'card-head' }, icon('warning', 15), h('strong', null, `${r.conflicts.length} conflict${r.conflicts.length === 1 ? '' : 's'} with main`), h('span', { class: 'spacer' }),
          button("Keep all team's", { size: 'sm', onClick: () => { r.conflicts.forEach((c) => (c.choice = 'team')); render(); } }),
          button('Use all of theirs', { size: 'sm', onClick: () => { r.conflicts.forEach((c) => (c.choice = 'draft')); render(); } })),
        h('p', { class: 'muted small' }, 'main changed these blocks after the draft started. Choose what the merge keeps for each.'),
        r.conflicts.map((c) =>
          h(
            'div',
            { class: 'conflict' },
            h('div', { class: 'conflict-label' }, c.label),
            h('div', { class: 'conflict-ancestor' }, h('span', { class: 'muted' }, 'Before both: '), c.ancestor),
            h(
              'div',
              { class: 'conflict-choices' },
              (['team', 'draft'] as const).map((side) =>
                h(
                  'label',
                  { class: cx('conflict-choice', c.choice === side && 'is-on') },
                  h('input', { type: 'radio', name: `rc-${r.id}-${c.blockId}`, checked: c.choice === side, onChange: () => { c.choice = side; render(); } }),
                  h('span', { class: 'conflict-side' }, side === 'team' ? "Keep the team's current version" : (r.authorId === state.db.me ? 'Use your version' : `Use ${personName(r.authorId)}'s version`)),
                  h('span', { class: 'conflict-text' }, side === 'team' ? c.team : c.draft),
                ),
              ),
            ),
          ),
        ),
      )
    : null;

  return h(
    'div',
    { class: 'pane pane-main' },
    h(
      'div',
      { class: 'req-head' },
      h('div', { class: 'req-head-line' }, h('h2', null, r.name, ' ', h('span', { class: 'muted' }, `#${r.number}`)), pill(r.status, requestTone(r.status))),
      h(
        'div',
        { class: 'req-sub' },
        avatar(personName(r.authorId), isAgent(r.authorId)),
        h('span', null, h('strong', null, personName(r.authorId)), ` wants to merge ${r.edits.length} change${r.edits.length === 1 ? '' : 's'} into `, h('span', { class: 'branch-chip' }, icon('branch', 11), 'main'), ' from ', h('span', { class: 'branch-chip' }, icon('branch', 11), d?.name ?? 'a draft')),
        h('span', { class: 'muted' }, `· ${doc.filename} · based on r${r.baseSeq} · ${when(r.createdAt)}`),
        d && d.authorId === state.db.me && (d.state === 'in_review' || d.state === 'private') ? button('Switch to draft', { size: 'sm', variant: 'ghost', onClick: () => switchBranch(d.id) }) : null,
      ),
    ),
    h(
      'div',
      { class: 'pane-scroll req-scroll', 'data-scroll-key': `req-${r.id}` },
      r.byAgent ? notice('info', 'Opened with an agent key (an AI proposal). The agent cannot approve or merge it; a person reviews it like any other request.') : null,
      r.agentStatement ? untrusted('What the agent said (not verified by Dynodoc)', r.agentStatement) : null,
      r.note ? h('div', { class: 'req-note' }, r.note) : null,
      open && behind ? notice('warning', `The team version has ${behind} newer revision${behind === 1 ? '' : 's'} since this draft started.${r.conflicts.length ? '' : ' None of them touch the same blocks.'}`) : null,
      conflicts,
      h('div', { class: 'section-title' }, `Changes`, h('span', { class: 'muted small' }, open ? ' · untick a change to leave it out of the merge' : '')),
      diffView(base, r.edits, {
        excluded: r.excluded,
        onToggle: open ? (blockId, include) => { r.excluded = include ? r.excluded.filter((x) => x !== blockId) : [...r.excluded, blockId]; render(); } : undefined,
        comments: byBlock,
        onComment: open ? (blockId) => commentDialog(r, blockId) : undefined,
      }),
      h('div', { class: 'section-title' }, 'Conversation'),
      h('div', { class: 'comments' }, r.comments.filter((c) => !c.blockId).map(commentItem), r.comments.filter((c) => !c.blockId).length ? null : h('div', { class: 'muted small' }, 'No general comments yet.')),
    ),
    h(
      'div',
      { class: 'review-bar' },
      h(
        'div',
        { class: 'review-state' },
        h('div', { class: 'review-rule' }, icon(st.satisfied ? 'check' : 'info', 14), h('strong', null, `${st.approvals} of ${st.required} approval${st.required === 1 ? '' : 's'}`), h('span', { class: 'muted' }, ` · ${p.rules.mergeRoles === 'owners' ? 'owners and managers merge' : 'editors and above merge'}`)),
        h(
          'div',
          { class: 'reviewers' },
          [...latest.values()].map((rv) =>
            h('span', { class: cx('reviewer', `v-${rv.verdict}`, rv.stale && 'is-stale'), title: rv.stale ? 'Given before the latest changes' : rv.note }, avatar(personName(rv.reviewerId)), personName(rv.reviewerId), ' ', rv.verdict === 'approved' ? 'approved' : rv.verdict === 'changes_requested' ? 'requested changes' : 'commented', rv.stale ? ' (stale)' : ''),
          ),
          latest.size ? null : h('span', { class: 'muted small' }, 'No reviews yet.'),
        ),
        h('div', { class: 'muted small' }, st.why),
      ),
      open
        ? h(
            'div',
            { class: 'review-compose' },
            textarea(state.ui.requests.comment, (v) => { state.ui.requests.comment = v; }, { id: 'review-comment', placeholder: mineReq ? 'Reply to the reviewers' : 'Leave a review comment', rows: 2 }),
            h(
              'div',
              { class: 'review-buttons' },
              button('Comment', { size: 'sm', onClick: () => { if (act(() => B.review(state.db, r.id, 'commented', state.ui.requests.comment), 'Comment added')) state.ui.requests.comment = ''; } }),
              button('Request changes', { size: 'sm', disabled: !st.canApprove, title: st.canApprove ? '' : mineReq ? 'You cannot review your own request.' : `A ${ROLE_LABEL[p.role]} cannot review.`, onClick: () => { if (act(() => B.review(state.db, r.id, 'changes_requested', state.ui.requests.comment), 'Requested changes')) state.ui.requests.comment = ''; } }),
              button('Approve', { size: 'sm', icon: 'check', disabled: !st.canApprove, title: st.canApprove ? '' : mineReq ? 'You cannot review your own request.' : `A ${ROLE_LABEL[p.role]} cannot approve.`, onClick: () => { if (act(() => B.review(state.db, r.id, 'approved', state.ui.requests.comment), `Approved #${r.number}`)) state.ui.requests.comment = ''; } }),
              h('span', { class: 'spacer' }),
              mineReq ? button('Withdraw', { size: 'sm', variant: 'ghost', onClick: () => act(() => B.withdraw(state.db, r.draftId), `Withdrew #${r.number}`) }) : button('Decline', { size: 'sm', variant: 'ghost', onClick: () => declineDialog(r) }),
              button(`Merge into main`, { variant: 'primary', icon: 'merge', disabled: !st.canMerge, title: st.why, onClick: () => mergeDialog(r) }),
            ),
          )
        : h('div', { class: 'muted small' }, r.status === 'merged' && r.mergedAt ? `Merged ${when(r.mergedAt)}.` : `This request is ${r.status}.`),
    ),
  );
}

export function requestsView(): HTMLElement {
  const ui = state.ui.requests;
  const counts = { open: listFor('open').length, merged: listFor('merged').length, closed: listFor('closed').length };
  const list = listFor(ui.filter);
  if (!ui.selected || !state.db.requests.some((r) => r.id === ui.selected && r.projectId === state.ui.projectId)) ui.selected = list[0]?.id ?? null;
  const sel = state.db.requests.find((r) => r.id === ui.selected);
  const left = h(
    'div',
    { class: 'pane pane-list' },
    h('div', { class: 'pane-tabs' }, tabs(ui.filter, [['open', 'Open', counts.open], ['merged', 'Merged', counts.merged], ['closed', 'Closed', counts.closed]], (v) => { ui.filter = v; ui.selected = null; render(); }, 'Request state')),
    h('div', { class: 'pane-scroll', 'data-scroll-key': 'req-list' }, list.length ? list.map(reqRow) : h('div', { class: 'change-none' }, `No ${ui.filter} change requests.`)),
  );
  const right = sel ? review(sel) : h('div', { class: 'pane pane-main' }, empty('request', 'No change request selected', 'Drafts become change requests when they are sent for review.'));
  return h('div', { class: 'split-view' }, left, right);
}
