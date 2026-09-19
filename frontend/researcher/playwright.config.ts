import { defineConfig } from '@playwright/test';
const external = process.env.PLAYWRIGHT_BASE_URL;
export default defineConfig({
  testDir: 'e2e',
  timeout: 60000,
  workers: 1,
  webServer: external
    ? undefined
    : {
        command: 'pnpm build && pnpm preview --port 4173',
        port: 4173,
        reuseExistingServer: !process.env.CI
      },
  use: {
    baseURL: external || 'http://localhost:4173',
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure'
  }
});
