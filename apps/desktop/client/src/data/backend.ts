// The scripted backend. It stands in for a Dynodoc server when the client is not signed in to one
// (and always in the hosted copy): an in-memory database seeded with a neutral sample project, and
// the operations the interface needs, each following Dynodoc's rules (append-only history with a
// hash chain, drafts per file, change requests merged by a person when the review rules are met).
// Nothing here makes a network call or touches storage.
import { applyEdits, compareBlocks, type Block, type Edit } from '../lib/diff.ts';
import { chainHash, sha256, GENESIS } from '../lib/sha256.ts';
import type { ActivityItem, ChangeRequest, Commit, CommitKind, Conflict, Db, Doc, Draft, Project, Role, Verdict, WorkingChange } from './types.ts';

let counter = 0;
export function uid(prefix: string): string {
  counter += 1;
  return `${prefix}_${Date.now().toString(36)}${counter.toString(36)}`;
}

export function canonical(edits: Edit[]): string {
  return JSON.stringify(edits.map((e) => [e.op, e.blockId, e.before ?? null, e.after ?? null, e.afterId ?? null, e.label ?? null]));
}

// ---- lookups -----------------------------------------------------------------------------

export function doc(db: Db, id: string): Doc {
  const d = db.docs.find((x) => x.id === id);
  if (!d) throw new Error('No such file.');
  return d;
}
export function project(db: Db, id: string): Project {
  const p = db.projects.find((x) => x.id === id);
  if (!p) throw new Error('No such project.');
  return p;
}
export function draft(db: Db, id: string): Draft | undefined {
  return db.drafts.find((x) => x.id === id);
}
export function person(db: Db, id: string) {
  return db.people.find((p) => p.id === id) ?? { id, name: 'Unknown', email: '' };
}
export function docsOf(db: Db, projectId: string): Doc[] {
  return db.docs.filter((d) => d.projectId === projectId);
}

/** Commits of one file on one branch, oldest first. */
export function branchCommits(db: Db, docId: string, branch: string): Commit[] {
  return db.commits.filter((c) => c.docId === docId && c.branch === branch).sort((a, b) => a.seq - b.seq);
}

export function mainSeq(db: Db, docId: string): number {
  const list = branchCommits(db, docId, 'main');
  return list.length ? list[list.length - 1].seq : 0;
}

/** The blocks of main as they were at revision `seq` (later revisions undone). */
export function blocksAt(db: Db, docId: string, seq: number): Block[] {
  let blocks = doc(db, docId).blocks;
  const later = branchCommits(db, docId, 'main').filter((c) => c.seq > seq).reverse();
  for (const c of later) blocks = applyEdits(blocks, invert(c.edits));
  return blocks;
}

export function blocksOf(db: Db, docId: string, branch: string): Block[] {
  if (branch === 'main') return doc(db, docId).blocks;
  const d = draft(db, branch);
  return d ? d.blocks : doc(db, docId).blocks;
}

/** Revisions of main the draft does not have. */
export function behind(db: Db, d: Draft): number {
  return Math.max(0, mainSeq(db, d.docId) - d.baseSeq);
}

/** Commits not yet on the server. */
export function aheadOf(db: Db, projectId: string, branch: string): number {
  const ids = new Set(docsOf(db, projectId).map((d) => d.id));
  return db.commits.filter((c) => ids.has(c.docId) && c.branch === branch && c.local).length;
}

// ---- the hash chain ----------------------------------------------------------------------

function lastChain(db: Db, docId: string, branch: string): string {
  const list = branchCommits(db, docId, branch);
  if (list.length) return list[list.length - 1].chainHash;
  if (branch === 'main') return GENESIS;
  const d = draft(db, branch);
  const base = d ? branchCommits(db, docId, 'main').filter((c) => c.seq <= d.baseSeq) : [];
  return base.length ? base[base.length - 1].chainHash : GENESIS;
}

export interface CommitInput {
  docId: string;
  branch: string;
  edits: Edit[];
  summary: string;
  description?: string;
  authorId: string;
  at?: number;
  kind?: CommitKind;
  mergedFrom?: string;
  local?: boolean;
}

