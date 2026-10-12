import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  BRANCH_PAGE,
  FILE_LIST_CAP,
  HistoryError,
  MESSAGE_CAP,
  REVIEW_PAGE,
  approvalsRequired,
  groupReviews,
  historyMessage,
  isAbort,
  loadBranches,
  loadCommit,
  loadReviews,
  parseCommit,
  parseRefs,
  parseReviews,
  type Review
} from './project-history';

// Synthetic values only. Shapes mirror crates/engine-api/src/project_history.rs; the
// generated OpenAPI types describe these bodies as `unknown`.
const AUTHORITY = '11111111-1111-4111-8111-111111111111';
const PROJECT = '22222222-2222-4222-8222-222222222222';
const hex = (digit: string) => digit.repeat(64);
const signed = (
  branch: string,
  commit: string,
  generation: number,
  previous: string | null = null,
  issuedAtMs = 1_760_270_400_000
) => ({
  receipt: {
    version: 1,
    scope: { authority: AUTHORITY, project: PROJECT, branch },
    head: { commit, generation },
    previous,
    issued_at_ms: issuedAtMs,
    issuer_key: hex('a')
  },
  signature: hex('b') + hex('c')
});
const head = (branch: string, commit: string, generation: number) => ({
  branch,
  head: { commit, generation }
});

afterEach(() => vi.unstubAllGlobals());

describe('parseRefs', () => {
  it('pairs each head with the receipt issued for exactly that head', () => {
    const page = parseRefs({
      items: [head('feature/budget', hex('2'), 3), head('main', hex('1'), 7)],
      next: 'main',
      receipts: [
        signed('main', hex('1'), 7, hex('9')),
        signed('feature/budget', hex('2'), 3, hex('8'))
      ]
    });
    expect(page.branches.map((b) => b.name)).toEqual(['feature/budget', 'main']);
    const main = page.branches[1];
    expect(main).toMatchObject({
      commit: hex('1'),
      generation: 7,
      isDefault: true,
      isProtected: true
    });
    expect(main.receipt).toMatchObject({
      branch: 'main',
      generation: 7,
      previous: hex('9'),
      authority: AUTHORITY,
      project: PROJECT,
      issuerKey: hex('a'),
      issuedAt: '2025-10-12T12:00:00.000Z'
    });
    // Only the literal main branch is the protected team branch.
    expect(page.branches[0]).toMatchObject({ isDefault: false, isProtected: false });
    expect(page.skipped).toBe(0);
  });

  it('never attaches a receipt that describes a different head', () => {
    const page = parseRefs({
      items: [head('main', hex('1'), 7)],
      next: 'main',
      receipts: [signed('main', hex('1'), 6, hex('9'))]
    });
    expect(page.branches[0].receipt).toBeNull();
  });

  it('skips malformed entries instead of guessing and counts them', () => {
    const page = parseRefs({
      items: [
        head('main', hex('1'), 1),
        { branch: 'broken', head: { commit: 'not-an-address', generation: 2 } },
        { branch: '', head: { commit: hex('2'), generation: 1 } },
        { branch: 'zero', head: { commit: hex('2'), generation: 0 } },
        'text',
        null
      ],
      next: null,
      receipts: [{ receipt: { scope: {} } }, 'junk']
    });
    expect(page.branches.map((b) => b.name)).toEqual(['main']);
    expect(page.skipped).toBe(5);
    expect(page.branches[0].receipt).toBeNull();
  });

  it('reports an empty project and tolerates unexpected bodies', () => {
    expect(parseRefs({ items: [], next: null, receipts: [] })).toEqual({
      branches: [],
      next: null,
      more: false,
      skipped: 0
    });
    expect(parseRefs(null).branches).toEqual([]);
    expect(parseRefs([1, 2]).branches).toEqual([]);
  });

  it('offers another page only when a full page came back with a cursor', () => {
    const items = (n: number) =>
      Array.from({ length: n }, (_, i) => head(`b${String(i).padStart(3, '0')}`, hex('3'), 1));
    expect(parseRefs({ items: items(2), next: 'b001', receipts: [] }, 2).more).toBe(true);
    expect(parseRefs({ items: items(1), next: 'b000', receipts: [] }, 2).more).toBe(false);
    expect(parseRefs({ items: items(2), next: null, receipts: [] }, 2).more).toBe(false);
  });
});

