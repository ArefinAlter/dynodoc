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
import {
  clientHint,
  formSubmission,
  ipv6Groups,
  isInformativeAddress,
  issueForForm,
  pairingCode,
  redeem,
  REFUSED
} from './app-signin';

const CHALLENGE = 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM';
const STATE = 'b3JkZXJseS1zdGF0ZS12YWx1ZQ';
const VERIFIER = 'dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk';
const CODE = 'ab'.repeat(32);
const ORIGIN = 'https://app.example.test';
const APP_UA =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0 DynodocApp/0.1.0 (desktop)';
const BROWSER_UA =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36';

type Headers = Record<string, string | null>;

/** What a real browser sends for a form the person submitted by a click on this site. */
const FORM: Headers = {
  origin: ORIGIN,
  'content-type': 'application/x-www-form-urlencoded',
  'sec-fetch-site': 'same-origin',
  'sec-fetch-mode': 'navigate',
  'sec-fetch-user': '?1',
  'sec-fetch-dest': 'document'
};

function made(headers: Headers, body: BodyInit = 'x=1') {
  const clean: Record<string, string> = {};
  for (const [k, v] of Object.entries(headers)) if (v !== null) clean[k] = v;
  return {
    request: new Request(`${ORIGIN}/app-signin`, { method: 'POST', headers: clean, body }),
    url: new URL(`${ORIGIN}/app-signin?challenge=${CHALLENGE}&state=${STATE}&platform=android`),
    cookies: { set: vi.fn(), delete: vi.fn(), get: vi.fn() } as never
  };
}

const parsed = { challenge: CHALLENGE, state: STATE, platform: 'android' as const };

beforeEach(() => {
  vi.resetAllMocks();
  vi.unstubAllGlobals();
});

describe('the four characters that pair the page with the app', () => {
  it('are a fixed function of the state, with vectors the app checks too', () => {
    for (const [state, want] of [
      ['b3JkZXJseS1zdGF0ZS12YWx1ZQ', '4K77'],
      ['a'.repeat(16), 'EDCG'],
      ['K8710daSV_AjCvmPElN6z6L-DQ2YusRS', '6CR6'],
      ['0123456789abcdef0123456789abcdef', 'YKX5'],
      ['g'.repeat(64), 'GLJA']
    ])
      expect(pairingCode(state), state).toBe(want);
  });

  it('use only symbols a person cannot confuse, and spread over many values', () => {
    const seen = new Set<string>();
    for (let i = 0; i < 2000; i += 1) {
      const code = pairingCode(`state-${i}-${'x'.repeat(16)}`);
      expect(code).toMatch(/^[2-9A-HJ-NP-Z]{4}$/);
      seen.add(code);
    }
    // 20 bits of a good hash: 2000 draws very nearly never repeat.
    expect(seen.size).toBeGreaterThan(1990);
  });
});

