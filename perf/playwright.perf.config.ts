import { defineConfig } from '@playwright/test'
import { resolve } from 'path'

const perfDir = resolve(__dirname)
const projectRoot = resolve(__dirname, '..')

export default defineConfig({
  testDir: perfDir,
  testMatch: '**/*.ts',
  testIgnore: ['**/fixtures.ts', '**/playwright.perf.config.ts'],
  timeout: 120_000,
  workers: 1,
  reporter: [['list'], ['json', { outputFile: resolve(projectRoot, 'perf/results/report.json') }]],
  outputDir: resolve(projectRoot, 'perf/results'),
})
