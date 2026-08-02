import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// Unit tests for pure helpers and rune stores (renderer + main) — no Electron or
// native modules. The svelte plugin compiles $state in *.svelte.ts modules so
// stores like toast.svelte.ts are testable; components are still out of scope.
// The app itself is built/verified via electron-vite + typecheck + svelte-check;
// end-to-end coverage lives under e2e/ (Playwright).
export default defineConfig({
  plugins: [svelte()],
  test: {
    environment: 'node',
    include: ['src/renderer/**/*.test.ts', 'src/main/**/*.test.ts'],
  },
})