describe('asking for a code: only a real, user-activated form submission of this site', () => {
  it('accepts exactly what a browser sends for a click on a form of this site', () => {
    expect(formSubmission(made(FORM).request, made(FORM).url)).toBeNull();
    const multipart = { ...FORM, 'content-type': 'multipart/form-data; boundary=x' };
    expect(formSubmission(made(multipart).request, made(multipart).url)).toBeNull();
  });

  it('refuses a script: fetch, XHR and a form submitted without a click differ in a header', () => {
    const cases: Record<string, Headers> = {
      'a script fetch (cors mode, empty destination)': {
        ...FORM,
        'sec-fetch-mode': 'cors',
        'sec-fetch-dest': 'empty',
        'sec-fetch-user': null
      },
      'a form a script submitted without a click (no user flag)': {
        ...FORM,
        'sec-fetch-user': null
      },
      'a user flag that is not ?1': { ...FORM, 'sec-fetch-user': '?0' },
      'a cross-site form': { ...FORM, 'sec-fetch-site': 'cross-site' },
      'a same-site (sibling host) form': { ...FORM, 'sec-fetch-site': 'same-site' },
      'a browser-initiated navigation': { ...FORM, 'sec-fetch-site': 'none' },
      'a frame': { ...FORM, 'sec-fetch-dest': 'iframe' },
      'a no-cors request': { ...FORM, 'sec-fetch-mode': 'no-cors' },
      'another origin': { ...FORM, origin: 'https://evil.example' },
      'no origin': { ...FORM, origin: null },
      'origin null': { ...FORM, origin: 'null' },
      JSON: { ...FORM, 'content-type': 'application/json' },
      'plain text': { ...FORM, 'content-type': 'text/plain' },
      'no content type': { ...FORM, 'content-type': null }
    };
    for (const [name, headers] of Object.entries(cases))
      expect(formSubmission(made(headers).request, made(headers).url), name).toBe('blocked');
  });

  it('fails closed, with its own message, for a browser that sends none of the headers', () => {
    const none = {
      origin: ORIGIN,
      'content-type': 'application/x-www-form-urlencoded'
    };
    expect(formSubmission(made(none).request, made(none).url)).toBe('browser');
  });

  it('makes the code with the person’s own token and the service key, and returns the app link', async () => {
    mock.accessToken.mockResolvedValue('the-access-token');
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        new Response(JSON.stringify({ code: CODE, expires_in: 60 }), { status: 200 })
      );
    vi.stubGlobal('fetch', fetchMock);
    const outcome = await issueForForm(made(FORM), parsed);
    expect(outcome).toEqual({ ok: true, url: `dynodoc://signin?code=${CODE}&state=${STATE}` });
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe('http://engine.invalid/auth/app-handover');
    expect(init.headers.authorization).toBe('Bearer the-access-token');
    expect(init.headers['x-dynodoc-service-key']).toBe('service-key-for-tests');
    expect(JSON.parse(init.body)).toEqual(parsed);
  });

  it('does nothing at all for a refused submission', async () => {
    vi.stubGlobal('fetch', vi.fn());
    const script = {
      ...FORM,
      'sec-fetch-mode': 'cors',
      'sec-fetch-dest': 'empty',
      'sec-fetch-user': null
    };
    expect(await issueForForm(made(script), parsed)).toEqual({
      ok: false,
      status: 403,
      problem: 'blocked'
    });
    const none = { origin: ORIGIN, 'content-type': 'application/x-www-form-urlencoded' };
    expect(await issueForForm(made(none), parsed)).toEqual({
      ok: false,
      status: 400,
      problem: 'browser'
    });
    expect(mock.accessToken).not.toHaveBeenCalled();
    expect(fetch).not.toHaveBeenCalled();
  });

  it('needs a signed-in browser, and passes on a busy account and an ended session', async () => {
    mock.accessToken.mockResolvedValue(null);
    vi.stubGlobal('fetch', vi.fn());
    expect(await issueForForm(made(FORM), parsed)).toMatchObject({
      ok: false,
      problem: 'signed-out'
    });
    expect(fetch).not.toHaveBeenCalled();
    mock.accessToken.mockResolvedValue('token');
    const answer = (status: number, body: unknown = {}) =>
      vi.stubGlobal(
        'fetch',
        vi.fn().mockResolvedValue(new Response(JSON.stringify(body), { status }))
      );
    answer(429);
    expect(await issueForForm(made(FORM), parsed)).toMatchObject({ status: 429, problem: 'busy' });
    answer(401);
    expect(await issueForForm(made(FORM), parsed)).toMatchObject({
      status: 401,
      problem: 'signed-out'
    });
    answer(500);
    expect(await issueForForm(made(FORM), parsed)).toMatchObject({
      status: 502,
      problem: 'failed'
    });
    // A 200 whose code is not a code is not turned into a link.
    for (const code of ['', 'x&evil=1', 7, null, `${CODE}&x=1`]) {
      answer(200, { code });
      const outcome = await issueForForm(made(FORM), parsed);
      expect(outcome.ok).toBe(false);
      expect(JSON.stringify(outcome)).not.toContain('dynodoc://');
    }
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('down')));
    expect(await issueForForm(made(FORM), parsed)).toMatchObject({
      status: 503,
      problem: 'failed'
    });
  });

  it('never puts the code or a token in what it returns on a failure', async () => {
    mock.accessToken.mockResolvedValue('secret-access-token');
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(JSON.stringify({ code: CODE }), { status: 500 }))
    );
    const text = JSON.stringify(await issueForForm(made(FORM), parsed));
    expect(text).not.toContain('secret-access-token');
    expect(text).not.toContain(CODE);
  });
});

