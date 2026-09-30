import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vitest/config'

// Unit and component tests. The Playwright suite lives in e2e/ and runs with `test:e2e`.
export default defineConfig({
  resolve: {
    alias: {
      '@talkr/model-catalog': fileURLToPath(new URL('../../packages/model-catalog', import.meta.url)),
      '@': fileURLToPath(new URL('./', import.meta.url)),
    },
  },
  oxc: {
    jsx: { runtime: 'automatic' },
  },
  test: {
    environment: 'node',
    include: ['tests/**/*.test.{ts,tsx}'],
    setupFiles: ['./tests/setup.ts'],
    restoreMocks: true,
    unstubGlobals: true,
  },
})
