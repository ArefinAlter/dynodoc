import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  AGENT_SCOPES,
  DeviceError,
  ProofFileError,
  SCOPES,
  accessRequest,
  deviceMessage,
  enrollKey,
  enrollRequest,
  grantRequest,
  grantableProjects,
  isUnavailable,
  labelProblem,
  normalizeScopes,
  parseProjectChoices,
  parseRegistry,
  revokeGrantRequest,
  scopeClass,
  scopeLabel,
  scopeUsable,
  scopesFor,
  shortKey,
  unusableScopes
} from './device-grants';

// Synthetic values only. Shapes mirror docs/DEVICE-KEY-GRANTS-API.md.
const KEY = '11111111-1111-4111-8111-111111111111';
const PROJECT = '22222222-2222-4222-8222-222222222222';
const PUBLIC = 'ab12cd34'.repeat(8);
const registry = (keys: unknown[], extra: Record<string, unknown> = {}) => ({
  authority: '33333333-3333-4333-8333-333333333333',
  identity: '44444444-4444-4444-8444-444444444444',
  active_limit: 20,
  total_limit: 128,
  grant_limit: 50,
  scopes: SCOPES.map((s) => s.id),
  keys,
  ...extra
});
const key = (extra: Record<string, unknown> = {}) => ({
  id: KEY,
  public_key: PUBLIC,
  registered_at: '2026-10-13T09:00:00Z',
  revoked_at: null,
  project_access: 'granted',
  grants: [],
  ...extra
});

describe('scopes', () => {
  it('names every scope in plain language and keeps display order', () => {
    expect(SCOPES.map((s) => s.label)).toEqual([
      'Read project data',
      'Read history',
      'Write history',
      'Review'
    ]);
    expect(scopeLabel('history:review')).toBe('Review');
    expect(scopeLabel('something:new')).toBe('something:new');
  });
  it('normalizes: known only, de-duplicated, display order', () => {
    expect(
      normalizeScopes(['history:review', 'nope', 'history:read', 'history:read', 'project:read'])
    ).toEqual(['project:read', 'history:read', 'history:review']);
    expect(normalizeScopes([])).toEqual([]);
  });
  it('flags a scope the project role cannot use, like the server does', () => {
    expect(scopeUsable('viewer', 'history:read')).toBe(true);
    expect(scopeUsable('viewer', 'history:write')).toBe(false);
    expect(scopeUsable('reviewer', 'history:write')).toBe(true);
    expect(scopeUsable('reviewer', 'history:review')).toBe(false);
    expect(scopeUsable('approver', 'history:review')).toBe(true);
    expect(scopeUsable('owner', 'history:review')).toBe(true);
    expect(scopeUsable(null, 'project:read')).toBe(false);
    // A role this page does not know is never flagged.
    expect(scopeUsable('auditor', 'history:review')).toBe(true);
    expect(unusableScopes('viewer', ['history:read', 'history:write', 'history:review'])).toEqual([
      'history:write',
      'history:review'
    ]);
  });
});