describe('POST /auth/app-signin/redeem (the app’s window finishing)', () => {
  const finish = { code: CODE, state: STATE, verifier: VERIFIER };
  const tokens = { access_token: 'a', refresh_token: 'r', expires_in: 3600 };
  const JSON_HEADERS = { origin: ORIGIN, 'content-type': 'application/json' };

  function context(
    body: unknown,
    {
      headers = {},
      userAgent = APP_UA,
      address
    }: { headers?: Headers; userAgent?: string | null; address?: string } = {}
  ) {
    const all: Headers = { ...JSON_HEADERS, ...headers };
    if (userAgent !== null) all['user-agent'] = userAgent;
    const clean: Record<string, string> = {};
    for (const [k, v] of Object.entries(all)) if (v !== null) clean[k] = v;
    return {
      request: new Request(`${ORIGIN}/auth/app-signin/redeem`, {
        method: 'POST',
        headers: clean,
        body: typeof body === 'string' ? body : JSON.stringify(body)
      }),
      url: new URL(`${ORIGIN}/auth/app-signin/redeem`),
      cookies: { set: vi.fn(), delete: vi.fn(), get: vi.fn() } as never,
      getClientAddress: () => {
        if (address === undefined) throw new Error('no address');
        return address;
      }
    };
  }

  it('trades code, state and verifier for a session cookie, naming the kind of app', async () => {
    mock.enginePost.mockResolvedValue(tokens);
    const c = context(finish, { address: '203.0.113.5' });
    const response = await redeem(c);
    expect(response.status).toBe(200);
    expect(response.headers.get('cache-control')).toBe('private, no-store');
    expect(await response.json()).toEqual({ ok: true });
    expect(mock.enginePost).toHaveBeenCalledWith('/auth/app-handover/redeem', {
      ...finish,
      app: 'desktop',
      client: '203.0.113.5'
    });
    expect(mock.saveSession).toHaveBeenCalledWith(c.cookies, tokens, true);
  });

  it('refuses a request that does not come from the app’s own window, before anything else', async () => {
    for (const userAgent of [
      BROWSER_UA,
      null,
      '',
      'DynodocApp/0.1.0',
      `${BROWSER_UA} DynodocApp/0.1.0 (web)`,
      `${BROWSER_UA} DynodocApp/0.1 (desktop)`,
      `${BROWSER_UA} xDynodocApp/0.1.0 (desktop)`,
      `${BROWSER_UA} DynodocApp/0.1.0 (desktop) DynodocApp/0.1.0 (android)`,
      `${BROWSER_UA} DynodocApp/0.1.0 (desktop)x`,
      `${APP_UA} ${'x'.repeat(1100)}`
    ]) {
      mock.enginePost.mockClear();
      mock.saveSession.mockClear();
      const response = await redeem(context(finish, { userAgent }));
      expect(response.status, String(userAgent).slice(0, 60)).toBe(401);
      expect(await response.json()).toEqual({ error: REFUSED });
      expect(mock.enginePost).not.toHaveBeenCalled();
      expect(mock.saveSession).not.toHaveBeenCalled();
    }
  });

  it('takes the kind of app from the token, for each of the three', async () => {
    mock.enginePost.mockResolvedValue(tokens);
    for (const kind of ['desktop', 'android', 'ios']) {
      mock.enginePost.mockClear();
      await redeem(context(finish, { userAgent: `${BROWSER_UA} DynodocApp/12.3.45 (${kind})` }));
      expect(mock.enginePost.mock.calls[0][1].app).toBe(kind);
    }
  });

  it('sends no address when it would not tell visitors apart', async () => {
    mock.enginePost.mockResolvedValue(tokens);
    for (const address of [undefined, '127.0.0.1', '::1', '10.0.0.4', '192.168.1.5']) {
      mock.enginePost.mockClear();
      await redeem(context(finish, { address }));
      expect(mock.enginePost.mock.calls[0][1], String(address)).toEqual({
        ...finish,
        app: 'desktop'
      });
    }
  });

  it('answers every failure the same way and sets no cookie', async () => {
    mock.enginePost.mockRejectedValue(new Error('The request could not be completed.'));
    const refused = await redeem(context(finish));
    expect(refused.status).toBe(401);
    const text = await refused.text();
    expect(JSON.parse(text)).toEqual({ error: REFUSED });
    for (const secret of [CODE, STATE, VERIFIER]) expect(text).not.toContain(secret);
    mock.enginePost.mockClear();
    for (const bad of [
      'nope',
      {},
      { ...finish, code: 'x' },
      { ...finish, state: 'x' },
      { ...finish, verifier: 'x' },
      { ...finish, extra: 1 },
      { ...finish, app: 'ios' },
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
    expect(
      (await redeem(context(finish, { headers: { origin: 'https://evil.example' } }))).status
    ).toBe(403);
    expect((await redeem(context(finish, { headers: { origin: null } }))).status).toBe(403);
    expect(
      (
        await redeem(
          context(finish, { headers: { 'content-type': 'application/x-www-form-urlencoded' } })
        )
      ).status
    ).toBe(415);
    expect(mock.enginePost).not.toHaveBeenCalled();
    expect(mock.saveSession).not.toHaveBeenCalled();
  });
});

describe('which visitor addresses are sent to the API', () => {
  it('only addresses that tell visitors apart', () => {
    for (const ok of [
      '203.0.113.9',
      '8.8.8.8',
      '2001:db8::1',
      '2606:4700:4700::1111',
      '::ffff:203.0.113.9',
      '::ffff:cb00:7109',
      '172.32.0.1'
    ])
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
      '1.2.3',
      'evil.example',
      '203.0.113.9 ',
      // every spelling of the IPv6 loopback and unspecified address
      '::',
      '::1',
      '0:0:0:0:0:0:0:1',
      '0000:0000:0000:0000:0000:0000:0000:0001',
      '0::1',
      '::0001',
      '0:0:0:0:0:0:0:0',
      // IPv4 inside IPv6, in each spelling
      '::ffff:127.0.0.1',
      '::ffff:7f00:1',
      '::FFFF:7F00:0001',
      '0:0:0:0:0:ffff:127.0.0.1',
      '0:0:0:0:0:ffff:7f00:1',
      '::ffff:10.0.0.1',
      '::ffff:a00:1',
      '::ffff:192.168.1.1',
      '::ffff:c0a8:101',
      '::127.0.0.1',
      '64:ff9b::7f00:1',
      '64:ff9b::10.0.0.1',
      '2002:7f00:1::1',
      '2002:a00:1::',
      // link-local (with and without a zone), site-local, unique local, multicast, discard
      'fe80::1',
      'fe80::1%eth0',
      'fe80::1%25eth0',
      'febf::1',
      'fec0::1',
      'fd00::1',
      'fc00::1',
      'fdff:ffff::1',
      'ff02::1',
      '100::1',
      // not addresses
      '1:2:3:4:5:6:7',
      '1:2:3:4:5:6:7:8:9',
      '1::2::3',
      'g::1',
      ':::',
      '12345::1',
      '::ffff:999.1.1.1'
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

  it('reads IPv6 in every spelling to the same eight groups', () => {
    const loopback = [0, 0, 0, 0, 0, 0, 0, 1];
    for (const text of ['::1', '0:0:0:0:0:0:0:1', '0::1', '::0001', '0000::0001', '::1%lo'])
      expect(ipv6Groups(text), text).toEqual(loopback);
    expect(ipv6Groups('::ffff:127.0.0.1')).toEqual([0, 0, 0, 0, 0, 0xffff, 0x7f00, 1]);
    expect(ipv6Groups('2001:db8::8:800:200c:417a')).toEqual([
      0x2001, 0xdb8, 0, 0, 8, 0x800, 0x200c, 0x417a
    ]);
    for (const bad of [
      '',
      '::1::',
      '1:2',
      ':::',
      'abcde::',
      '1:2:3:4:5:6:7:8:9',
      'fe80::g',
      '1.2.3.4'
    ])
      expect(ipv6Groups(bad), bad).toBeNull();
  });
});
