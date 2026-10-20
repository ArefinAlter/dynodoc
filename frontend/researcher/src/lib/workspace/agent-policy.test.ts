import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  AgentPolicyError,
  CALLS_PER_MINUTE,
  MB_RANGE,
  agentMessage,
  bytesToMb,
  dailyTotals,
  draftOf,
  draftProblems,
  fileLabel,
  floorHelp,
  formatBytes,
  formatDay,
  hasProblems,
  isConflict,
  isUnavailable,
  killAgents,
  killMessage,
  killRequest,
  loadPolicy,
  loadUsage,
  mbToBytes,
  overrideLabel,
  parseKill,
  parsePolicy,
  parseUsage,
  policyRequest,
  roleFloorLabel,
  sameDraft,
  saveRequest,
  savePolicy,
  savedMessage,
  usageRequest,
  type PolicyDraft
} from './agent-policy';

// Synthetic values only. Shapes mirror the generated PolicyView, KillView and UsageView.
const PROJECT = '22222222-2222-4222-8222-222222222222';
const FILE = '99999999-aaaa-4bbb-8ccc-dddddddddddd';
const view = (extra: Record<string, unknown> = {}) => ({
  project_id: PROJECT,
  enabled: true,
  min_role: 'editor',
  kinds: ['document', 'presentation'],
  calls_per_minute: 30,
  read_bytes_per_day: 5_000_000,
  revision: 4,
  configured: true,
  gateway: true,
  killed_at: null,
  updated_at: '2026-10-20T08:00:00Z',
  file_overrides: [],
  file_overrides_truncated: false,
  ...extra
});
const draft = (extra: Partial<PolicyDraft> = {}): PolicyDraft => ({
  enabled: true,
  minRole: 'viewer',
  kinds: ['document', 'spreadsheet', 'presentation'],
  calls: 60,
  megabytes: 20,
  ...extra
});

describe('parsePolicy', () => {
  it('reads every field of the policy view', () => {
    const policy = parsePolicy(
      view({
        killed_at: '2026-10-19T10:00:00Z',
        file_overrides: [{ document_id: FILE, access: 'none', updated_at: '2026-10-18T10:00:00Z' }],
        file_overrides_truncated: true,
        changed: false
      })
    );
    expect(policy).toEqual({
      projectId: PROJECT,
      enabled: true,
      minRole: 'editor',
      kinds: ['document', 'presentation'],
      callsPerMinute: 30,
      readBytesPerDay: 5_000_000,
      revision: 4,
      configured: true,
      gateway: true,
      killedAt: '2026-10-19T10:00:00Z',
      fileOverrides: [{ documentId: FILE, access: 'none', updatedAt: '2026-10-18T10:00:00Z' }],
      overridesTruncated: true,
      changed: false
    });
  });
  it('keeps the kinds in display order and says honestly when the gateway is off', () => {
    const policy = parsePolicy(
      view({ kinds: ['presentation', 'document'], gateway: false, configured: false, revision: 0 })
    );
    expect(policy?.kinds).toEqual(['document', 'presentation']);
    expect(policy?.gateway).toBe(false);
    expect(policy?.configured).toBe(false);
    expect(policy?.changed).toBeNull();
  });
  it('skips an override it cannot read but refuses a policy it cannot read in full', () => {
    const policy = parsePolicy(
      view({
        file_overrides: [
          { document_id: FILE, access: 'propose', updated_at: 'x' },
          { access: 'none', updated_at: 'x' },
          { document_id: FILE, access: 'read', updated_at: '2026-10-18T10:00:00Z' }
        ]
      })
    );
    expect(policy?.fileOverrides).toEqual([
      { documentId: FILE, access: 'read', updatedAt: '2026-10-18T10:00:00Z' }
    ]);
    // Saving over a role or kind this page does not know could widen who may read.
    expect(parsePolicy(view({ min_role: 'auditor' }))).toBeNull();
    expect(parsePolicy(view({ kinds: ['document', 'drawing'] }))).toBeNull();
    expect(parsePolicy(view({ kinds: [] }))).toBeNull();
    expect(parsePolicy(view({ enabled: 'yes' }))).toBeNull();
    expect(parsePolicy(view({ gateway: undefined }))).toBeNull();
    expect(parsePolicy(view({ revision: -1 }))).toBeNull();
    expect(parsePolicy(null)).toBeNull();
    expect(parsePolicy([])).toBeNull();
  });
});

