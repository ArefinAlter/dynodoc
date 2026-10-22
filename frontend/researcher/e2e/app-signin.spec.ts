import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';
import { spawn, type ChildProcess } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { createServer, request as httpRequest, type IncomingMessage, type Server } from 'node:http';
import { fileURLToPath } from 'node:url';

// Signing a Dynodoc app (desktop or phone) in from the system browser: docs/decisions/017.
// The pages and routes run in Chromium. The app's own half (the shell that opens the browser,
// receives dynodoc://signin and opens /app-signin/finish) is played by a browser context that
// carries what the app's window carries: the `DynodocApp/<version> (<kind>)` User-Agent token and
// the shell's `__TAURI_INTERNALS__` bridge object, with no cookies of its own. The shell hands the
// finish page its secrets by setting `location.hash` once the page says it is ready (the
// `data-finish-ready` attribute); `shellHands` below does what the shell's script does.

const b64url = (bytes: Buffer) => bytes.toString('base64url');
function pkce() {
  const verifier = b64url(randomBytes(32));
  return {
    verifier,
    challenge: b64url(createHash('sha256').update(verifier).digest()),
    state: b64url(randomBytes(24))
  };
}
type Pkce = ReturnType<typeof pkce>;
const requestPath = (p: Pkce, platform = 'android') =>
  `/app-signin?challenge=${p.challenge}&state=${p.state}&platform=${platform}`;
/** What the shell sets as `location.hash` once the finish page is ready. */
const hashOf = (code: string, p: Pkce, verifier = p.verifier, state = p.state) =>
  `code=${code}&state=${state}&verifier=${verifier}`;

// The same function as `pairingCode` in src/lib/server/app-signin.ts and `pairing_code` in the shell.
const ALPHABET = '23456789ABCDEFGHJKLMNPQRSTUVWXYZ';
const pairing = (state: string) =>
  [0, 1, 2, 3].map((i) => ALPHABET[createHash('sha256').update(state).digest()[i] & 31]).join('');

let defaultAgent: string | undefined;
/** The browser's own User-Agent, to which the app's window adds its token. */
async function browserAgent(browser: Browser) {
  if (!defaultAgent) {
    const probe = await browser.newPage();
    defaultAgent = await probe.evaluate(() => navigator.userAgent);
    await probe.close();
  }
  return defaultAgent;
}

/** A browser context that looks like the Dynodoc app's window (no cookies of its own). */
async function appContext(
  browser: Browser,
  baseURL?: string,
  { token = true, bridge = true, kind = 'android' } = {}
) {
  const agent = await browserAgent(browser);
  const context = await browser.newContext({
    baseURL,
    userAgent: token ? `${agent} DynodocApp/0.1.0 (${kind})` : agent
  });
  if (bridge)
    await context.addInitScript(() => {
      (window as unknown as { __TAURI_INTERNALS__: object }).__TAURI_INTERNALS__ = {};
    });
  return context;
}

/** What the shell's script does: wait until the page says it is ready, then set the fragment. */
async function shellHands(page: Page, hash: string) {
  await page.waitForFunction(
    () => document.documentElement.getAttribute('data-finish-ready') === '1'
  );
  await page.evaluate((value) => {
    window.location.hash = value;
  }, hash);
}

const doesNotFinish = (page: Page) =>
  expect(page.getByRole('heading', { name: 'Signing in did not finish' })).toBeVisible({
    timeout: 15000
  });

// --- no API needed: what the pages and routes do for a visitor who is not signed in ------------

