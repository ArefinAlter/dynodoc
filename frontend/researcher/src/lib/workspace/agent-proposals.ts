/**
 * What AI agents have proposed in a project, for the people who review it
 * (docs/AI-AGENT-CONNECTORS.md section 6.2): the list behind the "AI proposals" tab, the plain
 * words for each state and client, and the two refusals a reviewer can meet while reading.
 *
 * Everything an agent wrote (a proposal's name, the reasons it gave, the display name built
 * from it) is untrusted text: it is shown as text, never as markup, and never acted on.
 */
import { workspaceRequest } from './api';

export type ProposalState =
  | 'draft'
  | 'open'
  | 'changes_requested'
  | 'merged'
  | 'declined'
  | 'withdrawn';
/** The server's filter: one state, the three that need someone (`active`), or everything. */
export type ProposalFilter = ProposalState | 'active' | 'all';

export interface ProposalAgent {
  id: string;
  clientKind: string;
  displayName: string;
  principalId: string;
  principalName: string | null;
  revoked: boolean;
}
export interface AgentProposal {
  id: string;
  name: string;
  state: ProposalState;
  /** The revision of the proposal (what a review or an adoption must name). */
  revision: number;
  baseSeq: number;
  headSeq: number;
  /** Newer changes to the file since the proposal started. */
  behind: number;
  /** The file has moved on since the proposal started and the proposal is still live. */
  staleBase: boolean;
  createdAt: string;
  submittedAt: string | null;
  closedAt: string | null;
  document: { id: string; title: string; kind: string };
  agent: ProposalAgent;
  /** The viewer is the person this agent acts for. */
  mine: boolean;
  /** Changes stored in the proposal. */
  edits: number;
  /** Operations the agent sent, and those they were compiled into. */
  opsProposed: number;
  opsCompiled: number;
}
export interface ProposalBudgets {
  writeMode: string;
  openedToday: { used: number; limit: number };
  open: { used: number; limit: number };
  opsPerProposal: number;
}
export interface ProposalPage {
  items: AgentProposal[];
  more: boolean;
  offset: number;
  filter: ProposalFilter;
  /** The viewer's own agents' use of the project's limits; null when they have none. */
  budgets: ProposalBudgets | null;
}

/** The filters a person can choose, in the order the page offers them. */
export const FILTERS: readonly { id: ProposalFilter; label: string }[] = [
  { id: 'active', label: 'In progress' },
  { id: 'draft', label: 'Waiting for you to send' },
  { id: 'open', label: 'Waiting for review' },
  { id: 'changes_requested', label: 'Changes requested' },
  { id: 'merged', label: 'Merged' },
  { id: 'declined', label: 'Declined' },
  { id: 'withdrawn', label: 'Withdrawn' },
  { id: 'all', label: 'Everything' }
];
const FILTER_IDS: readonly string[] = FILTERS.map((f) => f.id);
const STATES: readonly string[] = [
  'draft',
  'open',
  'changes_requested',
  'merged',
  'declined',
  'withdrawn'
];
/** Rows a page asks for. The server allows 50. */
export const PAGE_SIZE = 20;

