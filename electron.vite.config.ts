import { resolve } from 'path'
import { defineConfig } from 'electron-vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'

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
    plugins: [svelte(), tailwindcss()],
    server: {
      // Vite 7 binds `localhost` to [::1] only on Windows, while the Electron window resolves
      // module/HMR requests over 127.0.0.1 — the renderer then never loads in dev. Pin IPv4.
      host: '127.0.0.1',
    },
    build: {
      rollupOptions: {
        input: {
          main: resolve(__dirname, 'src/renderer/index.html'),
          notch: resolve(__dirname, 'src/renderer/notch.html'),
          remote: resolve(__dirname, 'src/renderer/remote.html'),
        },
      },
    },
  },
})