describe('parseKill and parseUsage', () => {
  it('reads the kill result', () => {
    expect(
      parseKill({
        project_id: PROJECT,
        enabled: false,
        was_enabled: true,
        revoked_grants: 2,
        revision: 5
      })
    ).toEqual({ projectId: PROJECT, wasEnabled: true, revokedGrants: 2, revision: 5 });
    expect(parseKill({ project_id: PROJECT })).toBeNull();
  });
  it('reads usage rows and skips a broken one', () => {
    const usage = parseUsage({
      from: '2026-10-14',
      to: '2026-10-20',
      truncated: true,
      items: [
        {
          day: '2026-10-20',
          client: 'claude-code',
          calls: 3,
          bytes_out: 1200,
          denials: 1,
          errors: 0
        },
        { day: '2026-10-20', calls: 'many', bytes_out: 0 },
        { day: '2026-10-19', client: null, calls: 1, bytes_out: 10 }
      ],
      project_id: PROJECT
    });
    expect(usage?.truncated).toBe(true);
    expect(usage?.items).toEqual([
      { day: '2026-10-20', client: 'claude-code', calls: 3, bytesOut: 1200, refused: 1, errors: 0 },
      { day: '2026-10-19', client: null, calls: 1, bytesOut: 10, refused: 0, errors: 0 }
    ]);
    expect(parseUsage({ items: [] })).toBeNull();
  });
});

describe('budgets and the form', () => {
  it('shows bytes as megabytes and gives back the same bytes', () => {
    expect(bytesToMb(20_000_000)).toBe(20);
    expect(mbToBytes(20)).toBe(20_000_000);
    expect(mbToBytes(bytesToMb(12_345_678))).toBe(12_345_678);
    expect(MB_RANGE).toEqual({ min: 0.01, max: 1000 });
    expect(mbToBytes(0.01)).toBe(10_000);
    expect(mbToBytes(1000)).toBe(1_000_000_000);
  });
  it('checks the server’s bounds before anything is sent', () => {
    expect(hasProblems(draftProblems(draft()))).toBe(false);
    expect(draftProblems(draft({ calls: 1 })).calls).toBeNull();
    expect(draftProblems(draft({ calls: 600 })).calls).toBeNull();
    for (const calls of [0, 601, 1.5, null])
      expect(draftProblems(draft({ calls })).calls).toBe('Enter a whole number from 1 to 600.');
    expect(draftProblems(draft({ megabytes: 0.01 })).megabytes).toBeNull();
    expect(draftProblems(draft({ megabytes: 1000 })).megabytes).toBeNull();
    for (const megabytes of [0.009, 1000.01, 0, -1, null])
      expect(draftProblems(draft({ megabytes })).megabytes).toBe(
        'Enter an amount from 0.01 MB to 1,000 MB.'
      );
    expect(draftProblems(draft({ kinds: [] })).kinds).toBe('Choose at least one kind of file.');
    expect(hasProblems(draftProblems(draft({ kinds: [], calls: 0 })))).toBe(true);
  });
  it('compares drafts regardless of the order the kinds were ticked', () => {
    const policy = parsePolicy(view())!;
    expect(sameDraft(draftOf(policy), draftOf(policy))).toBe(true);
    expect(
      sameDraft(
        draft({ kinds: ['presentation', 'document'] }),
        draft({ kinds: ['document', 'presentation'] })
      )
    ).toBe(true);
    expect(sameDraft(draft(), draft({ enabled: false }))).toBe(false);
    expect(sameDraft(draft(), draft({ minRole: 'owner' }))).toBe(false);
    expect(sameDraft(draft(), draft({ calls: 61 }))).toBe(false);
    expect(sameDraft(draft(), draft({ megabytes: 21 }))).toBe(false);
    expect(sameDraft(draft(), draft({ kinds: ['document'] }))).toBe(false);
  });
  it('names roles the way the app does and explains the floor', () => {
    expect(roleFloorLabel('contributor')).toBe('Contributor');
    expect(roleFloorLabel('reviewer')).toBe('Reviewer');
    expect(roleFloorLabel('unheard-of')).toBe('unheard-of');
    expect(floorHelp('viewer')).toMatch(/Every member/);
    expect(floorHelp('editor')).toBe(
      'Members below Editor cannot have an AI agent read this project.'
    );
  });
});

