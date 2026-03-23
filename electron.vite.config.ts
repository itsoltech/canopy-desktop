import { defineConfig } from 'electron-vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  main: {
    build: {
      rollupOptions: {
        external: ['node-pty', 'better-sqlite3']
      }
    }
  },
  preload: {},
  renderer: {
    plugins: [svelte()]
  }
})
