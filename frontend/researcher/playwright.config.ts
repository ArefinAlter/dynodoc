import { defineConfig, devices } from '@playwright/test';
const external = process.env.PLAYWRIGHT_BASE_URL;
// Chromium always runs the whole suite. BROWSER_MATRIX adds Firefox, WebKit and two phone
// emulations (docs/BROWSER-SUPPORT.md): `full` runs the authenticated specs on the desktop
// engines and the public and workspace specs on the phones; `smoke` keeps only the tests
// tagged @smoke, which is what CI runs on them. Emulation is not a real device.
const matrix = process.env.BROWSER_MATRIX;
const phoneSpecs = /(smoke|editorial-pages|ux|workspace|projects|browsers|pwa)\.spec\.ts$/;
const extra = matrix
  ? [
      { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
      { name: 'webkit', use: { ...devices['Desktop Safari'] } },
      { name: 'pixel', use: { ...devices['Pixel 7'] }, testMatch: phoneSpecs },
      { name: 'iphone', use: { ...devices['iPhone 15'] }, testMatch: phoneSpecs }
    ].map((project) => ({ ...project, grep: matrix === 'smoke' ? /@smoke/ : undefined }))
  : [];
export default defineConfig({
  testDir: 'e2e',
  timeout: 60000,
  workers: 1,
  webServer: external
    ? undefined
    : {
        // CI already builds in a separate checked step. Keep local runs self-contained.
        command: process.env.CI
          ? 'pnpm preview --host 127.0.0.1 --port 4173'
          : 'pnpm build && pnpm preview --port 4173',
        timeout: 180000,
        port: 4173,
        reuseExistingServer: !process.env.CI
      },
  use: {
    baseURL: external || 'http://localhost:4173',
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure'
  },
  projects: [{ name: 'chromium' }, ...extra]
});
