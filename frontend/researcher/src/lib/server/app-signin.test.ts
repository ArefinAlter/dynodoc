import { beforeEach, describe, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({
  accessToken: vi.fn(),
  enginePost: vi.fn(),
  saveSession: vi.fn(),
  config: { secrets: { webServiceKey: { expose: () => 'service-key-for-tests' } } }
}));
vi.mock('$env/dynamic/private', () => ({ env: {} }));
vi.mock('./config', () => ({ getConfig: () => mock.config }));
vi.mock('./session', () => ({
  accessToken: mock.accessToken,
  enginePost: mock.enginePost,
  saveSession: mock.saveSession,
  engineUrl: () => 'http://engine.invalid'
}));
import { clientHint, isInformativeAddress, issue, redeem, REFUSED } from './app-signin';

const CHALLENGE = 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM';
const STATE = 'b3JkZXJseS1zdGF0ZS12YWx1ZQ';
const VERIFIER = 'dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk';
const CODE = 'ab'.repeat(32);
const ORIGIN = 'https://app.example.test';

function context(
  body: unknown,
  {
    origin = ORIGIN,
    type = 'application/json',
    address
  }: { origin?: string | null; type?: string; address?: string } = {}
) {
  const headers: Record<string, string> = { 'content-type': type };
  if (origin !== null) headers.origin = origin;
  return {
    request: new Request(`${ORIGIN}/auth/app-signin`, {
      method: 'POST',
      headers,
      body: typeof body === 'string' ? body : JSON.stringify(body)
    }),
    url: new URL(`${ORIGIN}/auth/app-signin`),
    cookies: { set: vi.fn(), delete: vi.fn(), get: vi.fn() } as never,
    getClientAddress: () => {
      if (address === undefined) throw new Error('no address');
      return address;
    }
  };
}

const request = { challenge: CHALLENGE, state: STATE, platform: 'android' };
const finish = { code: CODE, state: STATE, verifier: VERIFIER };

beforeEach(() => {
  vi.resetAllMocks();
  vi.unstubAllGlobals();
});

describe('POST /auth/app-signin (Continue was pressed)', () => {
  it('asks the API for a code with the person’s own token and the service key, and returns the app link', async () => {
    mock.accessToken.mockResolvedValue('the-access-token');
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        new Response(JSON.stringify({ code: CODE, expires_in: 60 }), { status: 200 })
      );
    vi.stubGlobal('fetch', fetchMock);
    const response = await issue(context(request));
    expect(response.status).toBe(200);
    expect(response.headers.get('cache-control')).toBe('private, no-store');
    expect(await response.json()).toEqual({ url: `dynodoc://signin?code=${CODE}&state=${STATE}` });
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe('http://engine.invalid/auth/app-handover');
    expect(init.headers.authorization).toBe('Bearer the-access-token');
    expect(init.headers['x-dynodoc-service-key']).toBe('service-key-for-tests');
    expect(JSON.parse(init.body)).toEqual(request);
  });

  it('refuses another origin, a missing origin and a form post before doing anything', async () => {
    vi.stubGlobal('fetch', vi.fn());
    expect((await issue(context(request, { origin: 'https://evil.example' }))).status).toBe(403);
    expect((await issue(context(request, { origin: null }))).status).toBe(403);
    expect((await issue(context(request, { origin: `${ORIGIN}.evil.example` }))).status).toBe(403);
    expect(
      (await issue(context('a=b', { type: 'application/x-www-form-urlencoded' }))).status
    ).toBe(415);
    expect((await issue(context(request, { type: 'text/plain' }))).status).toBe(415);
    expect(mock.accessToken).not.toHaveBeenCalled();
    expect(fetch).not.toHaveBeenCalled();
  });

  it('refuses a malformed request and any extra field', async () => {
    vi.stubGlobal('fetch', vi.fn());
    for (const bad of [
      'not json',
      '[]',
      '"x"',
      {},
      { ...request, extra: 1 },
      { ...request, platform: 'windows' },
      { ...request, state: 'tiny' },
      { ...request, challenge: CHALLENGE.slice(1) },
      { ...request, challenge: 7 },
      { challenge: CHALLENGE, state: STATE }
    ])
      expect((await issue(context(bad))).status, JSON.stringify(bad)).toBe(400);
    expect(fetch).not.toHaveBeenCalled();
  });

  it('needs a signed-in browser', async () => {
    mock.accessToken.mockResolvedValue(null);
    vi.stubGlobal('fetch', vi.fn());
    expect((await issue(context(request))).status).toBe(401);
    expect(fetch).not.toHaveBeenCalled();
  });

  it('passes on a busy account, an ended session, and never forwards an odd answer', async () => {
    mock.accessToken.mockResolvedValue('token');
    const answer = (status: number, body: unknown = {}) =>
      vi.stubGlobal(
        'fetch',
        vi.fn().mockResolvedValue(new Response(JSON.stringify(body), { status }))
      );
    answer(429);
    expect((await issue(context(request))).status).toBe(429);
    answer(401);
    expect((await issue(context(request))).status).toBe(401);
    answer(500);
    expect((await issue(context(request))).status).toBe(502);
    // A 200 whose code is not a code is not turned into a link.
    for (const code of ['', 'x&evil=1', 7, null, `${CODE}&x=1`]) {
      answer(200, { code });
      const response = await issue(context(request));
      expect(response.status).toBe(502);
      expect(JSON.stringify(await response.json())).not.toContain('dynodoc://');
    }
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('down')));
    expect((await issue(context(request))).status).toBe(503);
  });

  it('never puts the code or a token in an error message', async () => {
    mock.accessToken.mockResolvedValue('secret-access-token');
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(JSON.stringify({ code: CODE }), { status: 500 }))
    );
    const text = await (await issue(context(request))).text();
    expect(text).not.toContain('secret-access-token');
    expect(text).not.toContain(CODE);
  });
});