describe('request builders', () => {
  it('reads the policy', () => {
    expect(policyRequest(PROJECT)).toEqual({
      method: 'GET',
      path: `/projects/${PROJECT}/agent-policy`
    });
  });
  it('replaces the whole policy, naming the revision it was made from', () => {
    expect(
      saveRequest(PROJECT, 4, draft({ minRole: 'editor', megabytes: 5, kinds: ['document'] }))
    ).toEqual({
      method: 'PUT',
      path: `/projects/${PROJECT}/agent-policy`,
      headers: { 'if-match': '"4"' },
      body: {
        enabled: true,
        min_role: 'editor',
        kinds: ['document'],
        calls_per_minute: 60,
        read_bytes_per_day: 5_000_000
      }
    });
    // A policy that was never saved is revision 0.
    expect(saveRequest(PROJECT, 0, draft()).headers).toEqual({ 'if-match': '"0"' });
    expect(
      saveRequest(PROJECT, 1, draft({ kinds: ['presentation', 'spreadsheet'] })).body
    ).toMatchObject({
      kinds: ['spreadsheet', 'presentation']
    });
  });
  it('refuses to build a save the server would refuse', () => {
    expect(() => saveRequest(PROJECT, 1, draft({ calls: 601 }))).toThrow(/allowed ranges/);
    expect(() => saveRequest(PROJECT, 1, draft({ kinds: [] }))).toThrow(/allowed ranges/);
  });
  it('builds the kill request without a body', () => {
    expect(killRequest(PROJECT)).toEqual({
      method: 'POST',
      path: `/projects/${PROJECT}/agent-policy/kill`
    });
  });
  it('asks for the last seven UTC days, today included, and never more than 31', () => {
    const now = new Date('2026-10-20T23:30:00Z');
    expect(usageRequest(PROJECT, 7, now).path).toBe(
      `/projects/${PROJECT}/agent-usage?from=2026-10-14&to=2026-10-20`
    );
    expect(usageRequest(PROJECT, 1, now).path).toContain('from=2026-10-20&to=2026-10-20');
    expect(usageRequest(PROJECT, 400, now).path).toContain('from=2026-09-20&to=2026-10-20');
    expect(usageRequest(PROJECT, 0, now).path).toContain('from=2026-10-20&to=2026-10-20');
    expect(CALLS_PER_MINUTE.max).toBe(600);
  });
  it('encodes the project id', () => {
    expect(policyRequest('a/b').path).toBe('/projects/a%2Fb/agent-policy');
  });
});

