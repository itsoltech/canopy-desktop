import { resolve } from 'path'
import { defineConfig } from 'electron-vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  main: {
    build: {
      rollupOptions: {
        external: ['node-pty', 'better-sqlite3'],
      },
    },
  },
  preload: {
    build: {
      rollupOptions: {
        input: {
          index: resolve(__dirname, 'src/preload/index.ts'),
          notch: resolve(__dirname, 'src/preload/notch.ts'),
        },
      },
    },
  },
  renderer: {
    plugins: [svelte()],
    build: {
      rollupOptions: {
        input: {
          main: resolve(__dirname, 'src/renderer/index.html'),
          notch: resolve(__dirname, 'src/renderer/notch.html'),
        },
      },
    },
  },
})
