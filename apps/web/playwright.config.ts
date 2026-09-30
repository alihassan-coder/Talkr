import { defineConfig, devices } from '@playwright/test'

const PORT = 4173
const channel = process.env.PLAYWRIGHT_CHANNEL

// Runs against the static export (`next build` -> out/). Build first: turbo's test:e2e depends on build.
// Uses Playwright's Chromium (`playwright install chromium`); set PLAYWRIGHT_CHANNEL=chrome or msedge
// to use an installed browser instead.
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'], ...(channel ? { channel } : {}) } }],
  webServer: {
    command: 'node e2e/serve.mjs',
    url: `http://127.0.0.1:${PORT}`,
    env: { PORT: String(PORT) },
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
})
