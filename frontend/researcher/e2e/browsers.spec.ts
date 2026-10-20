import { expect, test, type Page } from '@playwright/test';
// Device and browser smoke. These run on Chromium, Firefox, WebKit and the two phone
// emulations (docs/BROWSER-SUPPORT.md); CI runs only the @smoke ones on the non-Chromium
// projects. Emulation is not a device: it checks layout, input type and APIs, not the
// feel of a real finger, a real keyboard or iOS's own zoom and toolbar behaviour.
const signedIn = process.env.PILOT_E2E === 'true';
const FIELDS =
  'input:not([type=checkbox]):not([type=radio]):not([type=range]):not([type=hidden]):not([type=file]):not([type=color]):not([type=button]):not([type=submit]), select, textarea';

async function signIn(page: Page) {
  await page.goto('/login');
  await page.getByLabel('Your name').fill('Browser check');
  await page
    .getByLabel('Work email')
    .fill(`browser-${Date.now()}-${Math.random().toString(36).slice(2)}@example.test`);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/projects$/);
}
async function create(page: Page, title: string, kind = 'document') {
  await page.getByRole('button', { name: 'New document', exact: true }).click();
  await page.getByLabel('Document name', { exact: true }).fill(title);
  await page.getByRole('dialog').getByRole('combobox').selectOption(kind);
  await page.getByRole('button', { name: 'Create document', exact: true }).click();
  await expect(page.getByLabel('Document title')).toHaveValue(title, { timeout: 20000 });
}
const noSideways = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1);
/** Visible form fields whose text is under 16px: iOS Safari zooms the page when one is focused. */
const smallFields = (page: Page) =>
  page.evaluate((selector) => {
    const found: string[] = [];
    for (const el of document.querySelectorAll<HTMLElement>(selector)) {
      const box = el.getBoundingClientRect();
      const style = getComputedStyle(el);
      if (!box.width || !box.height || style.visibility === 'hidden') continue;
      const size = parseFloat(style.fontSize);
      if (size < 16)
        found.push(
          `${el.tagName.toLowerCase()} "${el.getAttribute('aria-label') || el.id}" ${size}px`
        );
    }
    return found;
  }, FIELDS);
/** Visible controls in the page that a finger would miss: under 44px tall. */
const smallTargets = (page: Page) =>
  page.evaluate(() => {
    const found: string[] = [];
    const selector =
      'button, [role=button], [role=tab], select, summary, input:not([type=checkbox]):not([type=radio]):not([type=range]):not([type=hidden]):not([type=file]):not([type=color])';
    for (const el of document.querySelectorAll<HTMLElement>(selector)) {
      if (el.closest('.touch-dense')) continue;
      const box = el.getBoundingClientRect();
      const style = getComputedStyle(el);
      if (!box.width || !box.height || style.visibility === 'hidden') continue;
      // Off-screen controls (a collapsed menu, a skip link) are not tapped from here.
      if (box.bottom < 0 || box.top > innerHeight || box.right < 0 || box.left > innerWidth)
        continue;
      if (box.height < 43.5)
        found.push(
          `${el.tagName.toLowerCase()} "${(el.getAttribute('aria-label') || el.textContent || '').trim().slice(0, 30)}" ${Math.round(box.width)}x${Math.round(box.height)}`
        );
    }
    return found;
  });

test('public pages fit the screen without sideways scrolling @smoke', async ({ page }) => {
  for (const path of ['/', '/features', '/compatibility', '/login']) {
    await page.goto(path);
    await expect(page.getByRole('heading', { level: 1 }).first()).toBeVisible();
    expect(await noSideways(page), path).toBe(true);
  }
});

test('on a touch screen the sign-in fields are 16px or larger and controls are 44px tall @smoke', async ({
  page,
  isMobile
}) => {
  test.skip(!isMobile, 'Touch-screen rules apply to the phone emulations.');
  await page.goto('/login');
  await expect(page.getByLabel('Your name')).toBeVisible();
  expect(await smallFields(page)).toEqual([]);
  expect(await smallTargets(page)).toEqual([]);
});