describe('parseCommit', () => {
  const envelope = (commit: Record<string, unknown> = {}) => ({
    commit: {
      version: 1,
      project: PROJECT,
      parents: [hex('4')],
      files: {
        'Board update.docx': { document: AUTHORITY, state: hex('5') },
        'Q3 budget.xlsx': { document: PROJECT, state: hex('6') }
      },
      author_key: hex('d'),
      authored_at_ms: 1_760_270_000_000,
      message: 'Update the Q3 budget',
      ...commit
    },
    signature: hex('e') + hex('f')
  });

  it('summarises the signed envelope without inventing an author name', () => {
    const summary = parseCommit(envelope());
    expect(summary).toMatchObject({
      message: 'Update the Q3 budget',
      messageCut: false,
      authorKey: hex('d'),
      parents: 1,
      recovery: false,
      fileCount: 2,
      files: ['Board update.docx', 'Q3 budget.xlsx']
    });
    expect(summary).not.toHaveProperty('authorName');
  });

  it('caps long messages and long file lists', () => {
    const files = Object.fromEntries(
      Array.from({ length: FILE_LIST_CAP + 5 }, (_, i) => [
        `file-${String(i).padStart(3, '0')}.docx`,
        { document: AUTHORITY, state: hex('5') }
      ])
    );
    const summary = parseCommit(
      envelope({ message: 'x'.repeat(MESSAGE_CAP + 10), files, recovery: { commit: hex('7') } })
    );
    expect(summary?.message).toHaveLength(MESSAGE_CAP);
    expect(summary?.messageCut).toBe(true);
    expect(summary?.fileCount).toBe(FILE_LIST_CAP + 5);
    expect(summary?.files).toHaveLength(FILE_LIST_CAP);
    expect(summary?.recovery).toBe(true);
  });

  it('rejects bodies that are not a commit envelope', () => {
    expect(parseCommit(null)).toBeNull();
    expect(parseCommit({ commit: { files: {} } })).toBeNull();
    expect(parseCommit(envelope({ author_key: 'abc' }))).toBeNull();
    expect(parseCommit(envelope({ parents: 'none' }))).toBeNull();
  });
});

describe('reviews', () => {
  const row = (
    id: number,
    reviewer: string,
    decision: string,
    base: { generation: number; commit: string | null } = { generation: 6, commit: hex('9') },
    branch = 'main'
  ) => ({
    id,
    branch,
    expected_generation: base.generation,
    expected_commit: base.commit,
    reviewer,
    decision,
    created_at: '2026-10-12T10:00:00.000000+00:00'
  });

  it('parses decisions and drops anything it cannot name', () => {
    const page = parseReviews({
      items: [row(1, 'r1', 'approve'), row(2, 'r2', 'comment'), { id: 'x' }],
      next: 2
    });
    expect(page.items.map((r) => [r.id, r.decision])).toEqual([[1, 'approve']]);
    expect(page.skipped).toBe(2);
    expect(page.more).toBe(false);
    const full = parseReviews(
      { items: [row(1, 'r1', 'approve'), row(2, 'r2', 'approve')], next: 2 },
      2
    );
    expect(full.more).toBe(true);
    expect(full.next).toBe(2);
  });

  it('groups by exact base, keeps only the newest decision per reviewer and ignores other branches', () => {
    const parsed = parseReviews({
      items: [
        row(1, 'r1', 'request_changes'),
        row(2, 'r2', 'approve'),
        row(3, 'r1', 'approve'),
        row(4, 'r3', 'approve', { generation: 5, commit: hex('8') }),
        row(5, 'r4', 'approve', { generation: 0, commit: null }, 'feature/budget'),
        row(6, 'r2', 'request_changes')
      ],
      next: 6
    }).items;
    const groups = groupReviews(parsed, 'main');
    expect(groups.map((g) => [g.generation, g.commit])).toEqual([
      [6, hex('9')],
      [5, hex('8')]
    ]);
    const [latest] = groups;
    expect(latest.approvals).toBe(1);
    expect(latest.changeRequests).toBe(1);
    expect(latest.rows.map((r) => [r.id, r.current])).toEqual([
      [1, false],
      [2, false],
      [3, true],
      [6, true]
    ]);
    expect(groupReviews(parsed, 'feature/budget')).toHaveLength(1);
    expect(groupReviews([] as Review[], 'main')).toEqual([]);
  });

  it('only main asks for approvals', () => {
    expect(approvalsRequired('main', 2)).toBe(2);
    expect(approvalsRequired('main', 0)).toBe(0);
    expect(approvalsRequired('feature/budget', 2)).toBeNull();
  });
});

