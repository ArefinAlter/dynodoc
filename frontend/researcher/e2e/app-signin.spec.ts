import { expect, test, type BrowserContext, type Page } from '@playwright/test';
import { spawn, type ChildProcess } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { createServer, request as httpRequest, type IncomingMessage, type Server } from 'node:http';
import { fileURLToPath } from 'node:url';

// Signing a Dynodoc app (desktop or phone) in from the system browser: docs/decisions/017.
// The page and the routes in front of the API are exercised in Chromium. The app's own half
// (the shell that opens the browser, receives dynodoc://signin and opens /app-signin/finish) is
// played here by a second browser context that has no cookies, as the app's window has none.

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
const fragment = (code: string, p: Pkce, verifier = p.verifier, state = p.state) =>
  `/app-signin/finish#code=${code}&state=${state}&verifier=${verifier}`;

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
    for (const response of [
      await request.get('/app-signin'),
      await request.get(requestPath(p), { maxRedirects: 0 }),
      await request.get('/app-signin/finish')
    ]) {
      expect(response.headers()['cache-control']).toBe('private, no-store');
      expect(response.headers()['referrer-policy']).toBe('no-referrer');
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

  test('the finish page without its fragment, or with a bad one, does not finish', async ({
    page
  }) => {
    const seen: string[] = [];
    page.on('request', (r) => seen.push(r.url()));
    for (const hash of ['', '#code=1&state=2&verifier=3', `#code=${'a'.repeat(64)}`]) {
      await page.goto(`/app-signin/finish${hash}`);
      await expect(page.getByRole('heading', { name: 'Signing in did not finish' })).toBeVisible();
      // The fragment is removed from the address at once, valid or not.
      expect(page.url()).toBe(new URL('/app-signin/finish', page.url()).href);
    }
    expect(seen.some((url) => url.includes('/auth/app-signin/redeem'))).toBe(false);
  });

  test('the routes refuse another origin and a form post, and a refused redeem is uniform', async ({
    request,
    baseURL
  }) => {
    const p = pkce();
    const origin = new URL(baseURL!).origin;
    const json = { 'content-type': 'application/json' };
    const issue = { challenge: p.challenge, state: p.state, platform: 'ios' };
    expect(
      (
        await request.post('/auth/app-signin', {
          headers: { ...json, origin: 'https://evil.example' },
          data: issue
        })
      ).status()
    ).toBe(403);
    expect((await request.post('/auth/app-signin', { headers: json, data: issue })).status()).toBe(
      403
    );
    expect(
      (
        await request.post('/auth/app-signin', {
          headers: { origin, 'content-type': 'application/x-www-form-urlencoded' },
          data: 'challenge=x'
        })
      ).status()
    ).toBe(415);
    // Not signed in: no code.
    const unsigned = await request.post('/auth/app-signin', {
      headers: { ...json, origin },
      data: issue
    });
    expect(unsigned.status()).toBe(401);
    const bodies = new Set<string>();
    for (const data of [
      {},
      { code: 'x', state: p.state, verifier: p.verifier },
      { code: 'ab'.repeat(32), state: p.state, verifier: p.verifier },
      { code: 'ab'.repeat(32), state: p.state, verifier: p.verifier, extra: true }
    ]) {
      const refused = await request.post('/auth/app-signin/redeem', {
        headers: { ...json, origin },
        data
      });
      expect(refused.status()).toBe(401);
      expect(refused.headers()['set-cookie']).toBeUndefined();
      expect(refused.headers()['cache-control']).toBe('private, no-store');
      bodies.add(await refused.text());
    }
    expect(bodies.size).toBe(1);
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

  /** The browser half for one attempt: open the request, press Continue, return the app link. */
  async function carryOn(page: Page, p: Pkce, platform = 'android') {
    const links = await watchForApp(page);
    await page.goto(`${origin}${requestPath(p, platform)}`);
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

  /** The app's window: a fresh browser context with no cookies, opened on the finish page. */
  async function appWindow(browser: import('@playwright/test').Browser) {
    const context = await browser.newContext();
    return { context, page: await context.newPage() };
  }

  const sessionCookie = (context: BrowserContext) =>
    context.cookies(origin).then((all) => all.find((c) => c.name === 'dynodoc_session'));

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

    // 2. The confirmation page: who, which device, Continue and Cancel. Nothing is made yet.
    await expect(page.getByRole('heading', { name: 'Sign in to the Dynodoc app' })).toBeVisible();
    await expect(page.getByText(email)).toBeVisible();
    await expect(page.getByText('Android phone')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Cancel' })).toBeVisible();
    expect(webProxy.seen.some((r) => r.url.startsWith('/auth/app-signin'))).toBe(false);
    expect(apiProxy.seen.some((r) => r.url.includes('/auth/app-handover'))).toBe(false);

    // 3. Continue makes the code and the browser goes to dynodoc://signin.
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect.poll(() => links.length).toBeGreaterThan(0);
    const link = new URL(links[0]);
    expect(link.host).toBe('signin');
    const code = link.searchParams.get('code')!;
    expect(code).toMatch(/^[0-9a-f]{64}$/);
    expect(link.searchParams.get('state')).toBe(p.state);

    // 4. The app window (no cookies) opens the finish page with the fragment and is signed in.
    const app = await appWindow(browser);
    await app.page.goto(`${origin}${fragment(code, p)}`);
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
    //    the code is only in that body and in the answer to the request that made it. Neither is
    //    in any address, any header (Referer included) or any page.
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
        if (
          !(name === 'web server' && r.url === '/auth/app-signin') &&
          !/\/app-handover$/.test(r.url)
        ) {
          expect(answer, `${name} answer to ${r.url}`).not.toContain(code);
        }
      }
    }
    expect(webProxy.seen.filter((r) => r.url.endsWith('/auth/app-signin/redeem')).length).toBe(1);
    expect(apiProxy.seen.filter((r) => r.url.endsWith('/auth/app-handover/redeem')).length).toBe(1);
    // The finish page's own HTML is the same for every visitor.
    const finishPage = webProxy.seen.find((r) => r.url === '/app-signin/finish');
    expect(finishPage).toBeTruthy();
    // The one request for it carried no fragment.
    expect(webProxy.seen.every((r) => !r.url.includes('#'))).toBe(true);
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
      await app.page.goto(`${origin}${hash}`);
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
    await done.page.goto(`${origin}${fragment(codeA, a)}`);
    await expect(done.page).toHaveURL(`${origin}/workspace`);
    await done.context.close();
    const replay = await refused(fragment(codeA, a));

    // Wrong verifier: refused, and it uses the code up, so the right one no longer works.
    const b = pkce();
    const codeB = await carryOn(page, b);
    const wrongVerifier = await refused(fragment(codeB, b, pkce().verifier));
    const afterWrong = await refused(fragment(codeB, b));

    // Wrong state, same.
    const c = pkce();
    const codeC = await carryOn(page, c);
    const wrongState = await refused(fragment(codeC, c, c.verifier, pkce().state));
    const afterState = await refused(fragment(codeC, c));

    // A code that was never issued.
    const never = await refused(fragment('0'.repeat(64), c));

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
    await app.page.goto(`${origin}${fragment(codeA, a, pkce().verifier)}`);
    await expect(
      app.page.getByRole('heading', { name: 'Signing in did not finish' })
    ).toBeVisible();
    expect(app.page.url()).toBe(`${origin}/app-signin/finish`);
    // Second try, in the same window: only the fragment changes, which is no page load.
    await app.page.goto(`${origin}${fragment(codeB, b)}`);
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
    const idA = (await (await first.request.get(`${origin}/auth/session`)).json()).identity_id;

    // The app's window already holds a session of another person.
    const app = await appWindow(browser);
    await signIn(await app.context.newPage(), `other-${Date.now()}@example.test`, /\/projects$/);
    const idB = (await (await app.context.request.get(`${origin}/auth/session`)).json())
      .identity_id;
    expect(idB).not.toBe(idA);
    const window = await app.context.newPage();
    await window.goto(`${origin}${fragment(code, a)}`);
    await expect(window).toHaveURL(`${origin}/workspace`);
    const now = (await (await app.context.request.get(`${origin}/auth/session`)).json())
      .identity_id;
    expect(now).toBe(idA);
    await app.context.close();
    await first.close();
  });

  test('Cancel makes no code and tells the person nothing was shared', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await signIn(page, `cancel-${Date.now()}@example.test`, /\/projects$/);
    const before = webProxy.seen.length;
    const links = await watchForApp(page);
    await page.goto(`${origin}${requestPath(pkce(), 'desktop')}`);
    await expect(page.getByText('computer')).toBeVisible();
    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(page.getByRole('heading', { name: 'Cancelled' })).toBeVisible();
    expect(links).toEqual([]);
    expect(
      webProxy.seen.slice(before).some((r) => r.method === 'POST' && r.url.startsWith('/auth/'))
    ).toBe(false);
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

  test('a request from another site cannot make a code, signed in or not', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await signIn(page, `csrf-${Date.now()}@example.test`, /\/projects$/);
    const p = pkce();
    const data = { challenge: p.challenge, state: p.state, platform: 'ios' };
    const attempt = (headers: Record<string, string>, body: unknown = data) =>
      context.request.post(`${origin}/auth/app-signin`, { headers, data: body });
    expect((await attempt({ origin: 'https://evil.example' })).status()).toBe(403);
    expect((await attempt({ origin: 'null' })).status()).toBe(403);
    expect((await attempt({})).status()).toBe(403);
    expect(
      (await attempt({ origin, 'content-type': 'text/plain' }, JSON.stringify(data))).status()
    ).toBe(415);
    // The same request from the page's own origin works (the signed-in cookie rides along).
    expect((await attempt({ origin, 'content-type': 'application/json' })).status()).toBe(200);
    await context.close();
  });
});