test.describe('without a session', () => {
  test('a request that is not valid says so and does not send anyone to sign in', async ({
    page,
    request
  }) => {
    const p = pkce();
    for (const path of [
      '/app-signin',
      `/app-signin?challenge=${p.challenge}&state=${p.state}`,
      `${requestPath(p)}&next=//evil.example`,
      requestPath(p, 'windows'),
      `/app-signin?challenge=short&state=${p.state}&platform=ios`
    ]) {
      await page.goto(path);
      await expect(page).toHaveURL(/\/app-signin(\?|$)/);
      await expect(
        page.getByRole('heading', { name: 'This sign-in link is not valid' })
      ).toBeVisible();
      await expect(page.getByRole('button', { name: 'Continue' })).toHaveCount(0);
    }
    // The confirmation page shares its address with its own site only, so that its form carries a
    // real Origin (a POST under `no-referrer` sends `Origin: null`); the finish page says nothing.
    for (const [response, policy] of [
      [await request.get('/app-signin'), 'same-origin'],
      [await request.get(requestPath(p), { maxRedirects: 0 }), 'same-origin'],
      [await request.get('/app-signin/finish'), 'no-referrer']
    ] as const) {
      expect(response.headers()['cache-control']).toBe('private, no-store');
      expect(response.headers()['referrer-policy']).toBe(policy);
      expect(response.headers()['x-frame-options']).toBeDefined();
    }
  });

  test('a valid request goes through the ordinary sign-in and comes back to the same request', async ({
    page
  }) => {
    const p = pkce();
    await page.goto(requestPath(p, 'ios'));
    await expect(page).toHaveURL(/\/login\?next=/);
    const next = new URL(page.url()).searchParams.get('next');
    expect(next).toBe(requestPath(p, 'ios'));
  });

  test('an ordinary browser sent to the finish page is told to use the app, and never acts on a fragment', async ({
    browser,
    baseURL
  }) => {
    const p = pkce();
    const hash = hashOf('ab'.repeat(32), p);
    // Three browsers that are not the app's window: no token and no bridge; the token alone; the
    // bridge alone. Each opens the finish page WITH a valid-looking triple (what a crafted link
    // carries), then has a page script change the fragment after the load (what an attacker's page
    // holding a window reference does).
    for (const [name, options] of [
      ['an ordinary browser', { token: false, bridge: false }],
      ['the token without the bridge', { token: true, bridge: false }],
      ['the bridge without the token', { token: false, bridge: true }]
    ] as const) {
      const context = await appContext(browser, baseURL, options);
      const page = await context.newPage();
      const redeems: string[] = [];
      page.on('request', (r) => {
        if (r.url().includes('/auth/app-signin/redeem')) redeems.push(r.url());
      });
      await page.goto(`/app-signin/finish#${hash}`);
      await expect(
        page.getByRole('heading', { name: 'Open this from the Dynodoc app' }),
        name
      ).toBeVisible();
      // The fragment that was there at load is removed from the address bar.
      await expect.poll(() => page.url(), name).not.toContain('verifier');
      await page.evaluate((value) => {
        window.location.hash = value;
      }, hash);
      await page.waitForTimeout(1500);
      expect(redeems, name).toEqual([]);
      await context.close();
    }
  });

  test('in the app’s window a fragment already there at load is ignored; one the shell sets later is read, a bad one fails at once', async ({
    browser,
    baseURL
  }) => {
    const p = pkce();
    const context = await appContext(browser, baseURL);
    const page = await context.newPage();
    const redeems: string[] = [];
    page.on('request', (r) => {
      if (r.url().includes('/auth/app-signin/redeem')) redeems.push(r.url());
    });
    // At load: ignored and removed, whoever put it there. Nothing is redeemed.
    await page.goto(`/app-signin/finish#${hashOf('cd'.repeat(32), p)}`);
    await expect(page.getByText('Signing you in')).toBeVisible();
    await page.waitForTimeout(1500);
    expect(redeems).toEqual([]);
    expect(page.url()).toBe(new URL('/app-signin/finish', page.url()).href);
    // With nothing arriving the page gives up after a few seconds.
    await doesNotFinish(page);
    // A fragment the shell sets that is not three valid values fails at once and redeems nothing.
    await page.goto('/app-signin/finish');
    await shellHands(page, 'code=1&state=2&verifier=3');
    await doesNotFinish(page);
    expect(redeems).toEqual([]);
    await context.close();
  });

  test('the redeem route without the app’s User-Agent is refused with the one answer', async ({
    request,
    baseURL,
    browser
  }) => {
    const p = pkce();
    const origin = new URL(baseURL!).origin;
    const json = { 'content-type': 'application/json', origin };
    const agent = await browserAgent(browser);
    const body = { code: 'ab'.repeat(32), state: p.state, verifier: p.verifier };
    const answers = new Set<string>();
    for (const headers of [
      { ...json, 'user-agent': agent },
      { ...json, 'user-agent': `${agent} DynodocApp/0.1.0 (web)` },
      { ...json, 'user-agent': `${agent} DynodocApp/0.1 (android)` },
      // With the token but no such code: the same refusal, whatever the reason.
      { ...json, 'user-agent': `${agent} DynodocApp/0.1.0 (android)` }
    ]) {
      const refused = await request.post('/auth/app-signin/redeem', { headers, data: body });
      expect(refused.status()).toBe(401);
      expect(refused.headers()['set-cookie']).toBeUndefined();
      expect(refused.headers()['cache-control']).toBe('private, no-store');
      answers.add(await refused.text());
    }
    expect(answers.size).toBe(1);
    // Another origin and a form post are refused before the token is even looked at.
    expect(
      (
        await request.post('/auth/app-signin/redeem', {
          headers: { ...json, origin: 'https://evil.example' },
          data: body
        })
      ).status()
    ).toBe(403);
    expect(
      (
        await request.post('/auth/app-signin/redeem', {
          headers: { origin, 'content-type': 'application/x-www-form-urlencoded' },
          data: 'a=b'
        })
      ).status()
    ).toBe(415);
  });

  test('asking for a code is a form of the page; the old JSON route, other origins and clients that are not browsers get nothing', async ({
    request,
    baseURL
  }) => {
    const p = pkce();
    const origin = new URL(baseURL!).origin;
    // No JSON route any more.
    const json = await request.post('/auth/app-signin', {
      headers: { origin, 'content-type': 'application/json' },
      data: { challenge: p.challenge, state: p.state, platform: 'android' }
    });
    expect(json.status()).toBeGreaterThanOrEqual(400);
    expect(await json.text()).not.toContain('dynodoc://');
    // A client that is not a browser sends no Sec-Fetch headers: refused, and told why. (SvelteKit
    // answers a form post from a client that does not ask for HTML with a JSON failure.)
    const bare = await request.post(requestPath(p), { headers: { origin }, form: { x: '1' } });
    const bareText = await bare.text();
    expect(bareText).toContain('"type":"failure"');
    expect(bareText).toContain('\\"browser\\"');
    expect(bareText).not.toContain('dynodoc:');
    // Another origin's form: refused by the framework before the page.
    const evil = await request.post(requestPath(p), {
      headers: { origin: 'https://evil.example' },
      form: { x: '1' }
    });
    expect(evil.status()).toBe(403);
  });
});