/** A state in plain words, as the person it concerns would say it. */
export function stateLabel(state: ProposalState): string {
  switch (state) {
    case 'draft':
      return 'Waiting for you to send';
    case 'open':
      return 'Waiting for review';
    case 'changes_requested':
      return 'Changes requested';
    case 'merged':
      return 'Merged';
    case 'declined':
      return 'Declined';
    default:
      return 'Withdrawn';
  }
}
/** One sentence on what a state means and what happens next. */
export function stateHelp(state: ProposalState): string {
  switch (state) {
    case 'draft':
      return 'The AI agent prepared this change. Only you can see it. Nothing is shared until you send it for review.';
    case 'open':
      return 'Reviewers can read it and decide. Nothing changes in the file until someone merges it.';
    case 'changes_requested':
      return 'A reviewer asked for changes. The AI agent can add to its proposal, or you can withdraw it.';
    case 'merged':
      return 'A person merged it into the file.';
    case 'declined':
      return 'A reviewer declined it. The file was not changed.';
    default:
      return 'It was withdrawn. The file was not changed.';
  }
}
/** Shown when a filter has nothing in it. */
export function emptyText(filter: ProposalFilter): string {
  switch (filter) {
    case 'active':
      return 'No AI agent has a proposal waiting. When an agent suggests a change, it appears here for a person to review.';
    case 'draft':
      return 'Nothing is waiting for you to send. A draft an AI agent prepares for you appears here first.';
    case 'open':
      return 'No proposal from an AI agent is waiting for review.';
    case 'changes_requested':
      return 'No proposal from an AI agent has changes requested.';
    case 'merged':
      return 'No proposal from an AI agent has been merged yet.';
    case 'declined':
      return 'No proposal from an AI agent has been declined.';
    case 'withdrawn':
      return 'No proposal from an AI agent has been withdrawn.';
    default:
      return 'No AI agent has proposed anything in this project yet.';
  }
}

const CLIENTS: Record<string, string> = {
  claude: 'Claude',
  chatgpt: 'ChatGPT',
  gemini: 'Gemini',
  copilot: 'Copilot',
  cursor: 'Cursor',
  codex: 'Codex'
};
/** The client product an agent ran in, as a person would name it. */
export const clientName = (kind: string | null | undefined): string =>
  (kind && CLIENTS[kind]) || 'an AI tool';

/** The warning for a proposal written against an older version of the file, or ''. */
export function staleText(p: Pick<AgentProposal, 'staleBase' | 'behind'>): string {
  if (!p.staleBase) return '';
  return `The file has ${p.behind === 1 ? '1 newer change' : `${p.behind} newer changes`} since the AI agent read it. Changes that do not overlap are combined; check the comparison before you decide.`;
}
export function sizeText(p: Pick<AgentProposal, 'edits'>): string {
  return p.edits === 1 ? '1 change' : `${p.edits} changes`;
}
/** The viewer's own agents' use of the project's limits, in one sentence, or ''. */
export function budgetText(b: ProposalBudgets | null): string {
  if (!b || b.writeMode === 'none') return '';
  return `Your AI agents have started ${b.openedToday.used} of ${b.openedToday.limit} proposals today and have ${b.open.used} of ${b.open.limit} open.`;
}
export const proposalTime = (p: Pick<AgentProposal, 'submittedAt' | 'createdAt'>) =>
  p.submittedAt || p.createdAt;

// --- reading the server's answer ------------------------------------------------------

const record = (v: unknown): Record<string, unknown> | null =>
  v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
const str = (v: unknown) => (typeof v === 'string' ? v : null);
const whole = (v: unknown) =>
  typeof v === 'number' && Number.isSafeInteger(v) && v >= 0 ? v : null;

