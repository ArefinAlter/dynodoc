// Changes: GitHub Desktop's Changes tab with Git GUI's split into unstaged and staged lists.
// The changes are edits the add-ins captured on this computer (paragraph text, cell values,
// slide text) and that are not committed yet. Commit records the staged ones as a revision of
// each file on the current branch; Push sends them to the server.
import { h, cx } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, render, toast } from '../state.ts';
import * as B from '../data/backend.ts';
import { commitTarget } from '../lib/branchMenu.ts';
import { stats } from '../lib/diff.ts';
import { HOST_LABEL } from '../data/connectors.ts';
import type { WorkingChange } from '../data/types.ts';
import { button, checkbox, empty, input, textarea, segmented, select, avatar, notice, iconButton } from '../ui/controls.ts';
import { diffView } from '../ui/diffView.ts';
import { openMenu, confirmDialog } from '../ui/layer.ts';
import { act, branchContext, branchLabel, currentBranch, docOf, fileIcon, short } from './common.ts';
import { newDraftDialog, sync, fetchNow, sendForReviewDialog } from './actions.ts';
import { openInEditor } from './documents.ts';
import { isLocal } from '../data/live.ts';

const OP: Record<string, [string, string]> = { edit: ['M', 'Modified'], add: ['A', 'Added'], remove: ['D', 'Removed'] };

function labelOf(w: WorkingChange): string {
  if (w.edit.label) return w.edit.label;
  const b = B.blocksOf(state.db, w.docId, w.branch).find((x) => x.id === w.edit.blockId);
  return b?.label ?? w.edit.blockId;
}

export function commitStaged(): void {
  const c = state.ui.changes;
  const branch = currentBranch();
  const target = commitTarget(branchContext());
  if (!target.ok) return toast('warning', 'Cannot commit here', target.note);
  if (!c.summary.trim()) {
    toast('warning', 'A summary is required');
    document.getElementById('commit-summary')?.focus();
    return;
  }
  act(() => {
    const out = B.commitStaged(state.db, state.ui.projectId, branch, c.summary, c.description);
    c.summary = '';
    c.description = '';
    c.selected = null;
    return `Committed ${out.length} file${out.length === 1 ? '' : 's'} to ${branchLabel(branch)}`;
  });
}

function discardOne(w: WorkingChange): void {
  const run = () => act(() => { B.discard(state.db, [w.id]); }, 'Discarded the change');
  if (!state.prefs.prompts.discard) return void run();
  confirmDialog('Discard change', h('p', null, `Discard the captured change to ${labelOf(w)} in ${docOf(w.docId).filename}? It was never committed, so it cannot be brought back.`), 'Discard', run, { danger: true });
}

function rowMenu(anchor: HTMLElement, w: WorkingChange): void {
  openMenu(anchor, [
    { label: w.staged ? 'Unstage' : 'Stage', icon: w.staged ? 'down' : 'up', onSelect: () => { B.setStaged(state.db, [w.id], !w.staged); render(); } },
    { label: `Open in ${HOST_LABEL[w.source]}`, icon: 'external', onSelect: () => openInEditor(w.docId) },
    { label: 'Copy block label', icon: 'link', onSelect: () => void copy(labelOf(w)) },
    { label: '', separator: true },
    { label: 'Discard change…', icon: 'trash', danger: true, onSelect: () => discardOne(w) },
  ]);
}

async function copy(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast('success', 'Copied', text);
  } catch {
    toast('warning', 'Copying is not available here');
  }
}