describe('historyMessage', () => {
  const failure = (status: number, reason = '') => new HistoryError(status, reason);
  it('turns statuses into short specific sentences', () => {
    expect(historyMessage(failure(403), 'branches')).toMatch(/do not have access/);
    expect(historyMessage(failure(403), 'commit')).toMatch(/file you can no longer access/);
    expect(historyMessage(failure(404), 'branches')).toMatch(/not available/);
    expect(historyMessage(failure(404), 'commit')).toMatch(/no longer published/);
    expect(
      historyMessage(failure(409, 'A branch head is unavailable after erasure'), 'branches')
    ).toMatch(/permanently erased/);
    expect(historyMessage(failure(409, 'Branch changed; fetch and compare'), 'branches')).toMatch(
      /changed while loading/
    );
    expect(historyMessage(failure(503), 'reviews')).toMatch(/busy or timed out/);
    expect(historyMessage(failure(401), 'branches')).toMatch(/Sign in/);
    expect(historyMessage(failure(418), 'branches')).toBe(
      'Shared history could not be loaded (418).'
    );
    expect(historyMessage(new TypeError('Failed to fetch'), 'branches')).toMatch(
      /reach the server/
    );
  });
  it('never echoes raw server text', () => {
    const text = historyMessage(failure(500, 'internal error: relation "x" missing'), 'branches');
    expect(text).not.toMatch(/relation/);
  });
  it('recognises an aborted request', () => {
    expect(isAbort(new DOMException('aborted', 'AbortError'))).toBe(true);
    expect(isAbort(new Error('nope'))).toBe(false);
    expect(isAbort(null)).toBe(false);
  });
});

describe('requests', () => {
  const reply = (body: unknown, status = 200) =>
    vi.fn<(input: string, init?: RequestInit) => Promise<Response>>(
      async () => new Response(JSON.stringify(body), { status })
    );

  it('reads one bounded page of heads, keyed by branch name, with the caller abort signal', async () => {
    const fetchMock = reply({ items: [head('main', hex('1'), 1)], next: 'main', receipts: [] });
    vi.stubGlobal('fetch', fetchMock);
    const controller = new AbortController();
    const page = await loadBranches(PROJECT, {
      after: 'feature/budget',
      signal: controller.signal
    });
    expect(page.branches).toHaveLength(1);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe(
      `/api/engine/projects/${PROJECT}/history/refs?limit=${BRANCH_PAGE}&after=feature%2Fbudget`
    );
    expect(init.signal).toBe(controller.signal);
    expect(init.method).toBeUndefined();
  });

  it('keeps page sizes inside the server limit of 256', async () => {
    const fetchMock = reply({ items: [], next: null, receipts: [] });
    vi.stubGlobal('fetch', fetchMock);
    await loadBranches(PROJECT, { limit: 5000 });
    expect(String(fetchMock.mock.calls[0][0])).toContain('limit=256');
  });

  it('maps a refused read to a typed error carrying the server reason', async () => {
    vi.stubGlobal(
      'fetch',
      reply(
        {
          error: 'conflict: A branch head is unavailable after erasure',
          code: 'conflict',
          details: { reason: 'A branch head is unavailable after erasure' }
        },
        409
      )
    );
    const error = await loadBranches(PROJECT).catch((e) => e);
    expect(error).toBeInstanceOf(HistoryError);
    expect(error).toMatchObject({
      status: 409,
      reason: 'A branch head is unavailable after erasure'
    });
    expect(historyMessage(error, 'branches')).toMatch(/permanently erased/);
  });

  it('rejects an empty body as an unexpected response', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('', { status: 200 }))
    );
    const error = await loadBranches(PROJECT).catch((e) => e);
    expect(error).toMatchObject({ status: 0 });
    expect(historyMessage(error, 'branches')).toMatch(/does not understand/);
  });

  it('refuses a malformed commit address before any request is made', async () => {
    const fetchMock = reply({});
    vi.stubGlobal('fetch', fetchMock);
    await expect(loadCommit(PROJECT, '../etc/passwd')).rejects.toBeInstanceOf(HistoryError);
    await expect(loadReviews(PROJECT, 'main')).rejects.toBeInstanceOf(HistoryError);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('asks for the reviews of exactly one candidate, resuming after a review id', async () => {
    const fetchMock = reply({ items: [], next: null });
    vi.stubGlobal('fetch', fetchMock);
    await loadReviews(PROJECT, hex('1'), { after: 12 });
    expect(String(fetchMock.mock.calls[0][0])).toBe(
      `/api/engine/projects/${PROJECT}/history/reviews?candidate=${hex('1')}&limit=${REVIEW_PAGE}&after=12`
    );
  });
});