describe('what the screen shows', () => {
  it('adds the clients of a day together, newest day first', () => {
    const row = (day: string, calls: number, bytesOut: number, refused: number, client = 'a') => ({
      day,
      client,
      calls,
      bytesOut,
      refused,
      errors: 0
    });
    expect(
      dailyTotals([
        row('2026-10-19', 2, 100, 0),
        row('2026-10-20', 3, 1000, 1, 'a'),
        row('2026-10-20', 4, 2000, 2, 'b')
      ])
    ).toEqual([
      { day: '2026-10-20', calls: 7, bytesOut: 3000, refused: 3 },
      { day: '2026-10-19', calls: 2, bytesOut: 100, refused: 0 }
    ]);
    expect(dailyTotals([])).toEqual([]);
  });
  it('formats sizes in decimal units and days as UTC dates', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(999)).toBe('999 B');
    expect(formatBytes(1500)).toBe('1.5 kB');
    expect(formatBytes(20_000_000)).toBe('20 MB');
    expect(formatBytes(1_234_567_890)).toBe('1.2 GB');
    expect(formatDay('2026-10-20')).toBe('20 Oct 2026');
    expect(formatDay('not a day')).toBe('not a day');
  });
  it('says what a save did, and when it cannot work yet', () => {
    const on = parsePolicy(view())!;
    expect(savedMessage(on)).toBe('Saved. AI agents are on for this project.');
    expect(savedMessage({ ...on, enabled: false })).toBe(
      'Saved. AI agents are off for this project.'
    );
    expect(savedMessage({ ...on, changed: false })).toMatch(/already saved/);
    expect(savedMessage({ ...on, gateway: false })).toMatch(
      /only once your operator turns the agent connector on/
    );
    expect(savedMessage({ ...on, enabled: false, gateway: false })).not.toMatch(/operator/);
  });
  it('says exactly what the kill switch did', () => {
    const result = { projectId: PROJECT, wasEnabled: true, revokedGrants: 2, revision: 5 };
    expect(killMessage(result)).toBe(
      'AI agents are off for this project. 2 AI agent keys lost their access to this project. Every open agent session is refused from now on. People’s own keys are untouched.'
    );
    expect(killMessage({ ...result, revokedGrants: 1 })).toContain(
      'One AI agent key lost its access'
    );
    expect(killMessage({ ...result, revokedGrants: 0, wasEnabled: false })).toMatch(
      /^AI agents were already off\. AI agents are off for this project\. No AI agent key had access/
    );
  });
  it('labels file overrides and files', () => {
    expect(overrideLabel('none')).toBe('Hidden from AI agents');
    expect(overrideLabel('read')).toMatch(/never gives agents more/);
    expect(fileLabel(FILE, { [FILE]: 'Q3 Policy' })).toBe('Q3 Policy');
    expect(fileLabel(FILE)).toBe('File 99999999');
  });
});

describe('agentMessage', () => {
  const message = (status: number, subject: Parameters<typeof agentMessage>[1], reason = '') =>
    agentMessage(new AgentPolicyError(status, reason), subject);
  it('maps each status to a short sentence and never to raw text', () => {
    const raw = 'select * from project_agent_policy violates constraint';
    for (const status of [400, 401, 403, 404, 409, 422, 429, 500, 503]) {
      const text = message(status, 'save', raw);
      expect(text).not.toContain('select');
      expect(text.length).toBeGreaterThan(10);
    }
    expect(message(401, 'save')).toMatch(/session has ended/);
    expect(message(403, 'load')).toMatch(/owner or manager/);
    expect(message(404, 'load')).toMatch(/not available on this server yet/);
    expect(message(404, 'save')).toMatch(/not found/);
    expect(message(429, 'save')).toMatch(/too many/i);
    expect(message(400, 'save')).toMatch(/numbers/);
  });
  it('explains a conflicting save', () => {
    expect(message(409, 'save')).toMatch(/Someone else changed these settings/);
    expect(isConflict(new AgentPolicyError(409, ''))).toBe(true);
    expect(isConflict(new AgentPolicyError(403, ''))).toBe(false);
    expect(isUnavailable(new AgentPolicyError(501, ''))).toBe(true);
  });
  it('says first that the agents were not stopped when the stop failed', () => {
    for (const status of [401, 403, 404, 422, 429, 500, 0])
      expect(message(status, 'kill')).toMatch(/^AI agents were not stopped\. /);
    expect(agentMessage(new TypeError('Failed to fetch'), 'kill')).toMatch(
      /^AI agents were not stopped\. Could not reach/
    );
    expect(agentMessage(new Error('boom'), 'kill')).not.toContain('boom');
    expect(message(500, 'save')).not.toMatch(/not stopped/);
  });
});