// --- the whole flow, against the isolated signed-in stack ----------------------------------------

interface Seen {
  method: string;
  url: string;
  headers: string;
  body: string;
}

/** Forwards to `target` and keeps what it saw: the request line, its headers and its body. */
function recordingProxy(
  target: string,
  listenOn = 0
): Promise<{ server: Server; seen: Seen[]; port: number }> {
  const seen: Seen[] = [];
  const upstream = new URL(target);
  const server = createServer((req: IncomingMessage, res) => {
    const chunks: Buffer[] = [];
    req.on('data', (c: Buffer) => chunks.push(c));
    req.on('end', () => {
      const body = Buffer.concat(chunks);
      seen.push({
        method: req.method ?? '',
        url: req.url ?? '',
        headers: Object.entries(req.headers)
          .map(([k, v]) => `${k}: ${v}`)
          .join('\n'),
        body: body.toString('utf8')
      });
      const out = httpRequest(
        {
          host: upstream.hostname,
          port: upstream.port,
          method: req.method,
          path: req.url,
          headers: { ...req.headers, host: upstream.host }
        },
        (back) => {
          const answer: Buffer[] = [];
          back.on('data', (c: Buffer) => answer.push(c));
          back.on('end', () => {
            const whole = Buffer.concat(answer);
            // The answer, for the checks below: kept with the request it belongs to.
            seen[seen.length - 1].body += `\n--response--\n${back.headers['content-type'] ?? ''}\n${
              /text|json|javascript/.test(String(back.headers['content-type']))
                ? whole.toString('utf8')
                : ''
            }\n--location--\n${back.headers.location ?? ''}`;
            res.writeHead(back.statusCode ?? 502, back.headers);
            res.end(whole);
          });
        }
      );
      out.on('error', () => {
        res.writeHead(502);
        res.end();
      });
      out.end(body);
    });
  });
  return new Promise((resolve) =>
    server.listen(listenOn, '127.0.0.1', () => {
      const address = server.address();
      resolve({ server, seen, port: typeof address === 'object' && address ? address.port : 0 });
    })
  );
}

