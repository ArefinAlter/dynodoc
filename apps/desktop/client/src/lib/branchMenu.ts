// The Branch menu, in the shape of GitHub Desktop's, mapped to what Dynodoc has. Dynodoc's
// branches are the team version (`main`) and personal drafts, each a private branch of one file
// (POST /documents/{id}/drafts). A draft is shared by sending it for review (a change request),
// can be updated from main or rebased onto it (/drafts/{id}/sync), compared with main, and
// withdrawn (/drafts/{id}/withdraw: closed and kept as history). A change request is merged by a
// person when the review rules are met. Dynodoc has no rename, squash or stash: those items are
// shown greyed with the reason, so nobody expects them. Pure; tested in client/test/branch-menu.test.mjs.

export type BranchKind = 'main' | 'draft';
export type DraftState = 'private' | 'in_review' | 'merged' | 'withdrawn';

export interface BranchContext {
  kind: BranchKind;
  /** For a draft. */
  state?: DraftState;
  /** Revisions of main the draft does not have yet. */
  behind?: number;
  /** Commits on this branch not yet pushed to the server. */
  ahead?: number;
  /** Captured changes not yet committed. */
  uncommitted?: number;
  /** The person's role in the project (owner, manager, editor, approver, reviewer, viewer). */
  role: string;
  /** Whether a change request from this draft is open and its review rules are met. */
  requestSatisfied?: boolean;
  /** The project protects main: edits go through review. */
  protectedMain?: boolean;
  /** The draft is someone else's (shown in review). */
  mine?: boolean;
}

export type BranchActionId =
  | 'new'
  | 'rename'
  | 'delete'
  | 'discard'
  | 'stash'
  | 'update'
  | 'compare'
  | 'merge'
  | 'squash'
  | 'rebase'
  | 'request'
  | 'view';

export interface BranchAction {
  id: BranchActionId;
  label: string;
  shortcut: string;
  /** False: Dynodoc has no such operation; always disabled. */
  supported: boolean;
  enabled: boolean;
  /** Why it is disabled (or what it does in Dynodoc's words when enabled). */
  note: string;
  /** Starts a group (a separator above it). */
  group?: boolean;
}

const WRITERS = new Set(['owner', 'manager', 'editor']);
/** Roles that may propose changes (and so own a draft). Contributors (stored `reviewer`) suggest. */
const PROPOSERS = new Set(['owner', 'manager', 'editor', 'approver', 'reviewer']);

export function canPropose(role: string): boolean {
  return PROPOSERS.has(role);
}

export function canWriteMain(role: string, protectedMain: boolean): boolean {
  return WRITERS.has(role) && !protectedMain;
}

/** The menu for the current branch, in GitHub Desktop's order. */
export function branchMenu(c: BranchContext): BranchAction[] {
  const draft = c.kind === 'draft';
  const live = draft && (c.state === 'private' || c.state === 'in_review');
  const mine = c.mine !== false;
  const propose = canPropose(c.role);
  const a = (id: BranchActionId, label: string, shortcut: string, supported: boolean, enabled: boolean, note: string, group = false): BranchAction => ({
    id,
    label,
    shortcut,
    supported,
    enabled: supported && enabled,
    note,
    group,
  });
  return [
    a('new', 'New draft…', 'Ctrl+Shift+N', true, propose, propose ? 'A personal draft of one file, based on the latest main.' : 'Your role cannot propose changes.'),
    a('rename', 'Rename…', 'Ctrl+Shift+R', false, false, 'Dynodoc drafts keep the name they were made with.'),
    a(
      'delete',
      'Withdraw draft…',
      'Ctrl+Shift+D',
      true,
      live && mine,
      !draft ? 'main cannot be withdrawn.' : !live ? 'This draft is already closed.' : !mine ? 'Only its author can withdraw a draft.' : 'Closes the draft; it stays in history as withdrawn.',
    ),
    a('discard', 'Discard all changes…', 'Ctrl+Shift+Backspace', true, (c.uncommitted ?? 0) > 0, (c.uncommitted ?? 0) > 0 ? 'Drops the captured changes on this computer.' : 'There are no captured changes.', true),
    a('stash', 'Stash all changes', 'Ctrl+Shift+S', false, false, 'Dynodoc has no stash: commit to a draft instead.'),
    a(
      'update',
      'Update from main',
      'Ctrl+Shift+U',
      true,
      live && mine && (c.behind ?? 0) > 0,
      !draft ? 'You are on main.' : (c.behind ?? 0) > 0 ? `Brings in ${c.behind} newer revision${c.behind === 1 ? '' : 's'} of main.` : 'The draft has the latest main.',
      true,
    ),
    a('compare', 'Compare with main', 'Ctrl+Shift+B', true, draft, draft ? 'Shows what the draft changes against main.' : 'Choose a draft to compare.'),
    a(
      'merge',
      'Merge into main…',
      'Ctrl+Shift+M',
      true,
      draft && c.state === 'in_review' && Boolean(c.requestSatisfied),
      !draft
        ? 'Choose a draft. Drafts reach main through a change request.'
        : c.state !== 'in_review'
          ? 'Send the draft for review first.'
          : c.requestSatisfied
            ? 'Opens the review to merge; a person presses Merge.'
            : 'The review rules are not met yet.',
    ),
    a('squash', 'Squash and merge…', 'Ctrl+Shift+H', false, false, 'Dynodoc merges the selected changes of a request; it has no squash.'),
    a(
      'rebase',
      'Rebase onto main…',
      'Ctrl+Shift+E',
      true,
      live && mine && (c.behind ?? 0) > 0,
      !draft ? 'You are on main.' : (c.behind ?? 0) > 0 ? 'Replays the draft on the latest main; you choose for each conflict.' : 'Nothing to rebase: the draft has the latest main.',
    ),
    a(
      'request',
      'Send for review',
      'Ctrl+Alt+R',
      true,
      draft && c.state === 'private' && mine && (c.ahead ?? 0) === 0,
      !draft ? 'Choose a draft to send for review.' : c.state !== 'private' ? 'This draft is already in review or closed.' : (c.ahead ?? 0) > 0 ? 'Push the draft first.' : 'Opens a change request from this draft.',
      true,
    ),
    a('view', 'View change request', 'Ctrl+Alt+P', true, draft && c.state === 'in_review', draft && c.state === 'in_review' ? 'Opens the review.' : 'No open change request.'),
  ];
}

/** Whether a direct commit to the current branch is allowed, and if not, why. */
export function commitTarget(c: BranchContext): { ok: boolean; note: string } {
  if (c.kind === 'draft') {
    if (c.state !== 'private' && c.state !== 'in_review') return { ok: false, note: 'This draft is closed.' };
    if (c.mine === false) return { ok: false, note: 'This draft belongs to someone else.' };
    return { ok: canPropose(c.role), note: canPropose(c.role) ? '' : 'Your role cannot propose changes.' };
  }
  if (!WRITERS.has(c.role)) return { ok: false, note: 'Your role cannot write to main. Commit to a draft and send it for review.' };
  if (c.protectedMain) return { ok: false, note: 'main is protected in this project: commit to a draft and send it for review.' };
  return { ok: true, note: '' };
}
