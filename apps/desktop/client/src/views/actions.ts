// Actions shared by the sidebar, the top bar, the Branch menu, the command palette and the
// keyboard: fetch and push, switching project and branch, and the branch operations.
import { h } from '../dom.ts';
import { state, render, toast, go } from '../state.ts';
import * as B from '../data/backend.ts';
import { INCOMING } from '../data/sample.ts';
import { branchMenu, type BranchActionId } from '../lib/branchMenu.ts';
import { openDialog, closeDialog, confirmDialog, openMenu, type MenuItem } from '../ui/layer.ts';
import { button, input, textarea, field, select, notice } from '../ui/controls.ts';
import { act, branchContext, currentBranch, currentDraft, currentProject, docOf, branchLabel, draftWord } from './common.ts';
import { icon } from '../icons.ts';

export function switchProject(id: string): void {
  state.ui.projectId = id;
  state.ui.changes.selected = null;
  state.ui.history.selected = null;
  state.ui.requests.selected = null;
  state.ui.documents.selected = null;
  render();
}

export function switchBranch(branch: string): void {
  const from = currentBranch();
  if (from === branch) return;
  const pending = B.working(state.db, state.ui.projectId, from).length;
  const doSwitch = () => {
    state.ui.branchByProject[state.ui.projectId] = branch;
    state.ui.changes.selected = null;
    state.ui.history.selected = null;
    render();
  };
  if (pending && state.prefs.prompts.switchWithChanges === 'ask') {
    confirmDialog(
      'Switch branch',
      h('p', null, `You have ${pending} captured change${pending === 1 ? '' : 's'} on ${branchLabel(from)}. They stay on ${branchLabel(from)} and come back when you switch back. Dynodoc does not carry captured changes between a draft and main.`),
      `Switch to ${branchLabel(branch)}`,
      doSwitch,
    );
  } else doSwitch();
}

export function fetchNow(): void {
  if (state.ui.fetching) return;
  state.ui.fetching = true;
  render();
  setTimeout(() => {
    state.ui.fetching = false;
    state.db.lastFetched = Date.now();
    const next = INCOMING[state.db.incoming];
    if (next) {
      state.db.incoming += 1;
      toast('info', 'Fetched', next.apply(state.db));
    } else toast('success', 'Up to date', 'Nothing new on the server.');
    render();
  }, 650);
}

/** Ctrl+Shift+P: push local commits if there are any, otherwise fetch. */
export function sync(): void {
  const ahead = B.aheadOf(state.db, state.ui.projectId, currentBranch());
  if (!ahead) return fetchNow();
  state.ui.fetching = true;
  render();
  setTimeout(() => {
    state.ui.fetching = false;
    const n = B.push(state.db, state.ui.projectId, currentBranch());
    state.db.lastFetched = Date.now();
    B.log(state.db, { actorId: state.db.me, projectId: state.ui.projectId, kind: 'commit', summary: `pushed ${n} commit${n === 1 ? '' : 's'} to ${branchLabel()}` });
    toast('success', `Pushed ${n} commit${n === 1 ? '' : 's'} to ${branchLabel()}`, 'Signed with this computer’s key and added to the server’s history.');
    render();
  }, 700);
}

// ---- branch operations ------------------------------------------------------------------------

export function newDraftDialog(docId?: string): void {
  const docs = B.docsOf(state.db, state.ui.projectId);
  let target = docId ?? docs[0]?.id ?? '';
  let name = '';
  let error = '';
  const create = () => {
    try {
      const d = B.newDraft(state.db, target, name);
      closeDialog();
      state.ui.branchByProject[state.ui.projectId] = d.id;
      toast('success', `Created the draft ${d.name}`, `Based on main r${d.baseSeq} of ${docOf(target).title}.`);
      render();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      render();
    }
  };
  openDialog({
    title: 'New draft',
    width: 520,
    body: () => [
      h('p', { class: 'dialog-text' }, 'A draft is a private branch of one file, based on the latest main. Commit to it, then send it for review.'),
      field('Name', input(name, (v) => { name = v; }, { id: 'draft-name', placeholder: 'remote-work-update', autofocus: true, onEnter: create, maxlength: 63 })),
      field('File', select(target, docs.map((d) => [d.id, `${d.filename} (main r${B.mainSeq(state.db, d.id)})`] as [string, string]), (v) => { target = v; }, { id: 'draft-file', label: 'File' })),
      error ? notice('error', error) : null,
    ],
    actions: () => [button('Cancel', { onClick: closeDialog }), button('Create draft', { variant: 'primary', onClick: create })],
  });
}

