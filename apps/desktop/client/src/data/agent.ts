// The scripted agent. No model is called: a request is matched to one of a few prepared plans,
// and each plan calls real tools of the closed table against the scripted backend, through the
// same access check a real agent would meet (lib/agentTools.ts mayCall). Refused calls are
// shown. The replies are prepared text filled with what the tools returned.
import { mayCall, type AgentAccess } from '../lib/agentTools.ts';
import { stats } from '../lib/diff.ts';
import * as B from './backend.ts';
import type { Db } from './types.ts';
import type { AgentCard, ToolCall } from '../state.ts';

export interface Plan {
  steps: { tool: string; input: string; run: () => string }[];
  /** Built after the steps ran; receives whether any step was refused. */
  finish: (refused: string[]) => { text: string; card?: AgentCard };
}

export const STARTERS = [
  'Summarise the open change requests',
  'What changed in the budget this month?',
  'Draft a commit summary for my staged changes',
  'Propose a review comment on #14',
  'Check the hash chain of the handbook',
  'Merge #14 for me',
];

function name(db: Db, id: string): string {
  return B.person(db, id).name;
}

export function plan(db: Db, projectId: string, branch: string, text: string): Plan {
  const t = text.toLowerCase();
  const open = () => db.requests.filter((r) => r.projectId === projectId && r.status === 'open');

  if (/\bmerge|approve|decline\b/.test(t) && !/summar|comment/.test(t)) {
    const num = /#?(\d+)/.exec(t)?.[1];
    const r = db.requests.find((x) => x.projectId === projectId && String(x.number) === num) ?? open()[0];
    return {
      steps: r ? [{ tool: 'requests.summarise', input: `#${r.number}`, run: () => { const st = B.reviewState(db, r); return `${r.edits.length} changes · ${st.approvals}/${st.required} approvals · ${st.why}`; } }, { tool: 'requests.merge', input: `#${r.number}`, run: () => '' }] : [],
      finish: () => ({
        text: r
          ? `I cannot merge #${r.number}: there is no merge tool, at any access level. Merging is a person's button in Change requests. Where it stands: ${B.reviewState(db, r).why}`
          : 'I cannot merge, approve or decline anything: those are a person’s buttons. There is no such tool at any access level.',
        card: r ? { kind: 'requests', title: 'Open it to decide', rows: [[`#${r.number}`, r.name, B.reviewState(db, r).why]], columns: ['Request', 'Title', 'State'], requestId: r.id } : undefined,
      }),
    };
  }

  if (/commit|summary for|staged/.test(t)) {
    const staged = B.working(db, projectId, branch).filter((w) => w.staged);
    const files = [...new Set(staged.map((w) => B.doc(db, w.docId).filename))];
    const summary = staged.length
      ? files.length === 1
        ? `Update ${files[0].replace(/\.[a-z]+$/, '')}: ${staged.length} change${staged.length === 1 ? '' : 's'}`
        : `Update ${files.map((f) => f.replace(/\.[a-z]+$/, '').replace(/-\d{4}$/, '')).join(' and ')}`
      : '';
    const description = staged.map((w) => `${B.doc(db, w.docId).filename}: ${w.edit.op === 'edit' ? 'edited' : w.edit.op === 'add' ? 'added' : 'removed'} ${w.edit.label ?? w.edit.blockId}`).join('\n');
    return {
      steps: [{ tool: 'commit.draft_summary', input: `${staged.length} staged change(s) on ${B.branchName(db, branch)}`, run: () => (staged.length ? `summary: ${summary}` : 'nothing staged') }],
      finish: (refused) =>
        refused.length
          ? { text: `I could not draft it: ${refused[0]}. Switch this thread to Propose access if you want me to.` }
          : staged.length
            ? { text: 'Here is a summary drafted from your staged changes. It goes into the commit box only if you use it; committing stays yours.', card: { kind: 'summary', title: 'Proposed commit summary', text: summary.slice(0, 60), description } }
            : { text: 'Nothing is staged on this branch. Stage the changes you want in Changes and ask again.' },
    };
  }

  if (/summar|open (change )?requests|what.*waiting|review state/.test(t)) {
    const list = open();
    return {
      steps: [
        { tool: 'requests.list', input: 'state=open', run: () => `${list.length} open: ${list.map((r) => `#${r.number}`).join(', ')}` },
        ...list.map((r) => ({ tool: 'requests.summarise', input: `#${r.number}`, run: () => { const st = B.reviewState(db, r); return `${B.doc(db, r.docId).filename}: ${r.edits.length} change(s), ${st.approvals}/${st.required} approvals, ${r.conflicts.length} conflict(s)`; } })),
      ],
      finish: () => ({
        text: list.length
          ? `There ${list.length === 1 ? 'is' : 'are'} ${list.length} open change request${list.length === 1 ? '' : 's'} in this project.\n\n${list
              .map((r) => {
                const st = B.reviewState(db, r);
                return `- **#${r.number} ${r.name}** (${B.doc(db, r.docId).filename}, by ${name(db, r.authorId)}${r.byAgent ? ', an agent key' : ''}): ${r.edits.length} change${r.edits.length === 1 ? '' : 's'}. ${st.why}${r.conflicts.length ? ` It conflicts with main on ${r.conflicts.map((c) => c.label).join(' and ')}.` : ''}`;
              })
              .join('\n')}\n\nI can read and summarise these; approving and merging are yours.`
          : 'No change requests are open in this project.',
        card: list.length ? { kind: 'requests', title: 'Open change requests', columns: ['Request', 'Title', 'State'], rows: list.map((r) => [`#${r.number}`, r.name, B.reviewState(db, r).why]) } : undefined,
      }),
    };
  }

  if (/budget|spreadsheet|cell|changed/.test(t)) {
    const d = db.docs.find((x) => x.projectId === projectId && x.kind === 'spreadsheet');
    if (d) {
      const month = Date.now() - 31 * 86_400_000;
      const commits = B.branchCommits(db, d.id, 'main').filter((c) => c.at >= month);
      return {
        steps: [
          { tool: 'history.read', input: `${d.filename} since 31 days`, run: () => `${commits.length} revisions: ${commits.map((c) => `r${c.seq}`).join(', ')}` },
          { tool: 'diff.get', input: `${d.filename} r${(commits[0]?.seq ?? 1) - 1}..r${B.mainSeq(db, d.id)}`, run: () => { const s = stats(commits.flatMap((c) => c.edits)); return `${s.edited} cell edits`; } },
        ],
        finish: () => ({
          text: `In the last month ${d.filename} changed in ${commits.length} revision${commits.length === 1 ? '' : 's'} on main:\n\n${commits.map((c) => `- r${c.seq} **${c.summary}** (${name(db, c.authorId)}): ${c.edits.map((e) => `${e.blockId} ${e.before?.split('  ')[0]} → ${e.after?.split('  ')[0]}`).join(', ')}`).join('\n')}\n\nEach edit to a line came with an edit to its total, so the totals still add up.`,
          card: { kind: 'table', title: `${d.filename}, last 31 days`, columns: ['Rev', 'Summary', 'Author', 'Cells'], rows: commits.map((c) => [`r${c.seq}`, c.summary, name(db, c.authorId), c.edits.map((e) => e.blockId).join(' ')]) },
        }),
      };
    }
  }

  if (/comment|review/.test(t)) {
    const num = /#?(\d+)/.exec(t)?.[1];
    const r = db.requests.find((x) => x.projectId === projectId && String(x.number) === num && x.status === 'open') ?? open()[0];
    if (r) {
      const e = r.edits[0];
      const proposal =
        r.number === 14
          ? 'The new wording reads well. One question: "up to three blocks" — is there a minimum length for a block? A line on how far ahead to tell your manager would answer Lena’s question too.'
          : r.conflicts.length
            ? `This edits ${r.conflicts.map((c) => c.label).join(' and ')}, which main changed after the draft started. Could you rebase and check the totals before we merge?`
            : `Small and clear. Could the message say why ${e?.blockId ?? 'this'} changes, for the history?`;
      return {
        steps: [
          { tool: 'requests.summarise', input: `#${r.number}`, run: () => `${r.edits.length} change(s); reviews: ${r.reviews.length}` },
          { tool: 'diff.get', input: `#${r.number}`, run: () => r.edits.map((x) => `${x.blockId}: ${x.op}`).join(', ') },
          { tool: 'review.propose_comment', input: `#${r.number} on ${e?.blockId ?? 'the request'}`, run: () => 'proposed (not posted)' },
        ],
        finish: (refused) =>
          refused.length
            ? { text: `I read #${r.number}, but I could not propose a comment: ${refused[refused.length - 1]}.` }
            : { text: `I read #${r.number} and drafted one comment. It is posted only if you press the button, and it is posted as you, marked as drafted by the agent.`, card: { kind: 'comment', title: `Proposed comment on #${r.number}`, requestId: r.id, blockId: e?.blockId, text: proposal } },
      };
    }
  }

  if (/hash|chain|verif|tamper|integrity/.test(t)) {
    const d = db.docs.find((x) => x.projectId === projectId && (/budget|sheet/.test(t) ? x.kind === 'spreadsheet' : /deck|slide/.test(t) ? x.kind === 'presentation' : x.kind === 'document')) ?? db.docs.find((x) => x.projectId === projectId);
    if (d) {
      let res: ReturnType<typeof B.verify> = [];
      return {
        steps: [
          { tool: 'history.read', input: d.filename, run: () => `${db.commits.filter((c) => c.docId === d.id).length} revisions across main and drafts` },
          { tool: 'history.verify', input: d.filename, run: () => { res = B.verify(db, d.id); return `${res.filter((r) => r.ok).length}/${res.length} verify`; } },
        ],
        finish: () => {
          const bad = res.filter((r) => !r.ok);
          return {
            text: bad.length
              ? `The chain of ${d.filename} does not verify at ${bad.length} revision${bad.length === 1 ? '' : 's'}.`
              : `I recomputed all ${res.length} chain hashes of ${d.filename} (main and its drafts) from the edits: each one follows from the previous. The latest on main is r${B.mainSeq(db, d.id)}.`,
            card: { kind: 'verify', title: `${d.filename}: ${bad.length ? 'broken' : 'intact'}`, columns: ['Revision', 'Branch', 'Chain hash', 'Result'], rows: db.commits.filter((c) => c.docId === d.id).sort((a, b) => b.at - a.at).slice(0, 8).map((c) => [`${c.branch === 'main' ? 'r' : 'd'}${c.seq}`, B.branchName(db, c.branch), c.chainHash.slice(0, 12), res.find((x) => x.commitId === c.id)?.ok ? 'verified' : 'broken']) },
          };
        },
      };
    }
  }

  return {
    steps: [{ tool: 'documents.list', input: B.project(db, projectId).name, run: () => B.docsOf(db, projectId).map((d) => d.filename).join(', ') }],
    finish: () => ({
      text: `I read the files of ${B.project(db, projectId).name}. My answers in this release are prepared, so I understand a few requests well: summarising open change requests, what changed in a file, drafting a commit summary or a review comment, and checking a hash chain. Try one of those.`,
    }),
  };
}

/** Runs a plan's steps through the access check. */
export function execute(p: Plan, access: AgentAccess, enabled: readonly string[]): { calls: ToolCall[]; refused: string[] } {
  const calls: ToolCall[] = [];
  const refused: string[] = [];
  for (const s of p.steps) {
    const t0 = performance.now();
    const ok = mayCall(s.tool, access, enabled);
    if (!ok.ok) {
      calls.push({ tool: s.tool, input: s.input, output: '', refused: ok.reason, ms: 0 });
      refused.push(`${s.tool} was refused because ${ok.reason}`);
      continue;
    }
    let output: string;
    try {
      output = s.run();
    } catch (e) {
      output = `error: ${e instanceof Error ? e.message : String(e)}`;
    }
    calls.push({ tool: s.tool, input: s.input, output, ms: Math.max(1, Math.round(performance.now() - t0 + 40 + Math.random() * 160)) });
  }
  return { calls, refused };
}
