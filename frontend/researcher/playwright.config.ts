import { defineConfig } from '@playwright/test';
const external = process.env.PLAYWRIGHT_BASE_URL;
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
  }
});