function used(v: unknown): { used: number; limit: number } | null {
  const r = record(v),
    a = whole(r?.used),
    b = whole(r?.limit);
  return a === null || b === null ? null : { used: a, limit: b };
}
export function parseProposal(raw: unknown): AgentProposal | null {
  const r = record(raw),
    agent = record(r?.agent),
    principal = record(agent?.principal),
    doc = record(r?.document),
    ops = record(r?.ops);
  const id = str(r?.id),
    state = str(r?.state),
    created = str(r?.created_at);
  if (!r || !id || !state || !STATES.includes(state) || !created || !agent || !doc) return null;
  const agentId = str(agent.id),
    docId = str(doc.id),
    revision = whole(r.revision),
    base = whole(r.base_seq),
    head = whole(r.head_seq);
  if (!agentId || !docId || revision === null || base === null || head === null) return null;
  return {
    id,
    name: str(r.name) || 'Untitled proposal',
    state: state as ProposalState,
    revision,
    baseSeq: base,
    headSeq: head,
    behind: whole(r.behind) ?? Math.max(0, head - base),
    staleBase: r.stale_base === true,
    createdAt: created,
    submittedAt: str(r.submitted_at),
    closedAt: str(r.closed_at),
    document: { id: docId, title: str(doc.title) || 'Untitled file', kind: str(doc.kind) || '' },
    agent: {
      id: agentId,
      clientKind: str(agent.client_kind) || 'generic',
      displayName: str(agent.display_name) || 'An AI agent',
      principalId: str(principal?.id) || '',
      principalName: str(principal?.name),
      revoked: agent.revoked === true
    },
    mine: r.mine === true,
    edits: whole(r.edits) ?? 0,
    opsProposed: whole(ops?.proposed) ?? 0,
    opsCompiled: whole(ops?.compiled) ?? 0
  };
}
export function parseBudgets(raw: unknown): ProposalBudgets | null {
  const r = record(raw);
  const today = used(r?.proposals_today),
    open = used(r?.open_proposals),
    ops = whole(r?.ops_per_proposal);
  if (!r || !today || !open || ops === null) return null;
  return { writeMode: str(r.write_mode) || 'none', openedToday: today, open, opsPerProposal: ops };
}
export function parsePage(raw: unknown): ProposalPage | null {
  const r = record(raw);
  if (!r || !Array.isArray(r.items)) return null;
  const filter = str(r.state);
  return {
    items: r.items.map(parseProposal).filter((p): p is AgentProposal => p !== null),
    more: r.more === true,
    offset: whole(r.offset) ?? 0,
    filter: filter && FILTER_IDS.includes(filter) ? (filter as ProposalFilter) : 'active',
    budgets: parseBudgets(r.budgets)
  };
}

export function proposalsPath(projectId: string, filter: ProposalFilter, offset: number) {
  return `/projects/${encodeURIComponent(projectId)}/agent-proposals?state=${filter}&offset=${offset}&limit=${PAGE_SIZE}`;
}
/** One page of the project's AI agent proposals. */
export async function loadProposals(
  projectId: string,
  filter: ProposalFilter = 'active',
  offset = 0
): Promise<ProposalPage> {
  const page = parsePage(await workspaceRequest<unknown>(proposalsPath(projectId, filter, offset)));
  if (!page) throw new Error('The server sent a list this page does not understand.');
  return page;
}

// --- the refusals a reader can meet ---------------------------------------------------

/** The server refused because the request moved after the revision the person was reading. */
export const isRequestChanged = (error: unknown): boolean =>
  error instanceof Error &&
  (error as { status?: number }).status === 409 &&
  (error as { code?: string }).code === 'request_changed';
/** The revision the request is at now, when the refusal says. */
export function currentRevision(error: unknown): number | null {
  const details = record((error as { details?: unknown } | null)?.details);
  return whole(details?.current_revision);
}
/** Calm words for "it moved while you were reading", for what the person was doing. */
export function changedWhileReading(doing: 'approve' | 'adopt' | 'review'): string {
  const lead =
    doing === 'adopt'
      ? 'The AI agent added to this draft while you were reading it. Nothing was sent.'
      : doing === 'approve'
        ? 'This request changed while you were reading it. Nothing was recorded.'
        : 'This request changed while you were reading it.';
  return `${lead} Read the new version, then decide.`;
}
/** A short sentence for a failed list request; never a raw status. */
export function proposalsMessage(error: unknown): string {
  const status = (error as { status?: number } | null)?.status;
  if (status === 401) return 'Your session has ended. Sign in again to see AI proposals.';
  if (status === 403) return 'Only members of this project can see its AI proposals.';
  if (status === 429) return 'Too many requests. Wait a moment, then try again.';
  if (typeof status === 'number' && status >= 500)
    return 'The server is busy. Try again in a moment.';
  if (error instanceof TypeError)
    return 'Could not reach the server. Check your connection and try again.';
  return 'AI proposals could not be loaded. Try again in a moment.';
}