function changeRow(w: WorkingChange): HTMLElement {
  const c = state.ui.changes;
  const d = docOf(w.docId);
  const [letter, word] = OP[w.edit.op];
  const el = h(
    'div',
    {
      class: cx('change-row', c.selected === w.id && 'is-selected'),
      role: 'option',
      'aria-selected': String(c.selected === w.id),
      tabindex: '0',
      'data-id': w.id,
      onClick: () => { c.selected = w.id; render(); },
      onKeydown: (e: KeyboardEvent) => {
        if (e.key === ' ') {
          e.preventDefault();
          B.setStaged(state.db, [w.id], !w.staged);
          render();
        } else if (e.key === 'Enter') {
          c.selected = w.id;
          render();
        }
      },
      onContextmenu: (e: MouseEvent) => {
        e.preventDefault();
        rowMenu(e.currentTarget as HTMLElement, w);
      },
    },
    checkbox(w.staged, (v) => { B.setStaged(state.db, [w.id], v); render(); }, `${w.staged ? 'Unstage' : 'Stage'} ${labelOf(w)}`),
    fileIcon(d.kind, 14),
    h('span', { class: 'change-path' }, h('span', { class: 'change-file' }, d.filename), h('span', { class: 'change-sep' }, ' › '), h('span', { class: 'change-label' }, labelOf(w))),
    h('span', { class: cx('op', `op-${w.edit.op}`), title: word }, letter),
  );
  return el;
}

function list(title: string, items: WorkingChange[], staged: boolean): HTMLElement {
  const all = items.length > 0;
  return h(
    'div',
    { class: 'change-group' },
    h(
      'div',
      { class: 'change-group-head' },
      checkbox(staged && all ? true : false, (v) => { B.setStaged(state.db, items.map((w) => w.id), v); render(); }, staged ? 'Unstage all' : 'Stage all', { disabled: !all }),
      h('span', { class: 'change-group-title' }, title),
      h('span', { class: 'count' }, String(items.length)),
      h('span', { class: 'spacer' }),
      all ? button(staged ? 'Unstage all' : 'Stage all', { size: 'sm', variant: 'ghost', onClick: () => { B.setStaged(state.db, items.map((w) => w.id), !staged); render(); } }) : null,
    ),
    h('div', { class: 'change-list', role: 'listbox', 'aria-label': title }, items.length ? items.map(changeRow) : h('div', { class: 'change-none' }, staged ? 'Stage changes to commit them.' : 'Nothing unstaged.')),
  );
}

function commitBox(staged: WorkingChange[]): HTMLElement {
  const c = state.ui.changes;
  const branch = currentBranch();
  const target = commitTarget(branchContext());
  const files = new Set(staged.map((w) => w.docId)).size;
  const tooLong = state.prefs.prompts.commitLength && c.summary.length > 60;
  const counter = h('span', { class: cx('commit-count', tooLong && 'is-warn'), id: 'commit-count' }, c.summary.length ? String(60 - c.summary.length) : '');
  const label = staged.length ? `Commit ${staged.length} change${staged.length === 1 ? '' : 's'} to ${branchLabel(branch)}` : `Commit to ${branchLabel(branch)}`;
  return h(
    'div',
    { class: 'commit-box' },
    h(
      'div',
      { class: 'commit-summary-row' },
      avatar(state.prefs.authorName),
      input(c.summary, (v) => {
        c.summary = v;
        const el = document.getElementById('commit-count');
        if (el) {
          el.textContent = v.length ? String(60 - v.length) : '';
          el.classList.toggle('is-warn', state.prefs.prompts.commitLength && v.length > 60);
        }
        const btn = document.getElementById('commit-button') as HTMLButtonElement | null;
        if (btn) btn.disabled = !v.trim() || !staged.length || !target.ok;
      }, { id: 'commit-summary', placeholder: 'Summary (required)', maxlength: 200 }),
      counter,
    ),
    textarea(c.description, (v) => { c.description = v; }, {
      id: 'commit-description',
      placeholder: 'Description',
      rows: 3,
      onKeydown: (e: KeyboardEvent) => {
        if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
          e.preventDefault();
          commitStaged();
        }
      },
    }),
    h(
      'div',
      { class: 'commit-options' },
      h('label', { class: 'inline-check is-disabled', title: 'Dynodoc history is append-only: a revision cannot be changed after it is made.' }, h('input', { type: 'checkbox', class: 'check', disabled: true }), 'Amend last commit'),
      h('span', { class: 'muted small' }, state.db.remote ? (isLocal() ? 'Saved in this workspace as you' : 'Pushed to the server as you') : state.prefs.signCommits ? 'Signed with this computer’s key' : 'Unsigned'),
    ),
    !target.ok ? notice('info', target.note) : null,
    button(label, { variant: 'primary', id: 'commit-button', class: 'commit-button', disabled: !c.summary.trim() || !staged.length || !target.ok, onClick: commitStaged }),
    !target.ok && branch === 'main' && staged.length ? button('Commit to a new draft…', { onClick: () => newDraftDialog(staged[0].docId), class: 'commit-button' }) : null,
    staged.length ? h('div', { class: 'muted small center' }, `${files} file${files === 1 ? '' : 's'} · one revision per file`) : null,
  );
}

