import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_FILTERS,
  FALLBACK_COLOR,
  ISSUE_CAP,
  IssueError,
  LABEL_PALETTE,
  addComment,
  appendIssues,
  canManageLabels,
  canOpenIssues,
  contrastRatio,
  createIssue,
  deleteLabel,
  hasFilters,
  isUnavailable,
  issueMessage,
  issueQuery,
  labelTextColor,
  listIssues,
  luminance,
  normalizeColor,
  parseIssue,
  parseIssuePage,
  parseLabelList,
  parseStar,
  parseSummary,
  parseThread,
  roleRank,
  setStar,
  updateIssue,
  type IssueSummary
} from './issues';

// Synthetic values only. Shapes mirror docs/PROJECT-ISSUES-API.md.
const MAYA = { id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', name: 'Maya Chen' };
const SAM = { id: 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', name: 'Sam Okoye' };
const LABEL = {
  id: 'cccccccc-cccc-4ccc-8ccc-cccccccccccc',
  name: 'Needs evidence',
  color: 'd93f0b',
  description: 'Waiting on a source document'
};
const row = (number: number, extra: Record<string, unknown> = {}) => ({
  id: `00000000-0000-4000-8000-${String(number).padStart(12, '0')}`,
  number,
  title: `Issue ${number}`,
  state: 'open',
  author: MAYA,
  assignee: { ...SAM, member: true },
  labels: [LABEL],
  comment_count: 2,
  created_at: '2026-10-12T09:00:00Z',
  updated_at: '2026-10-13T09:00:00Z',
  closed_at: null,
  ...extra
});
const detail = (extra: Record<string, unknown> = {}) => ({
  ...row(7),
  body: 'Please attach the signed Q3 budget.xlsx.',
  revision: 3,
  closed_by: null,
  viewer: { role: 'editor', can_comment: true, can_edit: false },
  ...extra
});
const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

afterEach(() => vi.unstubAllGlobals());

describe('parsers', () => {
  it('reads an issue summary and keeps a stale assignee flagged', () => {
    const parsed = parseSummary(row(12, { assignee: { ...SAM, member: false } }));
    expect(parsed).toMatchObject({
      number: 12,
      title: 'Issue 12',
      state: 'open',
      commentCount: 2,
      closedAt: null,
      author: MAYA,
      assignee: { id: SAM.id, member: false }
    });
    expect(parsed?.labels).toEqual([LABEL]);
  });

  it('skips malformed rows instead of guessing', () => {
    expect(parseSummary({ ...row(1), state: 'merged' })).toBeNull();
    expect(parseSummary({ ...row(1), number: 0 })).toBeNull();
    expect(parseSummary({ ...row(1), title: null })).toBeNull();
    const page = parseIssuePage({
      items: [row(3), { nope: true }, row(2), { ...row(1), state: 'weird' }],
      next_before: 2,
      counts: { open: 5, closed: 9 }
    });
    expect(page.items.map((i) => i.number)).toEqual([3, 2]);
    expect(page.skipped).toBe(2);
    expect(page.nextBefore).toBe(2);
    expect(page.counts).toEqual({ open: 5, closed: 9 });
  });

  it('uses a calm fallback for a missing author name and a bad label colour', () => {
    const parsed = parseSummary(
      row(4, { author: { id: MAYA.id, name: null }, labels: [{ ...LABEL, color: 'zzz' }] })
    );
    expect(parsed?.author.name).toBe('Project member');
    expect(parsed?.labels[0].color).toBe(FALLBACK_COLOR);
  });

  it('reads one issue with what the caller may do', () => {
    const issue = parseIssue(
      detail({ viewer: { role: 'owner', can_comment: true, can_edit: true } })
    );
    expect(issue).toMatchObject({
      number: 7,
      body: 'Please attach the signed Q3 budget.xlsx.',
      revision: 3,
      closedBy: null,
      viewer: { role: 'owner', canComment: true, canEdit: true }
    });
    // Without a viewer block nothing is assumed to be allowed.
    expect(parseIssue(row(2))?.viewer).toEqual({ role: '', canComment: false, canEdit: false });
  });

  it('reads comments oldest first with the page marker', () => {
    const thread = parseThread({
      ...detail(),
      comments: {
        items: [
          { id: 'c1', seq: 1, author: SAM, body: 'On it.', created_at: '2026-10-12T10:00:00Z' },
          { id: 'c2', seq: 2, author: MAYA, body: 'Thanks', created_at: '2026-10-12T11:00:00Z' },
          { id: 'bad', seq: 0, author: MAYA, body: 'x' }
        ],
        next_after: 2
      }
    });
    expect(thread?.comments.map((c) => c.seq)).toEqual([1, 2]);
    expect(thread?.nextAfter).toBe(2);
    expect(parseThread({ nope: true })).toBeNull();
  });

  it('reads labels with counts, the manage flag and a star answer', () => {
    const list = parseLabelList({
      items: [{ ...LABEL, open_issue_count: 4 }, { id: 'x' }],
      can_manage: true
    });
    expect(list.items).toEqual([{ ...LABEL, openIssueCount: 4 }]);
    expect(list.canManage).toBe(true);
    expect(parseLabelList({ items: [] }).canManage).toBe(false);
    expect(parseStar({ starred: true, star_count: 3 })).toEqual({ starred: true, starCount: 3 });
    expect(parseStar({ starred: 'yes', star_count: 3 })).toBeNull();
  });
});

describe('label colour', () => {
  it('normalises typed colours and rejects anything else', () => {
    expect(normalizeColor('#0969DA')).toBe('0969da');
    expect(normalizeColor(' 0969da ')).toBe('0969da');
    expect(normalizeColor('0969d')).toBeNull();
    expect(normalizeColor('#12345g')).toBeNull();
    expect(normalizeColor(undefined)).toBeNull();
  });

  it('picks black or white text, never less than 4.5:1 on any palette colour', () => {
    expect(labelTextColor('ffffff')).toBe('#000000');
    expect(labelTextColor('000000')).toBe('#ffffff');
    expect(labelTextColor('b60205')).toBe('#ffffff');
    expect(labelTextColor('fbca04')).toBe('#000000');
    for (const { color } of LABEL_PALETTE)
      expect(contrastRatio(color, labelTextColor(color).slice(1))).toBeGreaterThanOrEqual(4.5);
    // The worst case over a coarse sweep of the whole colour cube stays readable too.
    let worst = Infinity;
    for (let r = 0; r < 256; r += 51)
      for (let g = 0; g < 256; g += 51)
        for (let b = 0; b < 256; b += 51) {
          const hex = [r, g, b].map((c) => c.toString(16).padStart(2, '0')).join('');
          worst = Math.min(worst, contrastRatio(hex, labelTextColor(hex).slice(1)));
        }
    expect(worst).toBeGreaterThanOrEqual(4.5);
  });

  it('computes WCAG luminance at the extremes', () => {
    expect(luminance('ffffff')).toBeCloseTo(1, 5);
    expect(luminance('000000')).toBeCloseTo(0, 5);
    expect(contrastRatio('ffffff', '000000')).toBeCloseTo(21, 1);
  });
});

describe('roles and filters', () => {
  it('ranks roles as the contract does', () => {
    expect(roleRank('owner')).toBe(5);
    expect(roleRank('approver')).toBe(2);
    expect(roleRank('reviewer')).toBe(2);
    expect(roleRank('viewer')).toBe(1);
    expect(roleRank('stranger')).toBe(0);
    expect(canOpenIssues('viewer')).toBe(false);
    expect(canOpenIssues('reviewer')).toBe(true);
    expect(canManageLabels('editor')).toBe(false);
    expect(canManageLabels('manager')).toBe(true);
  });

  it('builds the list query and leaves empty filters out', () => {
    expect(issueQuery(DEFAULT_FILTERS)).toBe('state=open&limit=30');
    const query = new URLSearchParams(
      issueQuery(
        { state: 'closed', q: '  budget  ', label: LABEL.id, assignee: 'me', author: SAM.id },
        { before: 41, limit: 500 }
      )
    );
    expect(Object.fromEntries(query)).toEqual({
      state: 'closed',
      q: 'budget',
      label: LABEL.id,
      assignee: 'me',
      author: SAM.id,
      before: '41',
      limit: '100'
    });
    expect(
      new URLSearchParams(issueQuery({ ...DEFAULT_FILTERS, assignee: 'none' })).get('assignee')
    ).toBe('none');
    expect(
      new URLSearchParams(issueQuery({ ...DEFAULT_FILTERS, q: 'x'.repeat(300) })).get('q')
    ).toHaveLength(100);
    expect(issueQuery(DEFAULT_FILTERS, { before: 0, limit: 0 })).toBe('state=open&limit=1');
  });

  it('tells filtering from the Open/Closed switch', () => {
    expect(hasFilters(DEFAULT_FILTERS)).toBe(false);
    expect(hasFilters({ ...DEFAULT_FILTERS, state: 'closed' })).toBe(false);
    expect(hasFilters({ ...DEFAULT_FILTERS, q: ' ' })).toBe(false);
    expect(hasFilters({ ...DEFAULT_FILTERS, label: LABEL.id })).toBe(true);
  });

  it('appends pages without repeats and stops at the cap', () => {
    const make = (n: number) => parseSummary(row(n)) as IssueSummary;
    const first = [make(5), make(4)];
    expect(appendIssues(first, [make(4), make(3)]).map((i) => i.number)).toEqual([5, 4, 3]);
    const many = Array.from({ length: ISSUE_CAP }, (_, i) => make(1000 - i));
    expect(appendIssues(many, [make(1)])).toHaveLength(ISSUE_CAP);
  });
});

describe('issueMessage', () => {
  const err = (status: number, reason = '') => new IssueError(status, reason);
  it('never shows raw server text', () => {
    const secret = 'select * from project_issue where password = 1';
    for (const status of [400, 401, 403, 404, 409, 422, 429, 500, 504])
      for (const subject of [
        'list',
        'issue',
        'create',
        'edit',
        'comment',
        'label',
        'star'
      ] as const)
        expect(issueMessage(err(status, secret), subject)).not.toContain('select');
  });

  it('is specific for the cases people can act on', () => {
    expect(issueMessage(err(401), 'list')).toMatch(/session has ended/);
    expect(issueMessage(err(403, 'viewer'), 'create')).toMatch(/Viewers can read/);
    expect(issueMessage(err(403), 'edit')).toMatch(/author, a manager or an owner/);
    expect(issueMessage(err(403), 'label')).toMatch(/managers and owners/);
    expect(issueMessage(err(404), 'issue')).toMatch(/does not exist/);
    expect(issueMessage(err(404), 'label')).toMatch(/no longer exists/);
    expect(issueMessage(err(409, 'changed since you opened it'), 'edit')).toMatch(
      /changed since you opened it/
    );
    expect(issueMessage(err(409, 'reached the 5,000 issue limit'), 'create')).toMatch(/5,000/);
    expect(issueMessage(err(409, 'reached the 1,000 comment limit'), 'comment')).toMatch(/1,000/);
    expect(issueMessage(err(409, 'reached the 100 label limit'), 'label')).toMatch(/100 labels/);
    expect(issueMessage(err(409, 'A label with this name already exists'), 'label')).toMatch(
      /already exists/
    );
    expect(issueMessage(err(400, 'Choose labels that exist in this project'), 'create')).toMatch(
      /label no longer exists/
    );
    expect(issueMessage(err(400, 'Assign issues to current project members'), 'edit')).toMatch(
      /not a member/
    );
    expect(issueMessage(err(400, 'Title must be 1-200 characters'), 'create')).toMatch(/title/);
    expect(issueMessage(err(422), 'create')).toMatch(/Reload the page/);
    expect(issueMessage(err(429), 'comment')).toMatch(/Too many requests/);
    expect(issueMessage(err(504), 'list')).toMatch(/busy or timed out/);
    expect(issueMessage(err(405), 'star')).toMatch(/not available on this server/);
    expect(issueMessage(new TypeError('Failed to fetch'), 'list')).toMatch(/Could not reach/);
  });

  it('treats a missing route as an older server, not an error', () => {
    expect(isUnavailable(err(404))).toBe(true);
    expect(isUnavailable(err(501))).toBe(true);
    expect(isUnavailable(err(403))).toBe(false);
    expect(isUnavailable(new Error('x'))).toBe(false);
  });
});

describe('requests', () => {
  it('lists issues with the query and parses the page', async () => {
    const fetchMock = vi.fn(async () =>
      json({ items: [row(2)], next_before: null, counts: { open: 1, closed: 0 } })
    );
    vi.stubGlobal('fetch', fetchMock);
    const page = await listIssues('proj 1', { ...DEFAULT_FILTERS, q: 'budget' });
    expect(page.items).toHaveLength(1);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/engine/projects/proj%201/issues?state=open&q=budget&limit=30');
    expect(init.method).toBe('GET');
  });

  it('creates an issue with only the fields that were filled in', async () => {
    const fetchMock = vi.fn(async () => json(detail()));
    vi.stubGlobal('fetch', fetchMock);
    await createIssue('p', { title: '  Attach budget ', body: '', labels: [], assigneeId: null });
    const [, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(init.method).toBe('POST');
    expect(JSON.parse(String(init.body))).toEqual({ title: 'Attach budget' });
    await createIssue('p', { title: 'T', body: 'B', labels: [LABEL.id], assigneeId: SAM.id });
    const [, second] = fetchMock.mock.calls[1] as unknown as [string, RequestInit];
    expect(JSON.parse(String(second.body))).toEqual({
      title: 'T',
      body: 'B',
      labels: [LABEL.id],
      assignee_id: SAM.id
    });
  });

  it('patches only what changed, sends null to unassign and an empty set to clear labels', async () => {
    const fetchMock = vi.fn(async () => json(detail()));
    vi.stubGlobal('fetch', fetchMock);
    await updateIssue('p', 7, { assigneeId: null, labels: [], state: 'closed' });
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/engine/projects/p/issues/7');
    expect(init.method).toBe('PATCH');
    expect(JSON.parse(String(init.body))).toEqual({
      assignee_id: null,
      labels: [],
      state: 'closed'
    });
  });

  it('adds a comment and returns the server count', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        json({
          comment: {
            id: 'c9',
            seq: 9,
            author: MAYA,
            body: 'Done',
            created_at: '2026-10-13T08:00:00Z'
          },
          comment_count: 9
        })
      )
    );
    const result = await addComment('p', 7, ' Done ');
    expect(result.comment.seq).toBe(9);
    expect(result.commentCount).toBe(9);
  });

  it('stars with PUT and unstars with DELETE', async () => {
    const fetchMock = vi.fn(async () => json({ starred: true, star_count: 4 }));
    vi.stubGlobal('fetch', fetchMock);
    expect(await setStar('p', true)).toEqual({ starred: true, starCount: 4 });
    await setStar('p', false);
    expect((fetchMock.mock.calls[0] as unknown as [string, RequestInit])[1].method).toBe('PUT');
    expect((fetchMock.mock.calls[1] as unknown as [string, RequestInit])[1].method).toBe('DELETE');
  });

  it('reports label deletion counts', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => json({ ok: true, issues_updated: 6 }))
    );
    expect(await deleteLabel('p', LABEL.id)).toEqual({ issuesUpdated: 6 });
  });

  it('turns a refusal into an IssueError carrying only status and reason', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        json({ error: 'forbidden', details: { reason: 'Viewers cannot comment' } }, 403)
      )
    );
    await expect(addComment('p', 1, 'x')).rejects.toMatchObject({
      status: 403,
      reason: 'Viewers cannot comment'
    });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('Method Not Allowed', { status: 405 }))
    );
    await expect(setStar('p', true)).rejects.toMatchObject({ status: 405 });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => json({ unexpected: true }))
    );
    await expect(setStar('p', true)).rejects.toMatchObject({ status: 0 });
  });
});