export function sendForReviewDialog(draftId = currentBranch()): void {
  const d = B.draft(state.db, draftId);
  if (!d) return;
  let name = '';
  let note = '';
  const edits = B.draftEdits(state.db, d);
  openDialog({
    title: 'Send for review',
    width: 560,
    body: () => [
      h('p', { class: 'dialog-text' }, `Opens a change request from ${d.name} into main for ${docOf(d.docId).title}: ${edits.length} change${edits.length === 1 ? '' : 's'}. Reviewers see it in Change requests; a person merges it when the project's rules are met.`),
      field('Title', input(name, (v) => { name = v; }, { id: 'cr-title', placeholder: d.name, autofocus: true, maxlength: 120 })),
      field('Message', textarea(note, (v) => { note = v; }, { id: 'cr-note', rows: 4, placeholder: 'What changed and why' })),
    ],
    actions: () => [
      button('Cancel', { onClick: closeDialog }),
      button('Open change request', {
        variant: 'primary',
        onClick: () => {
          if (act(() => {
            const r = B.sendForReview(state.db, d.id, name, note);
            state.ui.requests.selected = r.id;
            state.ui.requests.filter = 'open';
            return `Opened change request #${r.number}`;
          })) {
            closeDialog();
            go('requests');
          }
        },
      }),
    ],
  });
}

