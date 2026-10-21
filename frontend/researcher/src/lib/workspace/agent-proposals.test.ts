import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  FILTERS,
  budgetText,
  changedWhileReading,
  clientName,
  currentRevision,
  emptyText,
  isRequestChanged,
  loadProposals,
  parsePage,
  parseProposal,
  proposalsMessage,
  proposalsPath,
  sizeText,
  staleText,
  stateHelp,
  stateLabel,
  type ProposalState
} from './agent-proposals';

// Synthetic values only.
const PROJECT = '22222222-2222-4222-8222-222222222222';
const row = (extra: Record<string, unknown> = {}) => ({
  id: '33333333-3333-4333-8333-333333333333',
  name: 'Proposal from Claude for Olivia',
  state: 'open',
  revision: 2,
  base_seq: 5,
  head_seq: 7,
  behind: 2,
  stale_base: true,
  created_at: '2026-10-21T09:00:00Z',
  submitted_at: '2026-10-21T09:05:00Z',
  closed_at: null,
  document: { id: '44444444-4444-4444-8444-444444444444', title: 'Plan', kind: 'document' },
  agent: {
    id: '55555555-5555-4555-8555-555555555555',
    client_kind: 'claude',
    display_name: 'Claude for Olivia',
    principal: { id: '66666666-6666-4666-8666-666666666666', name: 'Olivia' },
    revoked: false
  },
  mine: true,
  edits: 3,
  ops: { proposed: 2, compiled: 3 },
  ...extra
});

afterEach(() => vi.unstubAllGlobals());

describe('the words for a proposal', () => {
  it('names every state in plain language, with what happens next', () => {
    const states: ProposalState[] = [
      'draft',
      'open',
      'changes_requested',
      'merged',
      'declined',
      'withdrawn'
    ];
    const labels = states.map(stateLabel);
    expect(labels).toEqual([
      'Waiting for you to send',
      'Waiting for review',
      'Changes requested',
      'Merged',
      'Declined',
      'Withdrawn'
    ]);
    expect(new Set(labels).size).toBe(6);
    for (const s of states) expect(stateHelp(s).length).toBeGreaterThan(20);
    expect(stateHelp('draft')).toContain('Only you can see it');
    expect(stateHelp('open')).toContain('Nothing changes in the file until someone merges it');
  });
  it('offers every filter once and says what an empty one means', () => {
    expect(FILTERS.map((f) => f.id)).toEqual([
      'active',
      'draft',
      'open',
      'changes_requested',
      'merged',
      'declined',
      'withdrawn',
      'all'
    ]);
    for (const f of FILTERS) expect(emptyText(f.id)).toMatch(/\.$/);
    expect(emptyText('active')).toContain('appears here for a person to review');
  });
  it('names the client product and falls back to a neutral phrase', () => {
    expect(clientName('claude')).toBe('Claude');
    expect(clientName('chatgpt')).toBe('ChatGPT');
    expect(clientName('device')).toBe('an AI tool');
    expect(clientName('<script>')).toBe('an AI tool');
    expect(clientName(null)).toBe('an AI tool');
  });
  it('warns about a base that is behind, only while it matters', () => {
    expect(staleText({ staleBase: false, behind: 3 })).toBe('');
    expect(staleText({ staleBase: true, behind: 1 })).toContain('1 newer change since');
    expect(staleText({ staleBase: true, behind: 4 })).toContain('4 newer changes since');
    expect(sizeText({ edits: 1 })).toBe('1 change');
    expect(sizeText({ edits: 0 })).toBe('0 changes');
  });
  it('says what the agents used of the limits, or nothing when writing is off', () => {
    const b = {
      writeMode: 'draft',
      openedToday: { used: 2, limit: 20 },
      open: { used: 1, limit: 5 },
      opsPerProposal: 500
    };
    expect(budgetText(b)).toBe(
      'Your AI agents have started 2 of 20 proposals today and have 1 of 5 open.'
    );
    expect(budgetText({ ...b, writeMode: 'none' })).toBe('');
    expect(budgetText(null)).toBe('');
  });
});