/** Appends a commit, applies its edits to the branch's blocks and chains its hash. */
export function addCommit(db: Db, input: CommitInput): Commit {
  const branchList = branchCommits(db, input.docId, input.branch);
  const d = input.branch === 'main' ? undefined : draft(db, input.branch);
  const seq = branchList.length ? branchList[branchList.length - 1].seq + 1 : d ? 1 : 1;
  const contentHash = sha256(canonical(input.edits));
  const prev = lastChain(db, input.docId, input.branch);
  const commit: Commit = {
    id: uid('rev'),
    docId: input.docId,
    branch: input.branch,
    seq,
    summary: input.summary,
    description: input.description ?? '',
    authorId: input.authorId,
    at: input.at ?? Date.now(),
    edits: input.edits,
    kind: input.kind ?? 'revision',
    mergedFrom: input.mergedFrom,
    contentHash,
    prevChainHash: prev,
    chainHash: chainHash(prev, contentHash),
    local: input.local ?? false,
  };
  db.commits.push(commit);
  if (input.branch === 'main') {
    const target = doc(db, input.docId);
    target.blocks = applyEdits(target.blocks, input.edits);
  } else if (d) {
    d.blocks = applyEdits(d.blocks, input.edits);
  }
  return commit;
}

export interface Verification {
  commitId: string;
  ok: boolean;
  detail: string;
}

/** Recomputes every chain hash of a file's history (main and its drafts) from the edits. */
export function verify(db: Db, docId: string): Verification[] {
  const out: Verification[] = [];
  if (db.remote) {
    // A real server's history: the server replays the chain itself (its hashes are over the
    // engine's events, not over the client's edits). Revisions before a break verify.
    const check = db.remote.verify[docId] ?? { ok: true };
    for (const c of db.commits.filter((x) => x.docId === docId)) {
      if (c.local) continue;
      if (c.branch !== 'main') {
        out.push({ commitId: c.id, ok: true, detail: 'a draft revision; its changes join the chain when merged' });
        continue;
      }
      const ok = check.ok || (check.brokenAt !== undefined && c.seq < check.brokenAt);
      out.push({ commitId: c.id, ok, detail: ok ? 'verified by the server: the chain replays to this hash' : check.detail ?? `the chain breaks at event ${check.brokenAt ?? '?'}` });
    }
    return out;
  }
  const branches = new Set(db.commits.filter((c) => c.docId === docId).map((c) => c.branch));
  for (const branch of branches) {
    let prev = branch === 'main' ? GENESIS : '';
    for (const c of branchCommits(db, docId, branch)) {
      if (!prev) prev = c.prevChainHash; // a draft starts from main's chain at its base
      const content = sha256(canonical(c.edits));
      const ok = content === c.contentHash && c.prevChainHash === prev && chainHash(prev, content) === c.chainHash;
      out.push({ commitId: c.id, ok, detail: ok ? 'chain hash recomputed and equal' : content !== c.contentHash ? 'content hash differs from the edits' : 'chain hash does not follow from the previous one' });
      prev = c.chainHash;
    }
  }
  return out;
}

// ---- activity ----------------------------------------------------------------------------

export function log(db: Db, item: Omit<ActivityItem, 'id' | 'at'> & { at?: number }): void {
  db.activity.unshift({ id: uid('act'), at: item.at ?? Date.now(), ...item });
}

// ---- working changes (captured by the add-ins on this computer) ----------------------------

export function working(db: Db, projectId: string, branch: string): WorkingChange[] {
  const ids = new Set(docsOf(db, projectId).map((d) => d.id));
  return db.working.filter((w) => ids.has(w.docId) && w.branch === branch);
}

export function setStaged(db: Db, ids: string[], staged: boolean): void {
  for (const w of db.working) if (ids.includes(w.id)) w.staged = staged;
}

export function discard(db: Db, ids: string[]): number {
  const before = db.working.length;
  db.working = db.working.filter((w) => !ids.includes(w.id));
  return before - db.working.length;
}

/** Commits the staged changes of the current branch: one revision per file (Dynodoc's history is
 *  per file), each on this computer until pushed. */