function nothingHere(): HTMLElement {
  const ctx = branchContext();
  const cards: HTMLElement[] = [];
  if ((ctx.ahead ?? 0) > 0)
    cards.push(h('div', { class: 'suggest' }, h('div', null, h('div', { class: 'suggest-title' }, `Push ${ctx.ahead} commit${ctx.ahead === 1 ? '' : 's'} ${isLocal() ? 'to the workspace' : 'to the server'}`), h('div', { class: 'suggest-text' }, isLocal() ? 'Your commits are kept in this window until you push them into the workspace’s main line.' : 'Your commits are on this computer only until you push.')), button('Push', { variant: 'primary', icon: 'up', onClick: sync })));
  if (ctx.kind === 'draft' && ctx.state === 'private' && !(ctx.ahead ?? 0))
    cards.push(h('div', { class: 'suggest' }, h('div', null, h('div', { class: 'suggest-title' }, 'Send this draft for review'), h('div', { class: 'suggest-text' }, 'Open a change request so the team can review and merge it.')), button('Send for review', { onClick: () => sendForReviewDialog() })));
  if (isLocal()) {
    cards.push(h('div', { class: 'suggest' }, h('div', null, h('div', { class: 'suggest-title' }, 'Edit a file in Word, Excel or PowerPoint'), h('div', { class: 'suggest-text' }, 'Open a file from Documents; the copy opens in the program your computer uses for it. When you save it there, the changes arrive as a draft to review. Bind the project to a folder to pick up files you save yourself.')), button('Open Documents', { onClick: () => { state.ui.view = 'documents'; render(); } })));
    return h('div', { class: 'no-changes' }, empty('check', 'Nothing waiting', `No captured changes on ${branchLabel()}. Edits you save in an editor arrive as drafts, and the changes you make in this app are committed as you go.`), h('div', { class: 'suggests' }, cards));
  }
  cards.push(h('div', { class: 'suggest' }, h('div', null, h('div', { class: 'suggest-title' }, 'Fetch from the server'), h('div', { class: 'suggest-text' }, 'See new revisions, reviews and change requests.')), button('Fetch', { icon: 'sync', onClick: fetchNow })));
  cards.push(h('div', { class: 'suggest' }, h('div', null, h('div', { class: 'suggest-title' }, 'Edit in your editor'), h('div', { class: 'suggest-text' }, 'Editing happens in Word, Excel, PowerPoint or Google’s editors with the Dynodoc add-in. Captured edits appear here.')), button('Open a file…', { onClick: () => { state.ui.view = 'documents'; render(); } })));
  return h('div', { class: 'no-changes' }, empty('check', 'No captured changes', `Nothing waiting on ${branchLabel()}. Here are some friendly suggestions for what to do next.`), h('div', { class: 'suggests' }, cards));
}

