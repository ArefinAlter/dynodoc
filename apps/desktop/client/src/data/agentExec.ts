// The agent's tools, run in the page against the workspace the screens already show. The set is the
// closed table in lib/agentTools.ts (and src-tauri/src/agent/tools.rs, which refuses any other name):
// the tools read history, diffs, files and change requests, and two of them draft a card (a review
// comment, a commit summary) that a person presses. None of them changes anything: there is no tool
// that approves, merges, commits, pushes, deletes or changes a role or a rule.
//
// What a tool returns goes to the model as untrusted text, so it is plain facts and quotes of the
// documents, cut to a length, never an instruction.
import * as B from './backend.ts';
import { compareBlocks, type Edit } from '../lib/diff.ts';
import type { Db, Doc, ChangeRequest } from './types.ts';
import type { AgentCard } from '../state.ts';
import { relative } from '../lib/format.ts';
import { DEFAULT_PREFS } from '../state.ts';

export interface ToolResult {
  output: string;
  card?: AgentCard;
}

class ToolError extends Error {}

const clip = (s: string, n = 220): string => {
  const t = s.replace(/\s+/g, ' ').trim();
  return t.length > n ? `${t.slice(0, n - 1)}…` : t;
};

const str = (v: unknown): string => (typeof v === 'string' ? v : typeof v === 'number' ? String(v) : '');
const num = (v: unknown): number | undefined => (typeof v === 'number' && Number.isFinite(v) ? v : typeof v === 'string' && /^\d+$/.test(v) ? Number(v) : undefined);

function findDoc(db: Db, projectId: string, ref: unknown): Doc {
  const q = str(ref).trim().toLowerCase();
  if (!q) throw new ToolError('Name the file (its file name or title).');
  const docs = B.docsOf(db, projectId);
  const exact = docs.filter((d) => d.id === q || d.filename.toLowerCase() === q || d.title.toLowerCase() === q);
  const found = exact.length ? exact : docs.filter((d) => d.filename.toLowerCase().includes(q) || d.title.toLowerCase().includes(q));
  if (found.length === 1) return found[0];
  if (!found.length) throw new ToolError(`No file matches "${clip(q, 60)}". Files: ${docs.map((d) => d.filename).join(', ') || 'none'}.`);
  throw new ToolError(`"${clip(q, 60)}" matches several files: ${found.map((d) => d.filename).join(', ')}.`);
}

function findRequest(db: Db, projectId: string, number: unknown): ChangeRequest {
  const n = num(number);
  const r = db.requests.find((x) => x.projectId === projectId && x.number === n);
  if (!r) throw new ToolError(`No change request #${String(number)} in this project.`);
  return r;
}

function who(db: Db, id: string): string {
  return id === db.me ? 'you' : B.person(db, id).name;
}

function labels(d: Doc): Map<string, string> {
  return new Map(d.blocks.map((b) => [b.id, b.label]));
}

function editLine(e: Edit, names: Map<string, string>): string {
  const label = names.get(e.blockId) ?? e.label ?? e.blockId;
  if (e.op === 'edit') return `${label}: "${clip(e.before ?? '', 160)}" → "${clip(e.after ?? '', 160)}"`;
  if (e.op === 'add') return `${label}: added "${clip(e.after ?? '', 200)}"`;
  return `${label}: removed "${clip(e.before ?? '', 200)}"`;
}

function editLines(edits: Edit[], names: Map<string, string>, limit = 30): string {
  const lines = edits.slice(0, limit).map((e) => editLine(e, names));
  if (edits.length > limit) lines.push(`… and ${edits.length - limit} more changes`);
  return lines.join('\n') || 'No changes.';
}