export function commitStaged(db: Db, projectId: string, branch: string, summary: string, description: string): Commit[] {
  const staged = working(db, projectId, branch).filter((w) => w.staged);
  if (!staged.length) throw new Error('Nothing is staged.');
  if (!summary.trim()) throw new Error('A summary is required.');
  const byDoc = new Map<string, WorkingChange[]>();
  for (const w of staged) byDoc.set(w.docId, [...(byDoc.get(w.docId) ?? []), w]);
  const out: Commit[] = [];
  for (const [docId, list] of byDoc) {
    const current = blocksOf(db, docId, branch);
    // An edit's "before" is what the block holds now on this branch, so the diff stays true.
    const edits = list.map((w) => {
      const b = current.find((x) => x.id === w.edit.blockId);
      return w.edit.op === 'add' ? w.edit : { ...w.edit, before: b ? b.text : w.edit.before };
    });
    out.push(addCommit(db, { docId, branch, edits, summary: summary.trim(), description: description.trim(), authorId: db.me, local: true }));
  }
  discard(db, staged.map((w) => w.id));
  project(db, projectId).updatedAt = Date.now();
  log(db, { actorId: db.me, projectId, kind: 'commit', summary: `committed "${summary.trim()}" to ${branchName(db, branch)}` });
  return out;
}

export function branchName(db: Db, branch: string): string {
  return branch === 'main' ? 'main' : draft(db, branch)?.name ?? branch;
}

/** Sends this branch's local commits to the server. */
export function push(db: Db, projectId: string, branch: string): number {
  const ids = new Set(docsOf(db, projectId).map((d) => d.id));
  let n = 0;
  for (const c of db.commits) {
    if (ids.has(c.docId) && c.branch === branch && c.local) {
      c.local = false;
      n++;
    }
  }
  return n;
}

// ---- drafts --------------------------------------------------------------------------------

export function newDraft(db: Db, docId: string, name: string): Draft {
  const clean = name.trim().replace(/\s+/g, '-').toLowerCase();
  if (!/^[a-z0-9][a-z0-9._-]{0,62}$/.test(clean)) throw new Error('A draft name uses letters, digits, dots, hyphens and underscores.');
  const target = doc(db, docId);
  if (db.drafts.some((d) => d.docId === docId && d.name === clean && (d.state === 'private' || d.state === 'in_review'))) {
    throw new Error('This file already has an open draft with that name.');
  }
  const d: Draft = {
    id: uid('draft'),
    docId,
    projectId: target.projectId,
    name: clean,
    authorId: db.me,
    baseSeq: mainSeq(db, docId),
    createdAt: Date.now(),
    state: 'private',
    blocks: target.blocks.map((b) => ({ ...b })),
  };
  db.drafts.push(d);
  log(db, { actorId: db.me, projectId: target.projectId, docId, kind: 'draft', summary: `started the draft ${clean} of ${target.title}` });
  return d;
}

export function withdraw(db: Db, draftId: string): void {
  const d = draft(db, draftId);
  if (!d) throw new Error('No such draft.');
  if (d.authorId !== db.me) throw new Error('Only its author can withdraw a draft.');
  d.state = 'withdrawn';
  for (const r of db.requests) if (r.draftId === draftId && r.status === 'open') r.status = 'withdrawn';
  db.working = db.working.filter((w) => w.branch !== draftId);
  log(db, { actorId: db.me, projectId: d.projectId, docId: d.docId, kind: 'draft', summary: `withdrew the draft ${d.name} (kept in history)` });
}

/** What the draft changes against main at its base (for compare and for a request). */
export function draftEdits(db: Db, d: Draft): Edit[] {
  const target = doc(db, d.docId);
  const base = baseBlocks(db, d, target);
  return compareBlocks(base, d.blocks);
}

/** Main's blocks at the draft's base revision. */
export function baseBlocks(db: Db, d: Draft, target = doc(db, d.docId)): Block[] {
  return blocksAt(db, target.id, d.baseSeq);
}

/** The edits that undo `edits` (for revert and for rebuilding an older revision). */
export function invert(edits: Edit[]): Edit[] {
  return edits
    .map((e): Edit => {
      if (e.op === 'add') return { blockId: e.blockId, op: 'remove', before: e.after };
      if (e.op === 'remove') return { blockId: e.blockId, op: 'add', after: e.before, label: e.label ?? e.blockId, afterId: e.afterId };
      return { blockId: e.blockId, op: 'edit', before: e.after, after: e.before };
    })
    .reverse();
}