describe('reading the list', () => {
  it('reads a row and keeps what the server says about the agent', () => {
    const p = parseProposal(row())!;
    expect(p.state).toBe('open');
    expect(p.agent.displayName).toBe('Claude for Olivia');
    expect(p.agent.principalName).toBe('Olivia');
    expect(p.staleBase).toBe(true);
    expect(p.behind).toBe(2);
    expect(p.opsProposed).toBe(2);
    expect(p.opsCompiled).toBe(3);
    expect(p.mine).toBe(true);
  });
  it('skips what it cannot read instead of guessing', () => {
    expect(parseProposal(null)).toBeNull();
    expect(parseProposal(row({ state: 'closed' }))).toBeNull();
    expect(parseProposal(row({ revision: -1 }))).toBeNull();
    expect(parseProposal(row({ agent: null }))).toBeNull();
    expect(parseProposal(row({ created_at: 5 }))).toBeNull();
    const page = parsePage({
      items: [row(), { nonsense: true }, row({ id: 'a' })],
      more: true,
      offset: 20,
      state: 'merged',
      budgets: null
    })!;
    expect(page.items.map((i) => i.id)).toEqual(['33333333-3333-4333-8333-333333333333', 'a']);
    expect(page.more).toBe(true);
    expect(page.filter).toBe('merged');
    expect(parsePage({ items: 'x' })).toBeNull();
  });
  it('keeps text from the agent as text', () => {
    const p = parseProposal(row({ name: '<img src=x onerror=alert(1)>' }))!;
    expect(p.name).toBe('<img src=x onerror=alert(1)>');
  });
  it('reads the budgets of the viewer’s own agents', () => {
    const page = parsePage({
      items: [],
      more: false,
      offset: 0,
      state: 'active',
      budgets: {
        write_mode: 'draft',
        proposals_today: { used: 1, limit: 20 },
        open_proposals: { used: 0, limit: 5 },
        ops_per_proposal: 500
      }
    })!;
    expect(page.budgets?.openedToday).toEqual({ used: 1, limit: 20 });
    expect(parsePage({ items: [], budgets: { write_mode: 'draft' } })!.budgets).toBeNull();
  });
  it('asks for one page at a time of a state', async () => {
    expect(proposalsPath(PROJECT, 'changes_requested', 20)).toBe(
      `/projects/${PROJECT}/agent-proposals?state=changes_requested&offset=20&limit=20`
    );
    const fetcher = vi.fn<(input: RequestInfo | URL) => Promise<Response>>(
      async () =>
        new Response(JSON.stringify({ items: [row()], more: false, offset: 0, state: 'active' }), {
          status: 200,
          headers: { 'content-type': 'application/json' }
        })
    );
    vi.stubGlobal('fetch', fetcher);
    const page = await loadProposals(PROJECT);
    expect(page.items).toHaveLength(1);
    expect(String(fetcher.mock.calls[0][0])).toContain('/agent-proposals?state=active&offset=0');
  });
});

describe('the refusals a reader can meet', () => {
  const refusal = (status: number, code?: string, details?: unknown) =>
    Object.assign(new Error('x'), { status, code, details });
  it('knows a request that moved while it was being read', () => {
    const moved = refusal(409, 'request_changed', { current_revision: 4 });
    expect(isRequestChanged(moved)).toBe(true);
    expect(currentRevision(moved)).toBe(4);
    expect(isRequestChanged(refusal(409, 'conflict'))).toBe(false);
    expect(isRequestChanged(refusal(400, 'request_changed'))).toBe(false);
    expect(isRequestChanged(new Error('request_changed'))).toBe(false);
    expect(isRequestChanged(null)).toBe(false);
    expect(currentRevision(refusal(409, 'request_changed', { current_revision: -1 }))).toBeNull();
    expect(currentRevision(new Error('x'))).toBeNull();
  });
  it('says it calmly and says that nothing was recorded', () => {
    expect(changedWhileReading('approve')).toBe(
      'This request changed while you were reading it. Nothing was recorded. Read the new version, then decide.'
    );
    expect(changedWhileReading('adopt')).toContain('Nothing was sent.');
    expect(changedWhileReading('review')).not.toContain('Nothing');
  });
  it('turns a failed list request into one sentence', () => {
    expect(proposalsMessage(refusal(401))).toContain('Sign in again');
    expect(proposalsMessage(refusal(403))).toContain('Only members');
    expect(proposalsMessage(refusal(429))).toContain('Wait a moment');
    expect(proposalsMessage(refusal(503))).toContain('busy');
    expect(proposalsMessage(new TypeError('fetch failed'))).toContain('connection');
    expect(proposalsMessage(refusal(418))).toContain('could not be loaded');
  });
});
