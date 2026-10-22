// The browser half of the bundle's smoke test (Chromium through Playwright), driven by smoke.mjs.
// Everything goes through the bundled proxy over HTTPS with the evaluation certificate (so
// certificate errors are ignored on purpose). Email sign-in end to end: the sign-in link is
// requested in the page and read from the test mail sink.

/** Sign in by email, create a document, see it listed. Returns what later steps need. */
export async function signInAndCreate({ playwright, baseUrl, email, title, readMail, step }) {
  const browser = await playwright.chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, baseURL: baseUrl });
  const page = await context.newPage();
  const problems = [];
  page.on('pageerror', (error) => problems.push(`page error: ${error.message}`));

  await page.goto('/login');
  await page.getByLabel('Your name').fill('Smoke tester');
  await page.getByLabel('Work email').fill(email);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page
    .getByText('Check your inbox. Your sign-in link is valid for 15 minutes.')
    .waitFor({ timeout: 20000 });
  step('the page says a sign-in link was sent');

  let mail = null;
  for (let i = 0; i < 40 && !mail; i++) {
    mail = (await readMail()).find((m) => m.to.includes(email)) ?? null;
    if (!mail) await new Promise((r) => setTimeout(r, 500));
  }
  if (!mail) throw new Error(`no sign-in mail for ${email} reached the sink`);
  // The message is quoted-printable: undo the soft line breaks and =XX escapes before looking.
  const decoded = mail.data.replace(/=\r?\n/g, '').replace(/=([0-9A-F]{2})/g, (_, h) => String.fromCharCode(parseInt(h, 16)));
  const link = /https?:\/\/[^\s"<>]+\/auth\/verify\?token=[A-Za-z0-9_\-.%]+/.exec(decoded)?.[0];
  if (!link) throw new Error('the sign-in mail has no link');
  step(`the mail arrived at the sink, link on ${new URL(link).origin}`, { link: link.replace(/token=.*/, 'token=...') });
  if (new URL(link).origin !== new URL(baseUrl).origin)
    throw new Error(`the link's origin ${new URL(link).origin} is not ${new URL(baseUrl).origin}`);

  await page.goto(link);
  await page.getByText('Your secure sign-in link is ready.').waitFor({ timeout: 15000 });
  await page.getByRole('button', { name: 'Continue to workspace' }).click();
  await page.waitForURL(/\/projects$/, { timeout: 20000 });
  step('signed in; the workspace opened');

  await page.goto('/workspace');
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  await page.getByLabel('Document name', { exact: true }).fill(title);
  await page.getByRole('button', { name: 'Create document', exact: true }).click();
  await page.getByLabel('Document title').waitFor({ timeout: 20000 });
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await editor.click();
  await page.keyboard.type('Smoke test paragraph that must survive restarts.');
  await page.locator('.save-state').filter({ hasText: /All changes saved|Personal draft saved/ }).waitFor({ timeout: 20000 });
  step('a document was created and saved');

  await page.goto('/workspace');
  await page.getByText(title).first().waitFor({ timeout: 20000 });
  step('the document is listed in the workspace');
  return { browser, context, page, problems };
}

/** The document is still listed (and its text still there) after something was restarted. */
export async function documentStillThere({ page, title, text }) {
  await page.goto('/workspace');
  await page.getByText(title).first().waitFor({ timeout: 30000 });
  await page.getByText(title).first().click();
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await editor.waitFor({ timeout: 20000 });
  await page.getByText(text).first().waitFor({ timeout: 20000 });
}

/**
 * The app sign-in confirmation step as a browser sees it THROUGH THE EDGE (docs/decisions/017):
 * the proxy in front of the web server replaces the application's Referrer-Policy with
 * `no-referrer`, which makes a browser send `Origin: null` with a same-origin form post, so the
 * confirmation form only works because the page carries its own `<meta name="referrer">`. Also
 * checked here, because only a real edge can show them: the headers that keep the two sign-in pages
 * from being framed or sharing a window group arrive, a script on an ordinary page cannot read the
 * code a form post into a popup returns, and the proxy limits POSTs to the form (outside /auth/).
 *
 * `page` is a signed-in page of `context` with `baseURL` set. Returns results for the caller's own
 * `check(id, label, ok, detail)`; nothing here throws for a failed expectation.
 */