/** Conflicts between a draft and main: blocks both changed since the draft's base, differently. */
export function conflictsOf(db: Db, d: Draft): Conflict[] {
  const target = doc(db, d.docId);
  const base = baseBlocks(db, d, target);
  const mine = new Map(compareBlocks(base, d.blocks).map((e) => [e.blockId, e]));
  const out: Conflict[] = [];
  for (const e of compareBlocks(base, target.blocks)) {
    const m = mine.get(e.blockId);
    if (!m || e.op !== 'edit' || m.op !== 'edit' || m.after === e.after) continue;
    const label = base.find((b) => b.id === e.blockId)?.label ?? e.blockId;
    out.push({ blockId: e.blockId, label, ancestor: e.before ?? '', team: e.after ?? '', draft: m.after ?? '' });
  }
  return out;
}

/** Brings main's newer revisions into the draft. Blocks in conflict take the choice given
 *  ("team" or "draft"); with an unresolved conflict nothing is changed. */
export function rebase(db: Db, draftId: string, choices: Record<string, 'team' | 'draft'>): Commit | null {
  const d = draft(db, draftId);
  if (!d) throw new Error('No such draft.');
  const conflicts = conflictsOf(db, d);
  for (const c of conflicts) if (!choices[c.blockId]) throw new Error(`Choose a version for ${c.label}.`);
  const target = doc(db, d.docId);
  const base = baseBlocks(db, d, target);
  const mineEdits = compareBlocks(base, d.blocks);
  // Start from main, then re-apply the draft's own edits except where the team's version was chosen.
  let blocks = target.blocks.map((b) => ({ ...b }));
  const kept: Edit[] = [];
  for (const e of mineEdits) {
    if (choices[e.blockId] === 'team') continue;
    const cur = blocks.find((b) => b.id === e.blockId);
    kept.push(e.op === 'edit' ? { ...e, before: cur?.text ?? e.before } : e);
  }
  blocks = applyEdits(blocks, kept);
  const changes = compareBlocks(d.blocks, blocks);
  const from = d.baseSeq;
  d.baseSeq = mainSeq(db, d.docId);
  if (!changes.length) {
    log(db, { actorId: db.me, projectId: d.projectId, docId: d.docId, kind: 'draft', summary: `updated ${d.name} from main (r${from} to r${d.baseSeq})` });
    return null;
  }
  const commit = addCommit(db, {
    docId: d.docId,
    branch: d.id,
    edits: changes,
    summary: `Rebase onto main r${d.baseSeq}`,
    description: conflicts.length ? `Resolved ${conflicts.length} conflict${conflicts.length === 1 ? '' : 's'}.` : '',
    authorId: db.me,
    kind: 'rebase',
    local: false,
  });
  for (const r of db.requests) {
    if (r.draftId === d.id && r.status === 'open') {
      r.conflicts = conflictsOf(db, d);
      r.edits = draftEdits(db, d);
      r.baseSeq = d.baseSeq;
      for (const rv of r.reviews) rv.stale = true;
    }
  }
  return commit;
}

// ---- change requests ------------------------------------------------------------------------

export function nextRequestNumber(db: Db, projectId: string): number {
  return db.requests.filter((r) => r.projectId === projectId).reduce((n, r) => Math.max(n, r.number), 0) + 1;
}

export function sendForReview(db: Db, draftId: string, name: string, note: string): ChangeRequest {
  const d = draft(db, draftId);
  if (!d) throw new Error('No such draft.');
  if (d.state !== 'private') throw new Error('This draft is already in review or closed.');
  const edits = draftEdits(db, d);
  if (!edits.length) throw new Error('The draft has no changes against main.');
  const r: ChangeRequest = {
    id: uid('cr'),
    number: nextRequestNumber(db, d.projectId),
    projectId: d.projectId,
    docId: d.docId,
    draftId,
    name: name.trim() || d.name,
    note: note.trim(),
    authorId: db.me,
    status: 'open',
    createdAt: Date.now(),
    baseSeq: d.baseSeq,
    edits,
    excluded: [],
    conflicts: conflictsOf(db, d),
    reviews: [],
    comments: [],
  };
  db.requests.push(r);
  d.state = 'in_review';
  log(db, { actorId: db.me, projectId: d.projectId, docId: d.docId, kind: 'request', summary: `opened change request #${r.number} ${r.name}` });
  return r;
}

