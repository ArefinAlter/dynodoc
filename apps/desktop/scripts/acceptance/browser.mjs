// The person's browser for the sign-in scenario: real Microsoft Edge, driven with Playwright. It
// signs in on the server's own login page (the development link of a throwaway server), opens the
// address the app handed to the browser, compares the pairing code, and presses Continue the way a
// person does (a real click on a real form).
import { createRequire } from 'node:module';
import { readFileSync, existsSync, rmSync, writeFileSync } from 'node:fs';

const require = createRequire('D:/verne-build/dynodoc/wt/frontend/researcher/package.json');
const { chromium } = require('@playwright/test');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Waits for the app to write the address it would open in the system browser. */
export async function waitForAddress(file, timeoutMs = 60000) {
  const t0 = Date.now();
  while (Date.now() - t0 < timeoutMs) {
    if (existsSync(file)) {
      const text = readFileSync(file, 'utf8').trim();
      if (text) return text;
    }
    await sleep(200);
  }
  throw new Error('the app did not ask for a browser');
}

/**
 * Signs in on `origin` (a throwaway server with the development link on), then opens `address` and
 * presses Continue. Returns what the page said, the pairing code it showed and where the browser
 * ended up.
 */
export async function browserSignIn({ origin, address, email, name, shots, expectPairing }) {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  const out = { steps: [] };
  try {
    const context = await browser.newContext({ viewport: { width: 1000, height: 760 } });
    const page = await context.newPage();
    if (!address.startsWith(origin + '/app-signin?')) throw new Error('not the server sign-in page');
    await page.goto(address);
    await page.waitForURL(/\/login/, { timeout: 15000 });
    out.steps.push('the server sent a visitor who is not signed in to its own sign-in page, with the request carried through');
    out.loginUrlHasNext = /next=/.test(page.url());
    await page.getByLabel('Your name').fill(name);
    await page.getByLabel('Work email').fill(email);
    await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
    await page.getByRole('button', { name: /Continue in local development/ }).click();
    await page.waitForURL(/\/app-signin\?/, { timeout: 15000 });
    out.steps.push('signed in on the server, back on the same sign-in request');
    out.requestUrl = page.url().replace(/(state|challenge)=[^&]+/g, '$1=…');
    const pairingText = await page.getByTestId('pairing').innerText();
    out.pairingShown = /Pairing code\s+(\S{4})/.exec(pairingText.replace(/\s+/g, ' '))?.[1] ?? '';
    out.pairingMatches = out.pairingShown === expectPairing;
    if (shots) await page.screenshot({ path: shots + 'web-confirm.png' });
    out.page = (await page.locator('main, body').first().innerText()).replace(/\s+/g, ' ').slice(0, 400);
    if (expectPairing && !out.pairingMatches) {
      out.steps.push('pairing code differs: not pressing Continue');
      return out;
    }
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.waitForURL(/127\.0\.0\.1:\d+\/done/, { timeout: 30000 }).catch(() => undefined);
    await sleep(500);
    out.finalUrl = page.url();
    out.finalText = (await page.locator('body').innerText().catch(() => '')).replace(/\s+/g, ' ').slice(0, 300);
    if (shots) await page.screenshot({ path: shots + 'web-done.png' });
    out.steps.push('pressed Continue; the browser went to the app on this computer');
    return out;
  } finally {
    await browser.close();
  }
}

export function resetFile(file) {
  rmSync(file, { force: true });
}

export function save(file, value) {
  writeFileSync(file, JSON.stringify(value, null, 2));
}