describe('parseRegistry', () => {
  it('reads access modes, grants and limits', () => {
    const parsed = parseRegistry(
      registry([
        key({
          grants: [
            {
              project_id: PROJECT,
              project_name: 'Board packs',
              role: 'owner',
              scopes: ['history:write', 'history:read'],
              created_at: '2026-10-13T09:30:00Z'
            }
          ]
        }),
        key({ id: 'x', project_access: 'all', revoked_at: '2026-10-13T10:00:00Z' })
      ])
    );
    expect(parsed?.grantLimit).toBe(50);
    expect(parsed?.activeLimit).toBe(20);
    expect(parsed?.keys).toHaveLength(2);
    expect(parsed?.keys[0].projectAccess).toBe('granted');
    expect(parsed?.keys[0].grants).toEqual([
      {
        projectId: PROJECT,
        projectName: 'Board packs',
        role: 'owner',
        scopes: ['history:read', 'history:write'],
        createdAt: '2026-10-13T09:30:00Z'
      }
    ]);
    expect(parsed?.keys[1].projectAccess).toBe('all');
    expect(parsed?.keys[1].revokedAt).toBe('2026-10-13T10:00:00Z');
  });
  it('keeps a grant for a project the account left, without a name or role', () => {
    const parsed = parseRegistry(
      registry([
        key({
          grants: [
            {
              project_id: PROJECT,
              project_name: null,
              role: null,
              scopes: ['project:read'],
              created_at: '2026-10-13T09:30:00Z'
            }
          ]
        })
      ])
    );
    expect(parsed?.keys[0].grants[0]).toMatchObject({ projectName: null, role: null });
  });
  it('has no access mode on a server that predates grants, and defaults the limit', () => {
    const old = key();
    delete (old as Record<string, unknown>).project_access;
    delete (old as Record<string, unknown>).grants;
    const parsed = parseRegistry(registry([old], { grant_limit: undefined }));
    expect(parsed?.keys[0].projectAccess).toBeNull();
    expect(parsed?.keys[0].grants).toEqual([]);
    expect(parsed?.grantLimit).toBe(50);
  });
  it('skips what it cannot read and refuses a body that is not a registry', () => {
    const parsed = parseRegistry(
      registry([
        { id: KEY },
        key({
          grants: [
            { project_id: PROJECT, scopes: [], created_at: 'x' },
            { project_id: PROJECT, scopes: ['unknown:scope'], created_at: 'x' },
            { scopes: ['history:read'], created_at: 'x' }
          ]
        })
      ])
    );
    expect(parsed?.keys).toHaveLength(1);
    expect(parsed?.keys[0].grants).toEqual([]);
    expect(parseRegistry(null)).toBeNull();
    expect(parseRegistry({ keys: 'no' })).toBeNull();
  });
});

describe('projects for the picker', () => {
  it('reads id, name and role, and lists only projects without a grant', () => {
    const choices = parseProjectChoices({
      items: [
        { id: 'b', name: 'beta', role: 'editor' },
        { id: 'a', name: 'Alpha', role: 'owner' },
        { id: 'c', name: 'Gamma', role: 'viewer' },
        { name: 'No id' }
      ]
    });
    expect(choices.map((c) => c.name)).toEqual(['beta', 'Alpha', 'Gamma']);
    const free = grantableProjects(choices, [
      {
        projectId: 'c',
        projectName: 'Gamma',
        role: 'viewer',
        scopes: ['project:read'],
        createdAt: ''
      }
    ]);
    expect(free.map((c) => c.name)).toEqual(['Alpha', 'beta']);
    expect(parseProjectChoices(null)).toEqual([]);
  });
});

describe('request builders', () => {
  it('builds the grant request with normalized scopes', () => {
    expect(grantRequest(KEY, PROJECT, ['history:write', 'history:read', 'history:read'])).toEqual({
      method: 'PUT',
      path: `/devices/${KEY}/projects/${PROJECT}`,
      body: { scopes: ['history:read', 'history:write'] }
    });
  });
  it('builds the grant revocation without a body', () => {
    expect(revokeGrantRequest(KEY, PROJECT)).toEqual({
      method: 'DELETE',
      path: `/devices/${KEY}/projects/${PROJECT}`
    });
  });
  it('encodes path segments', () => {
    expect(revokeGrantRequest('a/b', 'c d').path).toBe('/devices/a%2Fb/projects/c%20d');
  });
  it('sends the explicit confirmation for all projects and refuses to omit it', () => {
    expect(() => accessRequest(KEY, 'all')).toThrow(/confirmation/i);
    expect(accessRequest(KEY, 'all', true)).toEqual({
      method: 'PATCH',
      path: `/devices/${KEY}`,
      body: { project_access: 'all', confirm_all_projects: true }
    });
  });
  it('switches back to granted without a confirmation field', () => {
    expect(accessRequest(KEY, 'granted')).toEqual({
      method: 'PATCH',
      path: `/devices/${KEY}`,
      body: { project_access: 'granted' }
    });
  });
});

