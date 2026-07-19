import { defineConfig } from 'vitest/config'

// Unit tests for pure renderer helpers only. The app itself is built/verified via
// electron-vite + typecheck + svelte-check; end-to-end coverage lives under e2e/ (Playwright).
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/renderer/**/*.test.ts'],
  },
})
