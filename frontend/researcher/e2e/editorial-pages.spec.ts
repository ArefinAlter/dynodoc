import { expect, test } from '@playwright/test';

// The editorial marketing pages: one h1, one main landmark, a title and description, no
// stale research-product vocabulary, and no horizontal overflow on a phone.
const PAGES: [path: string, heading: RegExp][] = [
  ['/features', /^Features$/],
  ['/why-dynodoc', /^Documents have no provenance\.$/],
  ['/roadmap', /^The roadmap, without promises\.$/],
  ['/changelog', /^Changelog$/],
  ['/brand', /^Brand and press kit$/]
];

for (const [path, heading] of PAGES) {
  test(`${path} is a single-h1 page without legacy research framing`, async ({ page }) => {
    await page.goto(path);
    await expect(page.locator('main#main')).toHaveCount(1);
    await expect(page.getByRole('heading', { level: 1 })).toHaveCount(1);
    await expect(page.getByRole('heading', { level: 1 })).toHaveText(heading);
    await expect(page).toHaveTitle(/ · Dynodoc$/);
    await expect(page.locator('meta[name="description"]')).toHaveAttribute('content', /.{40,}/);
    const text = (await page.locator('main#main').textContent()) ?? '';
    expect(text).not.toMatch(
      /questionnaire|XLSForm|Kobo|ODK|REDCap|survey|fieldwork|FAIR|replication|researcher/i
    );
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(path);
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
      390
    );
  });
}

test('the roadmap and changelog never present source work as deployed', async ({ page }) => {
  await page.goto('/roadmap');
  const roadmap = (await page.locator('main#main').textContent()) ?? '';
  expect(roadmap).toMatch(/Built and tested, not enabled on the hosted pilot/);
  expect(roadmap).toMatch(/application f62d62c on database schema 41/);
  expect(roadmap).toMatch(/Nothing has been submitted and no listing exists/);
  await page.goto('/changelog');
  await expect(page.getByText('In source').first()).toBeVisible();
});

test('the press kit offers its logo and art downloads and the operator contact', async ({
  page,
  request
}) => {
  await page.goto('/brand');
  for (const file of ['mark.svg', 'logo.svg', 'logo-dark.svg', 'social.png']) {
    await expect(page.locator(`a[href="/brand/${file}"]`).first()).toBeVisible();
    expect((await request.get(`/brand/${file}`)).status()).toBe(200);
  }
  for (const art of ['rex', 'stego', 'trike', 'brachio', 'ptero', 'fossil', 'egg', 'tracks']) {
    expect((await request.get(`/brand/art/${art}.svg`)).status()).toBe(200);
  }
  await expect(page.getByRole('link', { name: 'info@vernesystems.com' }).first()).toBeVisible();
  await expect(page.getByText(/AGPL/)).toHaveCount(0);
});

test('/source redirects to the open engine repository and serves no archive', async ({
  request
}) => {
  const response = await request.get('/source', { maxRedirects: 0 });
  expect(response.status()).toBe(302);
  expect(response.headers()['location']).toBe('https://github.com/ArefinAlter/dynodoc-engine');
});

test('security.txt names the email and private-reporting contacts', async ({ request }) => {
  const response = await request.get('/.well-known/security.txt');
  expect(response.status()).toBe(200);
  const text = await response.text();
  expect(text).toContain('Contact: mailto:info@vernesystems.com');
  expect(text).toContain(
    'Contact: https://github.com/ArefinAlter/dynodoc-engine/security/advisories/new'
  );
  expect(text).toContain('Policy: https://dynodoc.online/security#report');
});