export async function appSigninThroughEdge({ page, context, baseUrl }) {
  const { createHash, randomBytes } = await import('node:crypto');
  const results = [];
  const note = (id, label, ok, detail = '') => results.push({ id, label, ok: Boolean(ok), detail: ok ? '' : String(detail).slice(0, 900) });
  const origin = new URL(baseUrl).origin;
  const b64 = (bytes) => bytes.toString('base64url');
  const request = () => {
    const verifier = b64(randomBytes(32));
    return `/app-signin?challenge=${b64(createHash('sha256').update(verifier).digest())}&state=${b64(randomBytes(24))}&platform=desktop`;
  };

  // 1. headers on both pages, through the edge
  const confirmation = await page.goto(request());
  const finish = await page.request.get('/app-signin/finish');
  const framing = (r) => {
    const h = r.headers();
    return h['x-frame-options'] === 'DENY' && /frame-ancestors 'none'/.test(h['content-security-policy'] ?? '') && h['cross-origin-opener-policy'] === 'same-origin';
  };
  const login = await page.request.get('/login');
  const brief = (r) => ({ xfo: r.headers()['x-frame-options'], ancestors: /frame-ancestors [^;]*/.exec(r.headers()['content-security-policy'] ?? '')?.[0], coop: r.headers()['cross-origin-opener-policy'] });
  note('h3', 'through the proxy the confirmation page and the finish page are never framed and never share a window group (X-Frame-Options DENY, frame-ancestors none, COOP same-origin), while other pages keep SAMEORIGIN', framing(confirmation) && framing(finish) && login.headers()['x-frame-options'] === 'SAMEORIGIN' && !login.headers()['cross-origin-opener-policy'], JSON.stringify({ confirmation: brief(confirmation), finish: brief(finish), login: brief(login) }));

  // 2. Continue works: the form posts its real Origin although the proxy says no-referrer
  const referrerHeader = confirmation.headers()['referrer-policy'];
  const meta = await page.locator('meta[name="referrer"]').getAttribute('content');
  const client = await context.newCDPSession(page);
  await client.send('Page.enable');
  const links = [];
  client.on('Page.frameRequestedNavigation', (event) => { if (event.url.startsWith('dynodoc:')) links.push(event.url); });
  const posted = page.waitForRequest((r) => r.method() === 'POST' && r.url().includes('/app-signin?'), { timeout: 20000 });
  const answer = page.waitForResponse((r) => r.request().method() === 'POST' && r.url().includes('/app-signin?'), { timeout: 20000 });
  await page.getByRole('button', { name: 'Continue' }).click();
  let sentOrigin = '(no request)';
  let status = 0;
  try { sentOrigin = (await posted).headers().origin ?? '(none)'; status = (await answer).status(); } catch { /* reported below */ }
  const offered = await page.getByRole('link', { name: 'Open the Dynodoc app' }).isVisible({ timeout: 10000 }).catch(() => false);
  await page.waitForTimeout(500);
  note('h4', `Continue works through the proxy (the edge says Referrer-Policy: ${referrerHeader}; the page's own meta says ${meta}): the form posts its real Origin and the answer holds the app link`,
    referrerHeader === 'no-referrer' && meta === 'same-origin' && sentOrigin === origin && status === 200 && offered && links.some((l) => /^dynodoc:\/\/signin\?code=[0-9a-f]{64}&state=/.test(l)),
    JSON.stringify({ referrerHeader, meta, sentOrigin, origin, status, offered, links: links.map((l) => l.slice(0, 30)) }));

  // 3. a script on an ordinary page cannot read what a form post into a popup returns
  await page.goto('/workspace');
  await page.evaluate((target) => {
    const state = window;
    let popup = null;
    let taps = 0;
    state.__read = 'nothing read yet';
    // The attacker's script sets its own page's referrer policy, as the confirmation page does, so
    // that the form post carries a real Origin even behind a proxy that says no-referrer.
    const meta = document.createElement('meta');
    meta.name = 'referrer';
    meta.content = 'same-origin';
    document.head.appendChild(meta);
    document.addEventListener('pointerdown', () => {
      taps += 1;
      if (taps === 1) popup = window.open('', 'pop');
      if (taps === 2) {
        const form = document.createElement('form');
        form.method = 'POST';
        form.action = target;
        form.target = 'pop';
        document.body.appendChild(form);
        form.submit();
        setTimeout(() => {
          try { state.__read = popup.document.querySelector('a[href^="dynodoc:"]')?.getAttribute('href') ?? 'popup readable, no link'; }
          catch (error) { state.__read = `BLOCKED ${String(error).slice(0, 40)}`; }
        }, 1500);
      }
    });
  }, request());
  // The answer arrives in the popup, a page of its own, so it is watched on the whole context.
  const minted = context.waitForEvent('response', { predicate: (r) => r.request().method() === 'POST' && r.url().includes('/app-signin?'), timeout: 20000 }).catch(() => null);
  await page.mouse.click(5, 5);
  await page.waitForTimeout(300);
  await page.mouse.click(6, 6);
  const popupAnswer = await minted;
  await page.waitForTimeout(2500);
  const read = await page.evaluate(() => window.__read);
  note('h5', 'a script on an ordinary page cannot read the code a form post into a popup returns (the answer is in a window group of its own)', Boolean(popupAnswer) && popupAnswer.status() === 200 && !String(read).includes('dynodoc://'), JSON.stringify({ answered: popupAnswer?.status(), read: String(read).slice(0, 80) }));

  // 4. the proxy limits POSTs to the form, and counts no GET
  const flood = await page.evaluate(async (target) => {
    const post = () => fetch(target, { method: 'POST', headers: { 'content-type': 'application/x-www-form-urlencoded' }, body: 'x=1' }).then((r) => r.status).catch(() => 0);
    const get = () => fetch(target).then((r) => r.status).catch(() => 0);
    const gets = await Promise.all(Array.from({ length: 90 }, get));
    const posts = await Promise.all(Array.from({ length: 90 }, post));
    return { gets429: gets.filter((s) => s === 429).length, posts429: posts.filter((s) => s === 429).length, postsPassed: posts.filter((s) => s !== 429 && s !== 0).length };
  }, request());
  note('h7', 'the proxy limits POSTs to /app-signin per address (429 after the burst) and counts no GET', flood.posts429 > 0 && flood.postsPassed > 0 && flood.gets429 === 0, JSON.stringify(flood));
  return results;
}