describe('POST /auth/app-signin/redeem (the app’s window finishing)', () => {
  const tokens = { access_token: 'a', refresh_token: 'r', expires_in: 3600 };

  it('trades code, state and verifier for a session cookie', async () => {
    mock.enginePost.mockResolvedValue(tokens);
    const c = context(finish, { address: '203.0.113.5' });
    const response = await redeem(c);
    expect(response.status).toBe(200);
    expect(response.headers.get('cache-control')).toBe('private, no-store');
    expect(await response.json()).toEqual({ ok: true });
    expect(mock.enginePost).toHaveBeenCalledWith('/auth/app-handover/redeem', {
      ...finish,
      client: '203.0.113.5'
    });
    expect(mock.saveSession).toHaveBeenCalledWith(c.cookies, tokens, true);
  });

  it('sends no address when it would not tell visitors apart', async () => {
    mock.enginePost.mockResolvedValue(tokens);
    for (const address of [undefined, '127.0.0.1', '::1', '10.0.0.4', '192.168.1.5']) {
      mock.enginePost.mockClear();
      await redeem(context(finish, { address }));
      expect(mock.enginePost.mock.calls[0][1], String(address)).toEqual(finish);
    }
  });

  it('answers every failure the same way and sets no cookie', async () => {
    mock.enginePost.mockRejectedValue(new Error('The request could not be completed.'));
    const refused = await redeem(context(finish));
    expect(refused.status).toBe(401);
    const text = await refused.text();
    expect(JSON.parse(text)).toEqual({ error: REFUSED });
    for (const secret of [CODE, STATE, VERIFIER]) expect(text).not.toContain(secret);
    // A malformed request gets the same answer and does not reach the API.
    mock.enginePost.mockClear();
    for (const bad of [
      'nope',
      {},
      { ...finish, code: 'x' },
      { ...finish, state: 'x' },
      { ...finish, verifier: 'x' },
      { ...finish, extra: 1 },
      { code: CODE, verifier: VERIFIER }
    ]) {
      const response = await redeem(context(bad));
      expect(response.status, JSON.stringify(bad)).toBe(401);
      expect(await response.text()).toBe(JSON.stringify({ error: REFUSED }));
    }
    expect(mock.enginePost).not.toHaveBeenCalled();
    expect(mock.saveSession).not.toHaveBeenCalled();
  });

  it('refuses another origin and a form post', async () => {
    expect((await redeem(context(finish, { origin: 'https://evil.example' }))).status).toBe(403);
    expect((await redeem(context(finish, { origin: null }))).status).toBe(403);
    expect(
      (await redeem(context(finish, { type: 'application/x-www-form-urlencoded' }))).status
    ).toBe(415);
    expect(mock.enginePost).not.toHaveBeenCalled();
    expect(mock.saveSession).not.toHaveBeenCalled();
  });
});

describe('which visitor addresses are sent to the API', () => {
  it('only addresses that tell visitors apart', () => {
    for (const ok of ['203.0.113.9', '8.8.8.8', '2001:db8::1', '::ffff:203.0.113.9', '172.32.0.1'])
      expect(isInformativeAddress(ok), ok).toBe(true);
    for (const no of [
      '',
      'unknown',
      '127.0.0.1',
      '0.0.0.0',
      '10.1.2.3',
      '172.16.0.1',
      '172.31.255.255',
      '192.168.0.1',
      '169.254.1.1',
      '100.64.0.1',
      '224.0.0.1',
      '999.1.1.1',
      '::',
      '::1',
      'fe80::1',
      'fd00::1',
      'fc00::1',
      '::ffff:127.0.0.1',
      '::ffff:10.0.0.1',
      '1.2.3',
      'evil.example',
      '203.0.113.9 '
    ])
      expect(isInformativeAddress(no), JSON.stringify(no)).toBe(false);
    expect(clientHint(() => '203.0.113.9')).toBe('203.0.113.9');
    expect(clientHint(() => 'a'.repeat(65))).toBeUndefined();
    expect(
      clientHint(() => {
        throw new Error('no address');
      })
    ).toBeUndefined();
    expect(clientHint(undefined)).toBeUndefined();
  });
});