describe('deviceMessage', () => {
  const message = (status: number, subject: Parameters<typeof deviceMessage>[1], reason = '') =>
    deviceMessage(new DeviceError(status, reason), subject);
  it('maps each status to a short, specific sentence and never to raw text', () => {
    const raw = 'select * from device_key_project_grant violates constraint';
    for (const status of [400, 401, 403, 404, 409, 422, 429, 500, 502, 503]) {
      const text = message(status, 'grant', raw);
      expect(text).not.toContain('select');
      expect(text).not.toMatch(/^\s*$/);
      expect(text.length).toBeLessThan(140);
    }
    expect(message(401, 'grant')).toMatch(/session has ended/i);
    expect(message(403, 'grant')).toMatch(/not a member/i);
    expect(message(404, 'grant')).toMatch(/not found/i);
    expect(message(422, 'grant')).toMatch(/did not understand/i);
    expect(message(429, 'grant')).toMatch(/too many/i);
    expect(message(503, 'grant')).toMatch(/try again/i);
  });
  it('tells the 50-grant limit and a revoked key apart', () => {
    expect(message(409, 'grant', 'A key can have at most 50 project grants')).toMatch(
      /50 project grants/
    );
    expect(message(409, 'grant', 'The key is revoked')).toMatch(/was revoked/);
    expect(message(409, 'grant', 'something else')).toMatch(/changed while/);
  });
  it('explains a missing confirmation and an unavailable server', () => {
    expect(message(400, 'mode')).toMatch(/confirm/i);
    expect(message(404, 'load')).toMatch(/not available on this server/i);
    expect(isUnavailable(new DeviceError(501, ''))).toBe(true);
    expect(isUnavailable(new DeviceError(403, ''))).toBe(false);
  });
  it('handles a network failure, a file problem and anything else', () => {
    expect(deviceMessage(new TypeError('Failed to fetch'), 'load')).toMatch(/could not reach/i);
    expect(deviceMessage(new ProofFileError('Choose the proof JSON file.'), 'enroll')).toBe(
      'Choose the proof JSON file.'
    );
    expect(deviceMessage(new Error('boom: stack trace'), 'grant')).not.toContain('boom');
  });
});

describe('shortKey', () => {
  it('names a key by its first eight characters', () => {
    expect(shortKey(PUBLIC)).toBe('ab12cd34');
  });
});