export function rebaseDialog(draftId = currentBranch(), mode: 'update' | 'rebase' = 'rebase'): void {
  const d = B.draft(state.db, draftId);
  if (!d) return;
  const conflicts = B.conflictsOf(state.db, d);
  const choices: Record<string, 'team' | 'draft'> = {};
  const behind = B.behind(state.db, d);
  openDialog({
    title: mode === 'update' ? 'Update from main' : 'Rebase onto main',
    width: 680,
    body: () => [
      h('p', { class: 'dialog-text' }, `${d.name} is based on main r${d.baseSeq}; main is at r${B.mainSeq(state.db, d.docId)} (${behind} newer revision${behind === 1 ? '' : 's'}). ${mode === 'update' ? 'Bringing main in' : 'Rebasing'} replays your draft on the latest main.`),
      conflicts.length
        ? [
            notice('warning', `${conflicts.length} block${conflicts.length === 1 ? '' : 's'} changed on both sides. Choose which version to keep for each.`),
            h(
              'div',
              { class: 'conflicts' },
              conflicts.map((c) =>
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
                        { class: `conflict-choice ${choices[c.blockId] === side ? 'is-on' : ''}` },
                        h('input', { type: 'radio', name: `cf-${c.blockId}`, checked: choices[c.blockId] === side, onChange: () => { choices[c.blockId] = side; render(); } }),
                        h('span', { class: 'conflict-side' }, side === 'team' ? "Team's newer version" : 'Your version'),
                        h('span', { class: 'conflict-text' }, side === 'team' ? c.team : c.draft),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ]
        : notice('success', 'No conflicts: none of the blocks your draft changes were changed on main.'),
    ],
    actions: () => [
      button('Cancel', { onClick: closeDialog }),
      button(mode === 'update' ? 'Update draft' : 'Rebase draft', {
        variant: 'primary',
        disabled: conflicts.some((c) => !choices[c.blockId]),
        onClick: () => {
          if (act(() => { B.rebase(state.db, d.id, choices); return `${d.name} now builds on main r${d.baseSeq}`; })) closeDialog();
        },
      }),
    ],
  });
}

export function withdrawDraft(draftId = currentBranch()): void {
  const d = B.draft(state.db, draftId);
  if (!d) return;
  const run = () => {
    if (act(() => B.withdraw(state.db, d.id), `Withdrew ${d.name}`)) {
      if (currentBranch() === d.id) state.ui.branchByProject[state.ui.projectId] = 'main';
      render();
    }
  };
  if (!state.prefs.prompts.withdraw) return run();
  confirmDialog('Withdraw draft', h('p', null, `Withdraw ${d.name}? It is closed and kept in history as withdrawn; its open change request is withdrawn too. This cannot be undone, but you can start a new draft.`), 'Withdraw draft', run, { danger: true });
}

export function discardAll(): void {
  const list = B.working(state.db, state.ui.projectId, currentBranch());
  if (!list.length) return;
  const run = () => act(() => { B.discard(state.db, list.map((w) => w.id)); }, `Discarded ${list.length} change${list.length === 1 ? '' : 's'}`);
  if (!state.prefs.prompts.discard) return void run();
  confirmDialog('Discard all changes', h('p', null, `Discard all ${list.length} captured changes on ${branchLabel()}? They were never committed, so they cannot be brought back from history. The documents in your editor keep their text.`), 'Discard changes', run, { danger: true });
}

export function compareWithMain(draftId = currentBranch()): void {
  if (draftId === 'main') return;
  state.ui.history.mode = 'compare';
  state.ui.history.compareBranch = draftId;
  go('history');
}

export function openRequestFor(draftId = currentBranch()): void {
  const r = state.db.requests.find((x) => x.draftId === draftId && x.status === 'open');
  if (!r) return;
  state.ui.requests.selected = r.id;
  state.ui.requests.filter = 'open';
  go('requests');
}

export function runBranchAction(id: BranchActionId): void {
  const ctx = branchContext();
  const item = branchMenu(ctx).find((a) => a.id === id);
  if (!item || !item.enabled) {
    if (item) toast('info', item.label.replace('…', ''), item.note);
    return;
  }
  switch (id) {
    case 'new':
      return newDraftDialog(currentDraft()?.docId);
    case 'delete':
      return withdrawDraft();
    case 'discard':
      return discardAll();
    case 'update':
      return rebaseDialog(currentBranch(), 'update');
    case 'rebase':
      return rebaseDialog(currentBranch(), 'rebase');
    case 'compare':
      return compareWithMain();
    case 'merge':
    case 'view':
      return openRequestFor();
    case 'request':
      return sendForReviewDialog();
    default:
      return;
  }
}

export function branchMenuItems(): MenuItem[] {
  const items: MenuItem[] = [];
  for (const a of branchMenu(branchContext())) {
    if (a.group) items.push({ label: '', separator: true });
    items.push({ label: a.label, shortcut: a.shortcut, disabled: !a.enabled, note: a.enabled ? undefined : a.note, onSelect: () => runBranchAction(a.id) });
  }
  return items;
}

export function openBranchMenu(anchor: HTMLElement): void {
  openMenu(anchor, branchMenuItems(), { width: 320 });
}

// ---- the project and branch lists (GitHub Desktop's dropdowns) ----------------------------------

let listFilter = '';

export function openProjectList(anchor: HTMLElement): void {
  listFilter = '';
  const build = (): MenuItem[] => {
    const q = listFilter.toLowerCase();
    const all = state.db.projects.filter((p) => p.name.toLowerCase().includes(q));
    const recent = [...all].sort((a, b) => b.updatedAt - a.updatedAt).slice(0, 2);
    const item = (p: (typeof all)[number]): MenuItem => ({ label: p.name, icon: 'repo', checked: p.id === state.ui.projectId ? true : undefined, note: `${B.docsOf(state.db, p.id).length} files · ${p.private ? 'private' : 'shared'}`, onSelect: () => switchProject(p.id) });
    return [{ label: '', heading: 'Recent' }, ...recent.map(item), { label: '', heading: 'All projects' }, ...all.map(item)];
  };
  const reopen = () => openMenu(anchor, build(), { width: 320, extra: filterBox('Filter projects', reopen), className: 'menu-list-picker' });
  reopen();
}

export function openBranchList(anchor: HTMLElement): void {
  listFilter = '';
  const build = (): MenuItem[] => {
    const q = listFilter.toLowerCase();
    const drafts = state.db.drafts.filter((d) => d.projectId === state.ui.projectId && d.name.includes(q));
    const mine = drafts.filter((d) => d.authorId === state.db.me && (d.state === 'private' || d.state === 'in_review'));
    const others = drafts.filter((d) => d.authorId !== state.db.me && d.state === 'in_review');
    const closed = drafts.filter((d) => d.state === 'merged' || d.state === 'withdrawn');
    const row = (d: (typeof drafts)[number]): MenuItem => ({
      label: d.name,
      icon: 'branch',
      checked: currentBranch() === d.id ? true : undefined,
      note: `${docOf(d.docId).filename} · ${draftWord(d.state)}${B.behind(state.db, d) ? ` · ${B.behind(state.db, d)} behind` : ''}`,
      onSelect: () => switchBranch(d.id),
    });
    const out: MenuItem[] = [{ label: '', heading: 'Default branch' }];
    if ('main'.includes(q)) out.push({ label: 'main', icon: 'branch', checked: currentBranch() === 'main' ? true : undefined, note: 'The team version', onSelect: () => switchBranch('main') });
    if (mine.length) out.push({ label: '', heading: 'Your drafts' }, ...mine.map(row));
    if (others.length) out.push({ label: '', heading: 'In review by others' }, ...others.map(row));
    if (closed.length) out.push({ label: '', heading: 'Closed' }, ...closed.slice(0, 6).map(row));
    out.push({ label: '', separator: true }, { label: 'New draft…', icon: 'plus', shortcut: 'Ctrl+Shift+N', onSelect: () => runBranchAction('new') });
    return out;
  };
  const reopen = () => openMenu(anchor, build(), { width: 340, extra: filterBox('Filter branches', reopen), className: 'menu-list-picker' });
  reopen();
}

function filterBox(placeholder: string, reopen: () => void): HTMLElement {
  const el = h('div', { class: 'menu-filter' }, icon('search', 14), h('input', {
    class: 'menu-filter-input',
    id: 'menu-filter',
    value: listFilter,
    placeholder,
    'aria-label': placeholder,
    spellcheck: 'false',
    onInput: (e: Event) => {
      listFilter = (e.target as HTMLInputElement).value;
      const pos = (e.target as HTMLInputElement).selectionStart ?? listFilter.length;
      reopen();
      const f = document.getElementById('menu-filter') as HTMLInputElement | null;
      f?.focus();
      f?.setSelectionRange(pos, pos);
    },
    onKeydown: (e: KeyboardEvent) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        (document.querySelector('.menu .menu-item:not(.is-disabled)') as HTMLElement | null)?.focus();
      }
    },
  }));
  return el;
}

export function projectName(): string {
  return currentProject().name;
}
