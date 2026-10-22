import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig, devices } from '@playwright/test';
const external = process.env.PLAYWRIGHT_BASE_URL;
// The self-hosted edition runs the same build with DYNODOC_MODE=selfhosted, so it needs its
// own server (and one more with email sign-in off). Their settings are in e2e/selfhosted.env.
// With an external stack the servers are started outside and named here; without one, they
// are started below. The hosted specs never run against them, nor these specs against hosted.
const selfhostedUrl = external ? process.env.SELFHOSTED_BASE_URL : 'http://localhost:4174';
const noEmailUrl = external ? process.env.SELFHOSTED_NOEMAIL_BASE_URL : 'http://localhost:4175';
// A fourth server sends its mail through SMTP, to the server the spec itself starts.
const smtpUrl = external ? process.env.SELFHOSTED_SMTP_BASE_URL : 'http://localhost:4176';
// A fifth serves its own copy of Office.js from a folder (DYNODOC_OFFICE_JS_DIR); the folder
// holds a stand-in file, because the real library is not in this repository.
const officeJsUrl = external ? process.env.SELFHOSTED_OFFICEJS_BASE_URL : 'http://localhost:4177';
const officeJsDir = fileURLToPath(new URL('./e2e/fixtures/office-js', import.meta.url));
const settingsOf = (name: string) =>
  Object.fromEntries(
    readFileSync(new URL(`./e2e/${name}`, import.meta.url), 'utf8')
      .split(/\r?\n/)
      .filter((line) => line.trim() && !line.startsWith('#'))
      .map((line) => [line.slice(0, line.indexOf('=')), line.slice(line.indexOf('=') + 1)])
  );
const selfhostedSettings = settingsOf('selfhosted.env');
const smtpSettings = settingsOf('selfhosted-smtp.env');
// A self-hosted server will not start without secrets; these exist only for the preview
// servers this file starts. The hosted preview needs none, as before. A stack started outside
// uses its own.
const previewSecrets = {
  SESSION_SECRET: process.env.SESSION_SECRET ?? 'e2e-preview-only-session-secret-0123456789abcdef',
  WEB_SERVICE_KEY: process.env.WEB_SERVICE_KEY ?? 'e2e-preview-only-service-key-0123456789abcdef'
};
const selfhostedSpecs = /selfhosted(-noemail|-smtp|-officejs)?\.spec\.ts$/;
// Chromium always runs the whole suite. BROWSER_MATRIX adds Firefox, WebKit and two phone
// emulations (docs/BROWSER-SUPPORT.md): `full` runs the authenticated specs on the desktop
// engines and the public and workspace specs on the phones; `smoke` keeps only the tests
// tagged @smoke, which is what CI runs on them. Emulation is not a real device.
const matrix = process.env.BROWSER_MATRIX;
const phoneSpecs = /(smoke|editorial-pages|ux|workspace|projects|browsers|pwa)\.spec\.ts$/;
// Playwright's WebKit does not apply page.route() mocks to a page a service worker controls,
// even when the worker lets the request through, which breaks the specs that mock the network.
// The WebKit projects therefore block workers, except pwa.spec.ts, which turns them back on.
const noWorker = { serviceWorkers: 'block' as const };
const extra = matrix
  ? [
      { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
      { name: 'webkit', use: { ...devices['Desktop Safari'], ...noWorker } },
      { name: 'pixel', use: { ...devices['Pixel 7'] }, testMatch: phoneSpecs },
      { name: 'iphone', use: { ...devices['iPhone 15'], ...noWorker }, testMatch: phoneSpecs }
    ].map((project) => ({
      ...project,
      testIgnore: selfhostedSpecs,
      grep: matrix === 'smoke' ? /@smoke/ : undefined,
      // Creating a workbook takes about 3.5 s here on every engine, and tracing makes the
      // slower engines miss the 5 s default. Same assertions, more patience.
      expect: { timeout: 15000 }
    }))
  : [];
export default defineConfig({
  testDir: 'e2e',
  timeout: 60000,
  workers: 1,
  webServer: external
    ? undefined
    : [
        {
          // CI already builds in a separate checked step. Keep local runs self-contained.
          command: process.env.CI
            ? 'pnpm preview --host 127.0.0.1 --port 4173'
            : 'pnpm build && pnpm preview --port 4173',
          timeout: 180000,
          port: 4173,
          reuseExistingServer: !process.env.CI
        },
        ...[
          // No mail transport: email sign-in is on only because DEV_AUTH is (a test stack).
          [4174, { DEV_AUTH: 'true' }],
          [4175, { AUTH_EMAIL_LINK: 'off' }],
          [4176, { ...smtpSettings, DEV_AUTH: 'false' }],
          [4177, { DEV_AUTH: 'true', DYNODOC_OFFICE_JS_DIR: officeJsDir }]
        ].map(([port, extra]) => ({
          command: `pnpm preview ${process.env.CI ? '--host 127.0.0.1 ' : ''}--port ${port}`,
          timeout: 180000,
          port: port as number,
          reuseExistingServer: !process.env.CI,
          env: {
            ...previewSecrets,
            ...selfhostedSettings,
            // The address the specs use (selfhostedUrl above); vite preview ignores ORIGIN for routing.
            ORIGIN: `http://localhost:${port}`,
            ...(extra as Record<string, string>)
          }
        }))
      ],
  use: {
    baseURL: external || 'http://localhost:4173',
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure'
  },
  projects: [
    { name: 'chromium', testIgnore: selfhostedSpecs },
    ...(selfhostedUrl
      ? [
          {
            name: 'selfhosted',
            testMatch: /selfhosted\.spec\.ts$/,
            use: { baseURL: selfhostedUrl }
          }
        ]
      : []),
    ...(noEmailUrl
      ? [
          {
            name: 'selfhosted-noemail',
            testMatch: /selfhosted-noemail\.spec\.ts$/,
            use: { baseURL: noEmailUrl }
          }
        ]
      : []),
    ...(smtpUrl
      ? [
          {
            name: 'selfhosted-smtp',
            testMatch: /selfhosted-smtp\.spec\.ts$/,
            use: { baseURL: smtpUrl }
          }
        ]
      : []),
    ...(officeJsUrl
      ? [
          {
            name: 'selfhosted-officejs',
            testMatch: /selfhosted-officejs\.spec\.ts$/,
            use: { baseURL: officeJsUrl }
          }
        ]
      : []),
    ...extra
  ]
});