test('sign in, open the workspace list, create and reopen a document, and see the projects page @smoke', async ({
  page,
  isMobile
}) => {
  test.skip(!signedIn, 'Requires the isolated signed-in development stack.');
  await signIn(page);
  await expect(page.getByRole('main').first()).toBeVisible();
  expect(await noSideways(page)).toBe(true);
  if (isMobile) expect(await smallFields(page)).toEqual([]);
  await page.goto('/workspace');
  await expect(page.getByRole('heading', { name: 'Home', level: 1 })).toBeVisible();
  expect(await noSideways(page)).toBe(true);
  if (isMobile) {
    expect(await smallFields(page)).toEqual([]);
    expect(await smallTargets(page)).toEqual([]);
  }
  await create(page, 'Browser check document');
  const editor = page.getByRole('textbox', { name: 'Document content' });
  await editor.click();
  await page.keyboard.type('Typed on this browser');
  // Wide screens show the saved-state text; phones show only the cloud icon, whose title and
  // the live region carry the same words.
  await expect(page.locator('.cloud')).toHaveAttribute(
    'title',
    /All changes saved|Personal draft saved/,
    {
      timeout: 20000
    }
  );
  await page.reload();
  await expect(editor).toContainText('Typed on this browser');
  expect(await noSideways(page)).toBe(true);
  await page.goto('/projects');
  await expect(page.getByRole('main').first()).toBeVisible();
  if (isMobile) {
    expect(await noSideways(page)).toBe(true);
    expect(await smallFields(page)).toEqual([]);
    expect(await smallTargets(page)).toEqual([]);
  }
});

test('on a phone the spreadsheet says plainly that a larger screen is easier, and the formula bar still edits @smoke', async ({
  page,
  isMobile
}) => {
  test.skip(!signedIn || !isMobile, 'Phone emulation with the signed-in development stack.');
  await signIn(page);
  await page.goto('/workspace');
  await create(page, 'Phone sheet', 'spreadsheet');
  const notice = page.getByRole('note').filter({ hasText: 'easier to edit on a larger screen' });
  await expect(notice).toBeVisible();
  expect(await noSideways(page)).toBe(true);
  // Double tap edits in place. iOS Safari never sends dblclick, so the editor listens for taps.
  const first = page.getByRole('button', { name: /^A1:/ });
  const box = (await first.boundingBox())!;
  // Two raw taps in a row; locator.tap() twice can take longer than a real double tap.
  await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
  await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
  await expect(page.getByLabel('Edit A1', { exact: true })).toBeVisible();
  await page.getByLabel('Edit A1', { exact: true }).press('Escape');
  await first.tap();
  const formula = page.getByLabel('Formula bar');
  await formula.tap();
  await formula.fill('=2+3');
  await formula.press('Enter');
  await expect(page.getByRole('button', { name: /^A1: 5$/ })).toBeVisible();
  await notice.getByRole('button', { name: 'Got it' }).tap();
  await expect(notice).toHaveCount(0);
});

test('the project page tabs sit side by side without overlapping @smoke', async ({ page }) => {
  test.skip(!signedIn, 'Requires the isolated signed-in development stack.');
  await signIn(page);
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await page.getByLabel('Project name').fill(`Tabs ${Date.now()}`);
  await page.getByRole('button', { name: 'Create project' }).click();
  await expect(page.getByRole('button', { name: 'Files', exact: true })).toBeVisible();
  // A squeezed tab bar once drew the labels on top of each other, so a tap hit the wrong tab.
  const overlaps = await page.evaluate(() => {
    const tabs = [...document.querySelectorAll<HTMLElement>('.tabnav button')];
    const found: string[] = [];
    tabs.forEach((tab, i) => {
      const box = tab.getBoundingClientRect();
      const name = tab.textContent?.trim();
      // The label must fit inside its own button, and the next one must start after it.
      if (tab.scrollWidth > Math.ceil(box.width) + 1) found.push(`${name} is clipped`);
      const next = tabs[i + 1]?.getBoundingClientRect();
      if (next && next.left < box.right - 1) found.push(`${name} overlaps the next tab`);
    });
    return found;
  });
  expect(overlaps).toEqual([]);
});