export function changesView(): HTMLElement {
  const c = state.ui.changes;
  const branch = currentBranch();
  const all = B.working(state.db, state.ui.projectId, branch);
  const q = c.filter.toLowerCase();
  const shown = all.filter((w) => !q || `${docOf(w.docId).filename} ${labelOf(w)}`.toLowerCase().includes(q));
  const unstaged = shown.filter((w) => !w.staged);
  const staged = shown.filter((w) => w.staged);
  if (!c.selected || !all.some((w) => w.id === c.selected)) c.selected = (staged[0] ?? unstaged[0])?.id ?? null;
  const sel = all.find((w) => w.id === c.selected);

  const left = h(
    'div',
    { class: 'pane pane-list changes-pane' },
    h('div', { class: 'pane-filter' }, icon('filter', 14), input(c.filter, (v) => { c.filter = v; render(); }, { id: 'changes-filter', placeholder: 'Filter changes' })),
    h('div', { class: 'pane-scroll', 'data-scroll-key': 'changes-list' }, list('Unstaged changes', unstaged, false), list('Staged changes', staged, true)),
    commitBox(all.filter((w) => w.staged)),
  );

  let right: HTMLElement;
  if (!all.length) right = h('div', { class: 'pane pane-main' }, nothingHere());
  else if (!sel) right = h('div', { class: 'pane pane-main' }, empty('changes', 'Select a change', 'Choose a change at the left to see its diff.'));
  else {
    const d = docOf(sel.docId);
    const base = B.blocksOf(state.db, sel.docId, branch);
    const docChanges = all.filter((w) => w.docId === sel.docId);
    const st = stats(docChanges.map((w) => w.edit));
    const stagedIds = new Set(docChanges.filter((w) => w.staged).map((w) => w.edit.blockId));
    right = h(
      'div',
      { class: 'pane pane-main' },
      h(
        'div',
        { class: 'diff-head' },
        fileIcon(d.kind),
        h('span', { class: 'diff-path' }, h('strong', null, d.filename), h('span', { class: 'muted' }, ` on ${branchLabel(branch)}`)),
        h('span', { class: 'diff-stat' }, h('span', { class: 'plus' }, `+${st.wordsAdded}`), ' ', h('span', { class: 'minus' }, `−${st.wordsRemoved}`), h('span', { class: 'muted' }, ' words')),
        h('span', { class: 'spacer' }),
        h('span', { class: 'muted small' }, `Captured by ${HOST_LABEL[sel.source]} · ${short(sel.capturedAt)}`),
        segmented(state.prefs.diffLayout, [['unified', '', 'unified'], ['split', '', 'split']], (v) => { state.prefs.diffLayout = v; render(); }, 'Diff layout'),
        select(String(state.prefs.diffContext), [['0', 'No context'], ['1', '1 block'], ['2', '2 blocks'], ['3', '3 blocks'], ['5', '5 blocks'], ['999', 'Whole file']], (v) => { state.prefs.diffContext = Number(v); render(); }, { label: 'Context', class: 'select-sm' }),
        iconButton('external', `Open in ${HOST_LABEL[sel.source]}`, () => openInEditor(sel.docId)),
        iconButton('trash', 'Discard this change', () => discardOne(sel)),
      ),
      h(
        'div',
        { class: 'pane-scroll diff-scroll', 'data-scroll-key': 'changes-diff' },
        diffView(base, docChanges.map((w) => w.edit), {
          excluded: docChanges.filter((w) => !stagedIds.has(w.edit.blockId)).map((w) => w.edit.blockId),
          onToggle: (blockId, include) => {
            const w = docChanges.find((x) => x.edit.blockId === blockId);
            if (w) B.setStaged(state.db, [w.id], include);
            render();
          },
        }),
        h('p', { class: 'muted small diff-foot' }, 'A tick stages the change; dimmed changes are not staged. Captured changes stay on this computer until you commit and push.'),
      ),
    );
  }
  const split = h('div', { class: 'split-view' }, left, right);
  return split;
}
