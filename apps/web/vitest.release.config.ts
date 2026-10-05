import { defineConfig } from 'vitest/config'
import base from './vitest.config'

// `pnpm check:release`: checks the live GitHub release the site links to (see checks/release.test.ts).
// Spread rather than mergeConfig, which would add to `include` instead of replacing it.
export default defineConfig({
  ...base,
  test: { ...base.test, include: ['checks/**/*.test.ts'] },
})