export interface ReviewState {
  approvals: number;
  required: number;
  changesRequested: number;
  satisfied: boolean;
  canMerge: boolean;
  canApprove: boolean;
  why: string;
}

const MERGERS: Record<'editors' | 'owners', Role[]> = { editors: ['owner', 'manager', 'editor'], owners: ['owner', 'manager'] };
const APPROVERS: Role[] = ['owner', 'manager', 'editor', 'approver'];

export function reviewState(db: Db, r: ChangeRequest): ReviewState {
  const p = project(db, r.projectId);
  const latest = new Map<string, Verdict>();
  for (const rv of r.reviews) if (!rv.stale && rv.verdict !== 'commented') latest.set(rv.reviewerId, rv.verdict);
  const approvals = [...latest.values()].filter((v) => v === 'approved').length;
  const changesRequested = [...latest.values()].filter((v) => v === 'changes_requested').length;
  const required = p.rules.requiredApprovals;
  const unresolved = r.conflicts.filter((c) => !c.choice).length;
  const satisfied = approvals >= required && changesRequested === 0;
  const mayMerge = MERGERS[p.rules.mergeRoles].includes(p.role);
  const canMerge = r.status === 'open' && satisfied && mayMerge && unresolved === 0;
  const canApprove = r.status === 'open' && APPROVERS.includes(p.role) && r.authorId !== db.me;
  const why =
    r.status !== 'open'
      ? `This request is ${r.status}.`
      : !mayMerge
        ? `Only ${p.rules.mergeRoles === 'owners' ? 'owners and managers' : 'editors, managers and owners'} merge in this project.`
        : changesRequested
          ? 'A reviewer asked for changes.'
          : approvals < required
            ? `${approvals} of ${required} required approval${required === 1 ? '' : 's'}.`
            : unresolved
              ? `Choose a version for ${unresolved} conflict${unresolved === 1 ? '' : 's'}.`
              : 'Ready to merge.';
  return { approvals, required, changesRequested, satisfied, canMerge, canApprove, why };
}

export function review(db: Db, requestId: string, verdict: Verdict, note: string): void {
  const r = db.requests.find((x) => x.id === requestId);
  if (!r) throw new Error('No such request.');
  if (verdict !== 'commented' && !reviewState(db, r).canApprove) throw new Error('You cannot review your own request, or your role does not review.');
  r.reviews.push({ reviewerId: db.me, verdict, note: note.trim(), at: Date.now(), stale: false });
  if (note.trim()) r.comments.push({ id: uid('cmt'), authorId: db.me, text: note.trim(), at: Date.now() });
  const word = verdict === 'approved' ? 'approved' : verdict === 'changes_requested' ? 'requested changes on' : 'commented on';
  log(db, { actorId: db.me, projectId: r.projectId, docId: r.docId, kind: 'review', summary: `${word} #${r.number} ${r.name}` });
}

export function comment(db: Db, requestId: string, text: string, blockId?: string, proposedByAgent = false): void {
  const r = db.requests.find((x) => x.id === requestId);
  if (!r) throw new Error('No such request.');
  if (!text.trim()) throw new Error('Write a comment first.');
  r.comments.push({ id: uid('cmt'), authorId: db.me, text: text.trim(), at: Date.now(), blockId, proposedByAgent });
}