export function runTool(db: Db, projectId: string, branch: string, name: string, args: Record<string, unknown>): ToolResult {
  const now = Date.now();
  try {
    switch (name) {
      case 'documents.list': {
        const p = B.project(db, projectId);
        const lines = B.docsOf(db, projectId).map((d) => {
          const drafts = db.drafts.filter((x) => x.docId === d.id && (x.state === 'private' || x.state === 'in_review')).length;
          const reqs = db.requests.filter((r) => r.docId === d.id && r.status === 'open').length;
          return `- ${d.filename} (${d.kind}), main r${B.mainSeq(db, d.id)}, ${drafts} open draft(s), ${reqs} open request(s)`;
        });
        return { output: `Files in the project "${p.name}":\n${lines.join('\n') || 'none'}` };
      }
      case 'documents.outline': {
        const d = findDoc(db, projectId, args.document);
        const blocks = d.blocks.slice(0, 80).map((b) => `${b.label}: ${clip(b.text, 90)}`);
        return { output: `${d.filename} at main r${B.mainSeq(db, d.id)}, ${d.blocks.length} blocks${d.blocks.length > 80 ? ' (first 80)' : ''}:\n${blocks.join('\n')}` };
      }
      case 'history.read': {
        const d = findDoc(db, projectId, args.document);
        const which = str(args.branch) || 'main';
        const draft = which === 'main' ? undefined : db.drafts.find((x) => x.docId === d.id && (x.id === which || x.name === which));
        if (which !== 'main' && !draft) throw new ToolError(`No draft "${clip(which, 60)}" of ${d.filename}.`);
        const limit = Math.min(Math.max(num(args.limit) ?? 10, 1), 25);
        const list = B.branchCommits(db, d.id, draft ? draft.id : 'main').slice(-limit).reverse();
        const lines = list.map((c) => `${c.branch === 'main' ? 'r' : 'd'}${c.seq} [${c.kind}] ${clip(c.summary, 90)}, by ${who(db, c.authorId)}, ${relative(c.at, now, DEFAULT_PREFS)}, ${c.edits.length} change(s), chain ${c.chainHash.slice(0, 12)}`);
        return { output: `${d.filename}, ${draft ? `draft "${draft.name}"` : 'main'}, newest first:\n${lines.join('\n') || 'no revisions'}` };
      }
      case 'history.verify': {
        const d = findDoc(db, projectId, args.document);
        const remote = db.remote?.verify[d.id];
        const res = B.verify(db, d.id);
        const bad = res.filter((r) => !r.ok);
        const server = remote ? (remote.ok ? 'The engine\'s own check of the chain says it is intact.' : `The engine's own check says the chain is broken${remote.brokenAt ? ` at revision ${remote.brokenAt}` : ''}.`) : '';
        return { output: `${d.filename}: ${res.length - bad.length} of ${res.length} revisions recompute from the edits shown here${bad.length ? `; ${bad.length} do not` : ''}. ${server}`.trim() };
      }
      case 'diff.get': {
        const d = findDoc(db, projectId, args.document);
        const draftRef = str(args.draft);
        if (draftRef) {
          const dr = db.drafts.find((x) => x.docId === d.id && (x.id === draftRef || x.name === draftRef));
          if (!dr) throw new ToolError(`No draft "${clip(draftRef, 60)}" of ${d.filename}.`);
          const edits = compareBlocks(B.baseBlocks(db, dr), dr.blocks);
          return { output: `${dr.name} against main r${dr.baseSeq} of ${d.filename}:\n${editLines(edits, labels(d))}` };
        }
        const commits = B.branchCommits(db, d.id, 'main');
        const to = num(args.to) ?? B.mainSeq(db, d.id);
        const from = num(args.from) ?? (commits.filter((c) => c.seq <= to).slice(-2)[0]?.seq ?? 0);
        const within = commits.filter((c) => c.seq > from && c.seq <= to);
        return { output: `${d.filename}, changes after r${from} through r${to} (${within.length} revision(s)):\n${editLines(within.flatMap((c) => c.edits), labels(d))}` };
      }
      case 'requests.list': {
        const want = str(args.state) || 'open';
        const list = db.requests.filter((r) => r.projectId === projectId && (want === 'all' || r.status === want));
        const lines = list.map((r) => {
          const st = B.reviewState(db, r);
          return `#${r.number} ${clip(r.name, 80)} (${B.doc(db, r.docId).filename}) by ${who(db, r.authorId)}, ${r.status}, ${r.edits.length} change(s), ${st.approvals}/${st.required} approvals${r.conflicts.length ? `, ${r.conflicts.length} conflict(s)` : ''}`;
        });
        return { output: `Change requests (${want}):\n${lines.join('\n') || 'none'}` };
      }
      case 'requests.summarise': {
        const r = findRequest(db, projectId, args.number);
        const st = B.reviewState(db, r);
        const reviews = r.reviews.map((v) => `${who(db, v.reviewerId)}: ${v.verdict}${v.note ? ` ("${clip(v.note, 120)}")` : ''}${v.stale ? ' (stale)' : ''}`);
        const comments = r.comments.slice(-8).map((c) => `${who(db, c.authorId)}${c.blockId ? ` on ${c.blockId}` : ''}: ${clip(c.text, 160)}`);
        return {
          output: [
            `#${r.number} ${r.name} (${r.status}) on ${B.doc(db, r.docId).filename}, by ${who(db, r.authorId)}, based on r${r.baseSeq}.`,
            r.note ? `Description: ${clip(r.note, 400)}` : '',
            `Review state: ${st.approvals} of ${st.required} approvals. ${st.why}`,
            reviews.length ? `Reviews:\n${reviews.join('\n')}` : 'No reviews yet.',
            comments.length ? `Comments:\n${comments.join('\n')}` : '',
            r.conflicts.length ? `Conflicts with main: ${r.conflicts.map((c) => c.label).join(', ')}` : 'No conflicts with main.',
            `Changes:\n${editLines(r.edits, labels(B.doc(db, r.docId)))}`,
          ].filter(Boolean).join('\n'),
        };
      }
      case 'review.propose_comment': {
        const r = findRequest(db, projectId, args.number);
        if (r.status !== 'open') throw new ToolError(`#${r.number} is ${r.status}; comments are only proposed on open requests.`);
        const text = clip(str(args.text), 900);
        if (!text) throw new ToolError('Give the comment text.');
        const ref = str(args.block).toLowerCase();
        const edit = ref ? r.edits.find((e) => e.blockId.toLowerCase() === ref || (e.label ?? '').toLowerCase().includes(ref)) : undefined;
        if (ref && !edit) throw new ToolError(`#${r.number} has no changed block "${clip(ref, 60)}". Its changed blocks: ${r.edits.map((e) => e.label ?? e.blockId).slice(0, 12).join(', ')}.`);
        return {
          output: 'Proposed to the person as a card. It is not posted; they press the button if they want it posted.',
          card: { kind: 'comment', title: `Proposed comment on #${r.number}`, requestId: r.id, blockId: edit?.blockId, text },
        };
      }
      case 'commit.draft_summary': {
        const summary = clip(str(args.summary), 60);
        if (!summary) throw new ToolError('Give a summary of up to 60 characters.');
        const staged = B.working(db, projectId, branch).filter((w) => w.staged);
        return {
          output: `Proposed to the person as a card (${staged.length} staged change(s) on this branch). Committing stays theirs.`,
          card: { kind: 'summary', title: 'Proposed commit summary', text: summary, description: str(args.description).slice(0, 1500) },
        };
      }
      default:
        return { output: `error: there is no tool called ${clip(name, 60)}.` };
    }
  } catch (e) {
    return { output: `error: ${e instanceof ToolError ? e.message : 'the tool failed.'}` };
  }
}
