import { defineConfig } from 'vitest/config'

// Vitest config for main-process unit tests only.
// Renderer/Svelte tests, Playwright e2e, and perf benches are intentionally
// excluded — those have their own runners.
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/main/**/*.test.ts'],
    globals: false,
    passWithNoTests: false,
    reporters: 'default',
  },
})