describe('requests over the wire', () => {
  afterEach(() => vi.unstubAllGlobals());
  const reply = (body: unknown, status = 200) =>
    vi.fn(
      async () =>
        new Response(JSON.stringify(body), {
          status,
          headers: { 'content-type': 'application/json' }
        })
    );
  const sent = (mock: ReturnType<typeof reply>) =>
    mock.mock.calls[0] as unknown as [string, RequestInit];

  it('loads the policy', async () => {
    const fetchMock = reply(view());
    vi.stubGlobal('fetch', fetchMock);
    expect((await loadPolicy(PROJECT)).revision).toBe(4);
    const [url, init] = sent(fetchMock);
    expect(url).toBe(`/api/engine/projects/${PROJECT}/agent-policy`);
    expect(init.method).toBe('GET');
  });
  it('saves with If-Match and the full body', async () => {
    const fetchMock = reply(view({ revision: 5, changed: true }));
    vi.stubGlobal('fetch', fetchMock);
    const saved = await savePolicy(PROJECT, 4, draft());
    expect(saved.revision).toBe(5);
    const [url, init] = sent(fetchMock);
    expect(url).toBe(`/api/engine/projects/${PROJECT}/agent-policy`);
    expect(init.method).toBe('PUT');
    const headers = init.headers as Record<string, string>;
    expect(headers['if-match']).toBe('"4"');
    expect(headers['content-type']).toBe('application/json');
    expect(JSON.parse(String(init.body))).toEqual({
      enabled: true,
      min_role: 'viewer',
      kinds: ['document', 'spreadsheet', 'presentation'],
      calls_per_minute: 60,
      read_bytes_per_day: 20_000_000
    });
  });
  it('throws a typed error for a refusal, keeping the status and the reason', async () => {
    vi.stubGlobal('fetch', reply({ error: 'conflict', details: { reason: 'stale' } }, 409));
    const error = await savePolicy(PROJECT, 1, draft()).catch((e) => e);
    expect(error).toBeInstanceOf(AgentPolicyError);
    expect(error).toMatchObject({ status: 409, reason: 'stale' });
    vi.stubGlobal('fetch', reply({ unexpected: true }));
    expect(await loadPolicy(PROJECT).catch((e) => e)).toMatchObject({ status: 0 });
  });
  it('pulls the kill switch with a POST and no body', async () => {
    const fetchMock = reply({
      project_id: PROJECT,
      enabled: false,
      was_enabled: true,
      revoked_grants: 3,
      revision: 6
    });
    vi.stubGlobal('fetch', fetchMock);
    expect(await killAgents(PROJECT)).toMatchObject({ revokedGrants: 3, wasEnabled: true });
    const [url, init] = sent(fetchMock);
    expect(url).toBe(`/api/engine/projects/${PROJECT}/agent-policy/kill`);
    expect(init.method).toBe('POST');
    expect(init.body).toBeUndefined();
  });
  it('loads usage for the last seven days', async () => {
    const fetchMock = reply({
      from: '2026-10-14',
      to: '2026-10-20',
      truncated: false,
      items: [],
      project_id: PROJECT
    });
    vi.stubGlobal('fetch', fetchMock);
    expect((await loadUsage(PROJECT)).items).toEqual([]);
    expect(sent(fetchMock)[0]).toMatch(
      new RegExp(
        `^/api/engine/projects/${PROJECT}/agent-usage\\?from=\\d{4}-\\d{2}-\\d{2}&to=\\d{4}-\\d{2}-\\d{2}$`
      )
    );
  });
});