/** Merges the included changes of an open request into main, as a person's action. */
export function merge(db: Db, requestId: string): Commit {
  const r = db.requests.find((x) => x.id === requestId);
  if (!r) throw new Error('No such request.');
  const st = reviewState(db, r);
  if (!st.canMerge) throw new Error(st.why);
  const target = doc(db, r.docId);
  const choice = new Map(r.conflicts.map((c) => [c.blockId, c.choice]));
  const edits: Edit[] = [];
  for (const e of r.edits) {
    if (r.excluded.includes(e.blockId)) continue;
    if (choice.get(e.blockId) === 'team') continue;
    const cur = target.blocks.find((b) => b.id === e.blockId);
    if (e.op === 'edit' && !cur) continue;
    edits.push(e.op === 'add' ? e : { ...e, before: cur?.text ?? e.before });
  }
  const commit = addCommit(db, {
    docId: r.docId,
    branch: 'main',
    edits,
    summary: `Merge #${r.number}: ${r.name}`,
    description: r.note,
    authorId: db.me,
    kind: 'merge',
    mergedFrom: r.draftId,
  });
  r.status = 'merged';
  r.mergedAt = Date.now();
  const d = draft(db, r.draftId);
  if (d) d.state = 'merged';
  log(db, { actorId: db.me, projectId: r.projectId, docId: r.docId, kind: 'merge', summary: `merged #${r.number} ${r.name} into main (r${commit.seq})` });
  return commit;
}

export function decline(db: Db, requestId: string, note: string): void {
  const r = db.requests.find((x) => x.id === requestId);
  if (!r) throw new Error('No such request.');
  if (r.status !== 'open') throw new Error(`This request is ${r.status}.`);
  r.status = 'declined';
  if (note.trim()) r.comments.push({ id: uid('cmt'), authorId: db.me, text: note.trim(), at: Date.now() });
  const d = draft(db, r.draftId);
  if (d) d.state = 'withdrawn';
  log(db, { actorId: db.me, projectId: r.projectId, docId: r.docId, kind: 'request', summary: `declined #${r.number} ${r.name}` });
}

// ---- recovery: revert, cherry-pick, restore (each makes a recovery draft, as Dynodoc does) ----

export function recoveryDraft(db: Db, commitId: string, mode: 'revert' | 'cherry_pick'): Draft {
  const c = db.commits.find((x) => x.id === commitId);
  if (!c) throw new Error('No such revision.');
  const target = doc(db, c.docId);
  const name = `${mode === 'revert' ? 'revert' : 'pick'}-${c.branch === 'main' ? 'r' : 'd'}${c.seq}`;
  const d = newDraft(db, target.id, name + (db.drafts.some((x) => x.name === name) ? '-' + Date.now().toString(36).slice(-3) : ''));
  const source = mode === 'revert' ? invert(c.edits) : c.edits;
  const edits: Edit[] = [];
  for (const e of source) {
    const cur = d.blocks.find((b) => b.id === e.blockId);
    if (e.op === 'edit' && cur) edits.push({ ...e, before: cur.text });
    else if (e.op === 'remove' && cur) edits.push({ ...e, before: cur.text });
    else if (e.op === 'add' && !cur) edits.push(e);
  }
  if (!edits.length) throw new Error('Those changes are already undone or no longer apply.');
  addCommit(db, {
    docId: target.id,
    branch: d.id,
    edits,
    summary: mode === 'revert' ? `Revert "${c.summary}"` : `Cherry-pick "${c.summary}"`,
    description: `${mode === 'revert' ? 'Reverts' : 'Applies'} ${c.branch === 'main' ? 'r' : 'd'}${c.seq} (${c.chainHash.slice(0, 8)}).`,
    authorId: db.me,
    kind: mode,
  });
  return d;
}

export function restoreVersion(db: Db, docId: string, versionId: string): Draft {
  const target = doc(db, docId);
  const v = target.versions.find((x) => x.id === versionId);
  if (!v) throw new Error('No such version.');
  const then = blocksAt(db, docId, v.throughSeq);
  const d = newDraft(db, docId, `restore-${v.name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 40) || 'version'}`);
  const edits = compareBlocks(d.blocks, then);
  if (!edits.length) throw new Error('main already matches this version.');
  addCommit(db, { docId, branch: d.id, edits, summary: `Restore "${v.name}" as new changes`, authorId: db.me, kind: 'restore' });
  return d;
}

export function saveVersion(db: Db, docId: string, name: string, note: string): void {
  const target = doc(db, docId);
  if (!name.trim()) throw new Error('Name the version.');
  target.versions.unshift({ id: uid('ver'), name: name.trim(), note: note.trim(), throughSeq: mainSeq(db, docId), at: Date.now(), authorId: db.me });
  log(db, { actorId: db.me, projectId: target.projectId, docId, kind: 'version', summary: `saved the version "${name.trim()}" of ${target.title}` });
}
