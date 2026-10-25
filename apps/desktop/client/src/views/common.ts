// Helpers the views share: the current project and branch, names, file icons, times.
import { h, type Child } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, branchOf, toast, render } from '../state.ts';
import * as B from '../data/backend.ts';
import { relative, age } from '../lib/format.ts';
import type { Commit, Doc, Draft, FileKind, Role } from '../data/types.ts';
import { applyEdits, type Block } from '../lib/diff.ts';
import type { BranchContext } from '../lib/branchMenu.ts';
import { pill, type Tone } from '../ui/controls.ts';

export const db = () => state.db;
export const ui = () => state.ui;
export const prefs = () => state.prefs;

export function currentProject() {
  return B.project(state.db, state.ui.projectId);
}

export function currentBranch(): string {
  return branchOf();
}

export function currentDraft(): Draft | undefined {
  const b = currentBranch();
  return b === 'main' ? undefined : B.draft(state.db, b);
}

export function branchLabel(branch = currentBranch()): string {
  return B.branchName(state.db, branch);
}

export function personName(id: string): string {
  return id === state.db.me ? 'You' : B.person(state.db, id).name;
}

export function isAgent(id: string): boolean {
  return Boolean(state.db.people.find((p) => p.id === id)?.agent);
}

export const KIND_ICON: Record<FileKind, string> = { document: 'word', spreadsheet: 'sheet', presentation: 'slides' };
export const KIND_LABEL: Record<FileKind, string> = { document: 'Document', spreadsheet: 'Spreadsheet', presentation: 'Presentation' };

export function fileIcon(kind: FileKind, size = 15): SVGElement {
  return icon(KIND_ICON[kind], size, `icon kind-${kind}`);
}

export const ROLE_LABEL: Record<Role, string> = { owner: 'Owner', manager: 'Manager', editor: 'Editor', approver: 'Reviewer', reviewer: 'Contributor', viewer: 'Viewer' };
export const ROLE_SUMMARY: Record<Role, string> = {
  owner: 'Everything, including transferring the project and deleting it.',
  manager: 'Manages people, rules and settings; edits and merges.',
  editor: 'Edits main where it is not protected, opens drafts and change requests, merges where the rules allow.',
  approver: 'Approves or requests changes on change requests; comments.',
  reviewer: 'Comments and suggests through drafts; cannot approve.',
  viewer: 'Reads files and their history.',
};

export function when(ms: number): string {
  return relative(ms, Date.now(), state.prefs);
}
export function short(ms: number): string {
  return age(ms, Date.now());
}

export function rev(c: Commit): string {
  return `${c.branch === 'main' ? 'r' : 'd'}${c.seq}`;
}

export function branchContext(branch = currentBranch(), projectId = state.ui.projectId): BranchContext {
  const p = B.project(state.db, projectId);
  const d = branch === 'main' ? undefined : B.draft(state.db, branch);
  const req = d ? state.db.requests.find((r) => r.draftId === d.id && r.status === 'open') : undefined;
  return {
    kind: d ? 'draft' : 'main',
    state: d?.state,
    behind: d ? B.behind(state.db, d) : 0,
    ahead: B.aheadOf(state.db, projectId, branch),
    uncommitted: B.working(state.db, projectId, branch).length,
    role: p.role,
    requestSatisfied: req ? B.reviewState(state.db, req).satisfied : false,
    protectedMain: p.rules.protectMain,
    mine: d ? d.authorId === state.db.me : true,
  };
}

/** The blocks a commit started from (its branch rebuilt to just before it). */
export function commitBase(c: Commit): Block[] {
  const later = B.branchCommits(state.db, c.docId, c.branch)
    .filter((x) => x.seq >= c.seq)
    .reverse();
  let blocks = B.blocksOf(state.db, c.docId, c.branch);
  for (const x of later) blocks = applyEdits(blocks, B.invert(x.edits));
  return blocks;
}

export function requestTone(status: string): Tone {
  return status === 'open' ? 'success' : status === 'merged' ? 'primary' : status === 'declined' ? 'error' : 'neutral';
}

export function draftTone(s: string): Tone {
  return s === 'private' ? 'neutral' : s === 'in_review' ? 'info' : s === 'merged' ? 'primary' : 'neutral';
}

export function draftWord(s: string): string {
  return s === 'in_review' ? 'in review' : s;
}

/** Runs an operation of the scripted backend: an error becomes a toast, success re-renders. */
export function act(fn: () => void | string, success?: string): boolean {
  try {
    const out = fn();
    if (success || typeof out === 'string') toast('success', success ?? (out as string));
    render();
    return true;
  } catch (e) {
    toast('error', e instanceof Error ? e.message : String(e));
    render();
    return false;
  }
}

export function docOf(id: string): Doc {
  return B.doc(state.db, id);
}

export function verifyPill(commitId: string): HTMLElement {
  const v = state.ui.verified[commitId];
  if (!v) return pill('unchecked', 'neutral', { title: 'Not verified in this session. Use "Verify hash chain".' });
  return v.ok ? pill('verified', 'success', { title: 'Chain hash recomputed and equal' }) : pill('broken', 'error');
}

export function verifyIcon(commitId: string): HTMLElement {
  const v = state.ui.verified[commitId];
  const name = !v ? 'shield' : v.ok ? 'shield' : 'shieldX';
  const cls = !v ? 'vf-unchecked' : v.ok ? 'vf-ok' : 'vf-bad';
  const label = !v ? 'Chain not verified yet' : v.ok ? 'Chain verified' : 'Chain broken here';
  return h('span', { class: `vf ${cls}`, title: label, 'aria-label': label }, icon(name, 14));
}

export function runVerify(docId: string): { ok: number; bad: number } {
  const res = B.verify(state.db, docId);
  let ok = 0, bad = 0;
  for (const r of res) {
    state.ui.verified[r.commitId] = { ok: r.ok, at: Date.now() };
    if (r.ok) ok++;
    else bad++;
  }
  return { ok, bad };
}

export function viewHeader(title: string, subtitle: Child, ...actions: Child[]): HTMLElement {
  return h('div', { class: 'view-head' }, h('div', null, h('h1', { class: 'view-title' }, title), subtitle ? h('div', { class: 'view-sub' }, subtitle) : null), actions.length ? h('div', { class: 'view-actions' }, actions) : null);
}
