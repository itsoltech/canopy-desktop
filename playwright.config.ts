import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  timeout: 30_000,
  globalTimeout: process.env.CI ? 300_000 : 0,
  workers: 1,
  reporter: process.env.CI ? 'list' : 'html',
  outputDir: 'test-results',
})