async function freePort(): Promise<number> {
  return new Promise((resolve) => {
    const probe = createServer();
    probe.listen(0, '127.0.0.1', () => {
      const address = probe.address();
      const port = typeof address === 'object' && address ? address.port : 0;
      probe.close(() => resolve(port));
    });
  });
}

test.describe('the whole flow', () => {
  test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in stack.');

  let origin = '';
  let webProxy: { server: Server; seen: Seen[]; port: number };
  let apiProxy: { server: Server; seen: Seen[]; port: number };
  let web: ChildProcess;

  test.beforeAll(async () => {
    const api = process.env.ENGINE_API_URL ?? 'http://127.0.0.1:8080';
    apiProxy = await recordingProxy(api);
    const inner = await freePort();
    const outer = await freePort();
    origin = `http://127.0.0.1:${outer}`;
    // The web server the browsers talk to is one this file starts, behind two recorders: one in
    // front of it (what a browser sends it, on the port ORIGIN names) and one between it and the
    // API (what it sends on). It is the same build, with the same secrets, as the stack the other
    // specs use.
    web = spawn(process.execPath, ['build'], {
      cwd: fileURLToPath(new URL('..', import.meta.url)),
      env: {
        ...process.env,
        HOST: '127.0.0.1',
        PORT: String(inner),
        ORIGIN: origin,
        ENGINE_API_URL: `http://127.0.0.1:${apiProxy.port}`
      },
      stdio: 'ignore'
    });
    webProxy = await recordingProxy(`http://127.0.0.1:${inner}`, outer);
    for (let i = 0; i < 60; i += 1) {
      try {
        const answer = await fetch(`${origin}/healthz`);
        if (answer.ok) return;
      } catch {
        /* not up yet */
      }
      await new Promise((r) => setTimeout(r, 500));
    }
    throw new Error('the web server for this spec did not start');
  });

  test.afterAll(async () => {
    web?.kill();
    webProxy?.server.close();
    apiProxy?.server.close();
  });

  /** Sign in with the development link and wait for `land`. */
  async function signIn(page: Page, email: string, land: RegExp) {
    await page.goto(`${origin}/login`);
    await page.getByLabel('Your name').fill('App tester');
    await page.getByLabel('Work email').fill(email);
    await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
    await page.getByRole('button', { name: /Continue in local development/ }).click();
    await expect(page).toHaveURL(land);
  }

  /** Watches the page for the navigation to dynodoc://, which a browser cannot show. */
  async function watchForApp(page: Page) {
    const client = await page.context().newCDPSession(page);
    await client.send('Page.enable');
    const links: string[] = [];
    client.on('Page.frameRequestedNavigation', (event) => {
      if (event.url.startsWith('dynodoc:')) links.push(event.url);
    });
    return links;
  }

  /**
   * The browser half for one attempt: open the request, check the pairing code, press Continue
   * (a real click on a real form), return the code of the app link.
   */
  async function carryOn(page: Page, p: Pkce, platform = 'android') {
    const links = await watchForApp(page);
    await page.goto(`${origin}${requestPath(p, platform)}`);
    await expect(page.getByTestId('pairing')).toContainText(pairing(p.state));
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect(page.getByRole('link', { name: 'Open the Dynodoc app' })).toBeVisible();
    await expect.poll(() => links.length).toBeGreaterThan(0);
    const url = new URL(links[0]);
    expect(url.protocol).toBe('dynodoc:');
    expect(url.host).toBe('signin');
    expect([...url.searchParams.keys()]).toEqual(['code', 'state']);
    expect(url.searchParams.get('state')).toBe(p.state);
    const code = url.searchParams.get('code')!;
    expect(code).toMatch(/^[0-9a-f]{64}$/);
    // The button offers the very same link.
    await expect(page.getByRole('link', { name: 'Open the Dynodoc app' })).toHaveAttribute(
      'href',
      links[0]
    );
    return code;
  }

  /** The app's window, with no cookies of its own, opened on the plain finish page. */
  async function appWindow(browser: Browser, options?: { kind?: string }) {
    const context = await appContext(browser, origin, options);
    const page = await context.newPage();
    await page.goto('/app-signin/finish');
    return { context, page };
  }

  const sessionCookie = (context: BrowserContext) =>
    context.cookies(origin).then((all) => all.find((c) => c.name === 'dynodoc_session'));
  const identityOf = async (context: BrowserContext) =>
    (await (await context.request.get(`${origin}/auth/session`)).json()).identity_id;
  const redeems = (list: Seen[]) => list.filter((r) => r.url.endsWith('/auth/app-signin/redeem'));
  const issues = (list: Seen[]) => list.filter((r) => r.url.endsWith('/auth/app-handover'));

  test('sign in in the browser, press Continue, finish in the app window: signed in, with no secret anywhere a server could see', async ({
    browser
  }) => {
    const email = `app-${Date.now()}@example.test`;
    const p = pkce();
    const browserContext = await browser.newContext();
    const page = await browserContext.newPage();

    // 1. The app opened the browser; the person is not signed in there, so the ordinary
    //    sign-in comes first and returns to the same request.
    await page.goto(`${origin}${requestPath(p)}`);
    await expect(page).toHaveURL(/\/login\?next=/);
    await page.getByLabel('Your name').fill('App tester');
    await page.getByLabel('Work email').fill(email);
    await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
    const links = await watchForApp(page);
    await page.getByRole('button', { name: /Continue in local development/ }).click();
    await expect(page).toHaveURL(`${origin}${requestPath(p)}`);

    // 2. The confirmation page: who, which device, the pairing code, Continue and Cancel. Nothing is
    //    made yet.
    await expect(page.getByRole('heading', { name: 'Sign in to the Dynodoc app' })).toBeVisible();
    await expect(page.getByText(email)).toBeVisible();
    await expect(page.getByText('Android phone')).toBeVisible();
    await expect(page.getByTestId('pairing')).toContainText(pairing(p.state));
    await expect(page.getByRole('button', { name: 'Cancel' })).toBeVisible();
    expect(issues(apiProxy.seen)).toEqual([]);

    // 3. Continue (a real click on the form) makes the code and the browser goes to dynodoc://signin.
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect.poll(() => links.length).toBeGreaterThan(0);
    const link = new URL(links[0]);
    expect(link.host).toBe('signin');
    const code = link.searchParams.get('code')!;
    expect(code).toMatch(/^[0-9a-f]{64}$/);
    expect(link.searchParams.get('state')).toBe(p.state);

    // 4. The app window (no cookies) opens the PLAIN finish page, and the shell then sets the
    //    fragment once the page says it is ready.
    const app = await appWindow(browser);
    await expect(app.page.getByText('Signing you in')).toBeVisible();
    await shellHands(app.page, hashOf(code, p));
    await expect(app.page).toHaveURL(`${origin}/workspace`);
    expect(app.page.url()).not.toContain('verifier');
    const cookie = await sessionCookie(app.context);
    expect(cookie?.httpOnly).toBe(true);
    expect(cookie?.sameSite).toBe('Lax');
    const session = await (await app.context.request.get(`${origin}/auth/session`)).json();
    expect(session.authenticated).toBe(true);
    const browserSession = await (
      await browserContext.request.get(`${origin}/auth/session`)
    ).json();
    expect(session.identity_id).toBe(browserSession.identity_id);

    // 5. What the servers saw. The verifier is only ever in the body of the redeem request, and
    //    the code is only in that body, in the answer to the form that made it and in the API's
    //    answer to the web server. Neither is in any address, any header (Referer included) or any
    //    page.
    const secrets = [p.verifier, code];
    for (const [name, list] of [
      ['web server', webProxy.seen],
      ['API', apiProxy.seen]
    ] as const) {
      for (const r of list) {
        const [body, answer = ''] = r.body.split('\n--response--\n');
        const whole = `${r.method} ${r.url}\n${r.headers}`;
        for (const secret of secrets) {
          expect(whole, `${name}: ${r.method} ${r.url.slice(0, 60)}`).not.toContain(secret);
        }
        const redeeming = r.url.endsWith('/redeem');
        if (!redeeming) {
          expect(body, `${name} body of ${r.url}`).not.toContain(p.verifier);
          expect(body, `${name} body of ${r.url}`).not.toContain(code);
        }
        expect(answer, `${name} answer to ${r.url}`).not.toContain(p.verifier);
        const makesTheLink =
          (name === 'web server' && r.method === 'POST' && r.url.startsWith('/app-signin')) ||
          (name === 'API' && /\/app-handover$/.test(r.url));
        if (!makesTheLink) {
          expect(answer, `${name} answer to ${r.url}`).not.toContain(code);
        }
      }
    }
    expect(redeems(webProxy.seen).filter((r) => r.body.includes(code)).length).toBe(1);
    expect(
      apiProxy.seen.filter(
        (r) => r.url.endsWith('/auth/app-handover/redeem') && r.body.includes(code)
      ).length
    ).toBe(1);
    // The finish page was requested plain, with no fragment to send, and the web server told the API
    // what kind of app asked.
    const finishPage = webProxy.seen.filter((r) => r.url === '/app-signin/finish');
    expect(finishPage.length).toBeGreaterThan(0);
    expect(webProxy.seen.every((r) => !r.url.includes('#'))).toBe(true);
    const toApi = apiProxy.seen.find(
      (r) => r.url.endsWith('/auth/app-handover/redeem') && r.body.includes(code)
    );
    expect(toApi?.body).toContain('"app":"android"');
    await app.context.close();
    await browserContext.close();
  });

  test('a replayed finish link, a wrong verifier and a wrong state all end the same way', async ({
    browser
  }) => {
    const browserContext = await browser.newContext();
    const page = await browserContext.newPage();
    await signIn(page, `replay-${Date.now()}@example.test`, /\/projects$/);

    const refused = async (hash: string) => {
      const app = await appWindow(browser);
      await shellHands(app.page, hash);
      await expect(
        app.page.getByRole('heading', { name: 'Signing in did not finish' })
      ).toBeVisible();
      await expect(app.page.getByText(/may have expired or already been used/)).toBeVisible();
      expect(app.page.url()).toBe(`${origin}/app-signin/finish`);
      expect(await sessionCookie(app.context)).toBeUndefined();
      const text = await app.page.locator('main').innerText();
      await app.context.close();
      return text;
    };

    // A good attempt, then its replay.
    const a = pkce();
    const codeA = await carryOn(page, a);
    const done = await appWindow(browser);
    await shellHands(done.page, hashOf(codeA, a));
    await expect(done.page).toHaveURL(`${origin}/workspace`);
    await done.context.close();
    const replay = await refused(hashOf(codeA, a));

    // Wrong verifier: refused, and it uses the code up, so the right one no longer works.
    const b = pkce();
    const codeB = await carryOn(page, b);
    const wrongVerifier = await refused(hashOf(codeB, b, pkce().verifier));
    const afterWrong = await refused(hashOf(codeB, b));

    // Wrong state, same.
    const c = pkce();
    const codeC = await carryOn(page, c);
    const wrongState = await refused(hashOf(codeC, c, c.verifier, pkce().state));
    const afterState = await refused(hashOf(codeC, c));

    // A code that was never issued.
    const never = await refused(hashOf('0'.repeat(64), c));

    // The person is told nothing about WHICH check failed.
    expect(new Set([replay, wrongVerifier, afterWrong, wrongState, afterState, never]).size).toBe(
      1
    );
    await browserContext.close();
  });

  test('a second try in the window that still shows the failure works, and no secret stays in the address', async ({
    browser
  }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await signIn(page, `again-${Date.now()}@example.test`, /\/projects$/);
    const a = pkce();
    const codeA = await carryOn(page, a);
    const b = pkce();
    const codeB = await carryOn(page, b);

    const app = await appWindow(browser);
    // First try: a wrong verifier. The page says it did not finish, and the address is clean.
    await shellHands(app.page, hashOf(codeA, a, pkce().verifier));
    await expect(
      app.page.getByRole('heading', { name: 'Signing in did not finish' })
    ).toBeVisible();
    expect(app.page.url()).toBe(`${origin}/app-signin/finish`);
    // Second try, in the same window: only the fragment changes, which is no page load.
    await shellHands(app.page, hashOf(codeB, b));
    await expect(app.page).toHaveURL(`${origin}/workspace`);
    expect(await sessionCookie(app.context)).toBeDefined();
    await app.context.close();
    await context.close();
  });

  test('a code gives its own account, even in a window that is signed in as someone else', async ({
    browser
  }) => {
    const first = await browser.newContext();
    const pageA = await first.newPage();
    await signIn(pageA, `owner-${Date.now()}@example.test`, /\/projects$/);
    const a = pkce();
    const code = await carryOn(pageA, a);
    const idA = await identityOf(first);

    // The app's window already holds a session of another person.
    const app = await appWindow(browser);
    await signIn(app.page, `other-${Date.now()}@example.test`, /\/projects$/);
    const idB = await identityOf(app.context);
    expect(idB).not.toBe(idA);
    await app.page.goto('/app-signin/finish');
    await shellHands(app.page, hashOf(code, a));
    await expect(app.page).toHaveURL(`${origin}/workspace`);
    expect(await identityOf(app.context)).toBe(idA);
    await app.context.close();
    await first.close();
  });

  test('Cancel makes no code and tells the person nothing was shared', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await signIn(page, `cancel-${Date.now()}@example.test`, /\/projects$/);
    const before = webProxy.seen.length;
    const issued = issues(apiProxy.seen).length;
    const links = await watchForApp(page);
    await page.goto(`${origin}${requestPath(pkce(), 'desktop')}`);
    await expect(page.getByText('computer')).toBeVisible();
    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(page.getByRole('heading', { name: 'Cancelled' })).toBeVisible();
    expect(links).toEqual([]);
    expect(webProxy.seen.slice(before).some((r) => r.method === 'POST')).toBe(false);
    expect(issues(apiProxy.seen).length).toBe(issued);
    await context.close();
  });

  test('an account holds at most three unused codes, and says so plainly', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await signIn(page, `limit-${Date.now()}@example.test`, /\/projects$/);
    for (let i = 0; i < 3; i += 1) await carryOn(page, pkce());
    await page.goto(`${origin}${requestPath(pkce())}`);
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect(page.getByRole('alert')).toContainText('Too many sign-in attempts');
    await context.close();
  });

  test('no script and no other client can make a code; the real click on the form can', async ({
    browser
  }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await signIn(page, `scripted-${Date.now()}@example.test`, /\/projects$/);
    const p = pkce();
    const path = requestPath(p, 'ios');
    await page.goto(`${origin}${path}`);
    const before = issues(apiProxy.seen).length;
    const noCode = async (what: string) => {
      expect(issues(apiProxy.seen).length, what).toBe(before);
    };

    // What a refusal looks like to a client that does not ask for HTML: SvelteKit's JSON failure.
    const refused = (text: string, problem: string, what: string) => {
      expect(text, what).toContain('"type":"failure"');
      expect(text, what).toContain(`\\"${problem}\\"`);
      expect(text, what).not.toContain('dynodoc:');
    };

    // 1. A script's fetch to the form's address, with every header it is allowed to set (the
    //    browser overrides the Sec-Fetch ones it is not: they say cors / empty, with no user).
    const viaFetch = await page.evaluate(async (target) => {
      const response = await fetch(target, {
        method: 'POST',
        headers: {
          'content-type': 'application/x-www-form-urlencoded',
          'sec-fetch-site': 'same-origin',
          'sec-fetch-mode': 'navigate',
          'sec-fetch-user': '?1',
          'sec-fetch-dest': 'document'
        },
        body: 'x=1'
      });
      return await response.text();
    }, path);
    refused(viaFetch, 'blocked', 'fetch');
    await noCode('fetch');

    // 2. A script's XMLHttpRequest.
    const viaXhr = await page.evaluate(
      (target) =>
        new Promise<string>((resolve) => {
          const xhr = new XMLHttpRequest();
          xhr.open('POST', target);
          xhr.setRequestHeader('content-type', 'application/x-www-form-urlencoded');
          xhr.onload = () => resolve(xhr.responseText);
          xhr.send('x=1');
        }),
      path
    );
    refused(viaXhr, 'blocked', 'xhr');
    await noCode('xhr');

    // 3. A form a script submits without any click: a real top-level navigation, but with no user
    //    activation, so the browser sends no Sec-Fetch-User. Every call Playwright makes into a page
    //    (evaluate, and the polling of an expect) counts as a user gesture whose activation lasts
    //    about five seconds, so the script waits it out and nothing touches the page meanwhile.
    await page.evaluate(() => {
      setTimeout(() => {
        const form = document.createElement('form');
        form.method = 'POST';
        document.body.appendChild(form);
        form.submit();
      }, 6500);
    });
    await page.waitForTimeout(9000);
    await expect(page.getByRole('alert')).toContainText(
      'did not come from the button on this page',
      {
        timeout: 15000
      }
    );
    await noCode('a form submitted by a script');

    // 4. Another client altogether, with the signed-in cookie (a copied one) and a made-up Origin.
    const viaClient = await context.request.post(`${origin}${path}`, {
      headers: { origin },
      form: { x: '1' }
    });
    refused(await viaClient.text(), 'browser', 'another client');
    await noCode('another client');

    // 5. The real thing: a click on Continue.
    await page.goto(`${origin}${path}`);
    const links = await watchForApp(page);
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect.poll(() => links.length).toBeGreaterThan(0);
    expect(issues(apiProxy.seen).length).toBe(before + 1);
    await context.close();
  });

  test('a crafted finish link cannot sign a visitor’s browser in as someone else, with or without a later change of the fragment', async ({
    browser
  }) => {
    // The attacker's own account and a valid code and verifier for it.
    const attacker = await browser.newContext();
    const attackerPage = await attacker.newPage();
    await signIn(attackerPage, `attacker-${Date.now()}@example.test`, /\/projects$/);
    const idAttacker = await identityOf(attacker);
    const victimShapes = [
      ['an ordinary browser', { token: false, bridge: false }],
      ['the token without the bridge', { token: true, bridge: false }],
      ['the bridge without the token', { token: false, bridge: true }]
    ] as const;
    for (const [name, options] of victimShapes) {
      const p = pkce();
      const code = await carryOn(attackerPage, p);
      // The victim: a browser that is already signed in as somebody else.
      const victim = await appContext(browser, origin, options);
      const victimPage = await victim.newPage();
      await signIn(
        victimPage,
        `victim-${Date.now()}-${Math.random().toString(16).slice(2)}@example.test`,
        /\/projects$/
      );
      const idVictim = await identityOf(victim);
      expect(idVictim, name).not.toBe(idAttacker);
      const cookieBefore = await sessionCookie(victim);
      const redeemsBefore = redeems(webProxy.seen).length;

      // (a) the crafted link, with the triple in the fragment; then (b) a page script that holds a
      // window reference changing the fragment after the load.
      await victimPage.goto(`${origin}/app-signin/finish#${hashOf(code, p)}`);
      await victimPage.evaluate(
        (value) => {
          window.location.hash = value;
        },
        hashOf(code, p)
      );
      await victimPage.waitForTimeout(1500);
      expect(await sessionCookie(victim), name).toEqual(cookieBefore);
      expect(await identityOf(victim), name).toBe(idVictim);
      expect(redeems(webProxy.seen).length, name).toBe(redeemsBefore);
      await victim.close();
      // The code is still good: it was never asked for, so it can be redeemed by the real app.
      const app = await appWindow(browser);
      await shellHands(app.page, hashOf(code, p));
      await expect(app.page).toHaveURL(`${origin}/workspace`);
      expect(await identityOf(app.context)).toBe(idAttacker);
      await app.context.close();
    }
    await attacker.close();
  });
});