describe('keys for an AI agent', () => {
  it('has one agent scope, kept apart from a person’s scopes', () => {
    expect(AGENT_SCOPES.map((s) => s.id)).toEqual(['content:read']);
    expect(scopeLabel('content:read')).toBe('Read documents');
    expect(scopesFor('agent')).toBe(AGENT_SCOPES);
    expect(scopesFor('person')).toBe(SCOPES);
    expect(SCOPES.map((s) => s.id)).not.toContain('content:read');
    expect(scopeClass('content:read')).toBe('agent');
    expect(scopeClass('history:write')).toBe('person');
    expect(scopeClass('something:new')).toBeNull();
    expect(normalizeScopes(['content:read', 'history:read', 'content:read'])).toEqual([
      'history:read',
      'content:read'
    ]);
  });
  it('reads the class and name of each key, and treats a missing class as a person', () => {
    const old = key({ id: 'old' });
    const parsed = parseRegistry(
      registry([
        key({
          class: 'agent',
          label: '  Claude on my laptop ',
          grants: [
            {
              project_id: PROJECT,
              project_name: 'Board packs',
              role: 'viewer',
              scopes: ['content:read'],
              created_at: '2026-10-20T09:30:00Z'
            }
          ]
        }),
        key({ id: 'blank', class: 'person', label: '   ' }),
        key({ id: 'odd', class: 'robot' }),
        old
      ])
    );
    expect(parsed?.keys.map((k) => [k.keyClass, k.label])).toEqual([
      ['agent', 'Claude on my laptop'],
      ['person', null],
      ['person', null],
      ['person', null]
    ]);
    expect(parsed?.keys[0].grants[0].scopes).toEqual(['content:read']);
  });
  it('checks a key name the way the server does', () => {
    expect(labelProblem('Claude on my laptop')).toBeNull();
    expect(labelProblem('x'.repeat(80))).toBeNull();
    expect(labelProblem('é'.repeat(80))).toBeNull();
    expect(labelProblem('x'.repeat(81))).toMatch(/at most 80/);
    expect(labelProblem('   ')).toMatch(/name/i);
    expect(labelProblem('two\nlines')).toMatch(/control characters/);
  });
  it('sends a person’s proof exactly as before, and adds class and name for an agent', () => {
    const proof = { challenge: { id: 'c' }, signature: 'ab' };
    expect(enrollRequest(proof).body).toBe(proof);
    expect(enrollRequest(proof, { keyClass: 'person', label: 'ignored' }).body).toBe(proof);
    expect(enrollRequest(proof, { keyClass: 'agent', label: '  Claude  ' })).toEqual({
      method: 'POST',
      path: '/devices',
      body: { challenge: { id: 'c' }, signature: 'ab', class: 'agent', label: 'Claude' }
    });
    expect(enrollRequest(proof, { keyClass: 'agent', label: ' ' }).body).toEqual({
      ...proof,
      class: 'agent'
    });
    // Not an object: nothing to add to, and the server will say it is not a proof.
    expect(enrollRequest('nope', { keyClass: 'agent', label: 'x' }).body).toBe('nope');
  });
  it('builds a grant of the agent scope and refuses to mix it with a person’s scopes', () => {
    expect(grantRequest(KEY, PROJECT, ['content:read'])).toEqual({
      method: 'PUT',
      path: `/devices/${KEY}/projects/${PROJECT}`,
      body: { scopes: ['content:read'] }
    });
    expect(() => grantRequest(KEY, PROJECT, ['content:read', 'history:read'])).toThrow(
      /never both/
    );
  });
  it('explains the class errors in plain words, never with the server text', () => {
    const message = (subject: Parameters<typeof deviceMessage>[1], reason: string) =>
      deviceMessage(new DeviceError(400, reason), subject);
    expect(message('grant', 'An agent key can hold only the content:read scope')).toBe(
      'A key for an AI agent can only be given Read documents.'
    );
    expect(
      message('grant', 'content scopes need a key enrolled for an AI agent (class agent)')
    ).toMatch(/Only a key set up for an AI agent can be given Read documents/);
    expect(message('grant', 'Choose one or more of project:read')).toBe(
      'Choose at least one permission for the project.'
    );
    expect(message('enroll', 'class must be person or agent')).toMatch(
      /for you or for an AI agent/
    );
    expect(message('enroll', 'A label needs a class')).toMatch(/for you or for an AI agent/);
    expect(message('enroll', 'label must be 1 to 80 characters without control characters')).toBe(
      'The key name must be 1 to 80 characters, with no line breaks.'
    );
    expect(message('enroll', 'something else')).toMatch(/proof was not accepted/);
    expect(deviceMessage(new DeviceError(422, ''), 'enroll')).toMatch(/may not be available/);
    expect(deviceMessage(new DeviceError(422, ''), 'grant')).toMatch(/did not understand/);
    for (const text of [
      message('grant', 'An agent key can hold only the content:read scope'),
      message('enroll', 'class must be person or agent')
    ])
      expect(text).not.toMatch(/content:read|class must/);
  });
});

describe('enrollKey', () => {
  afterEach(() => vi.unstubAllGlobals());
  const reply = (body: unknown) =>
    vi.fn(
      async () =>
        new Response(JSON.stringify(body), {
          status: 200,
          headers: { 'content-type': 'application/json' }
        })
    );
  it('posts the proof with the agent choice and reads the class back', async () => {
    const fetchMock = reply({
      id: KEY,
      public_key: PUBLIC,
      project_access: 'granted',
      class: 'agent'
    });
    vi.stubGlobal('fetch', fetchMock);
    const enrolled = await enrollKey(
      { challenge: { id: 'c' }, signature: 'ab' },
      { keyClass: 'agent', label: 'Claude' }
    );
    expect(enrolled).toEqual({
      id: KEY,
      publicKey: PUBLIC,
      projectAccess: 'granted',
      keyClass: 'agent'
    });
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/engine/devices');
    expect(init.method).toBe('POST');
    expect(JSON.parse(String(init.body))).toEqual({
      challenge: { id: 'c' },
      signature: 'ab',
      class: 'agent',
      label: 'Claude'
    });
  });
  it('sends nothing extra for a person’s key, and treats a missing class as a person', async () => {
    const fetchMock = reply({ id: KEY, public_key: PUBLIC, project_access: 'granted' });
    vi.stubGlobal('fetch', fetchMock);
    const proof = { challenge: { id: 'c' }, signature: 'ab' };
    const enrolled = await enrollKey(proof, { keyClass: 'person', label: '' });
    expect(enrolled.keyClass).toBe('person');
    const [, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(JSON.parse(String(init.body))).toEqual(proof);
  });
});
