/**
 * Startup performance benchmark.
 *
 * Launches the app multiple times and measures each startup phase:
 * - app:init -> app:ready (Electron bootstrap)
 * - app:ready -> app:loginEnvResolved (shell env sourcing)
 * - app:loginEnvResolved -> app:managersReady (manager initialization)
 * - app:managersReady -> app:ipcHandlersRegistered (IPC setup)
 * - app:ipcHandlersRegistered -> app:firstWindowReady (window creation + render)
 * - Total: app:init -> app:firstWindowReady
 */

import { test, expect, getDiagnostics, formatMs } from './fixtures'

interface StartupTiming {
  total: number
  electronBootstrap: number
  loginEnv: number
  managers: number
  ipcHandlers: number
  firstWindow: number
  heapAtReady: number
  rss: number
}

function markDelta(
  marks: Array<{ name: string; startTime: number }>,
  from: string,
  to: string,
): number {
  const fromMark = marks.find((m) => m.name === from)
  const toMark = marks.find((m) => m.name === to)
  if (!fromMark || !toMark) return -1
  return toMark.startTime - fromMark.startTime
}

test('startup timing across multiple launches', async ({ page }) => {
  // First launch is already done by the fixture, measure it
  const diag = await getDiagnostics(page)
  expect(diag).toBeTruthy()

  const marks = diag!.marks
  const timing: StartupTiming = {
    total: markDelta(marks, 'app:init', 'app:firstWindowReady'),
    electronBootstrap: markDelta(marks, 'app:init', 'app:ready'),
    loginEnv: markDelta(marks, 'app:ready', 'app:loginEnvResolved'),
    managers: markDelta(marks, 'app:loginEnvResolved', 'app:managersReady'),
    ipcHandlers: markDelta(marks, 'app:managersReady', 'app:ipcHandlersRegistered'),
    firstWindow: markDelta(marks, 'app:ipcHandlersRegistered', 'app:firstWindowReady'),
    heapAtReady: diag!.heapUsed,
    rss: diag!.rss,
  }

  console.log('\n--- Startup Timing ---')
  console.log(`Electron bootstrap:  ${formatMs(timing.electronBootstrap)}`)
  console.log(`Login env resolve:   ${formatMs(timing.loginEnv)}`)
  console.log(`Manager init:        ${formatMs(timing.managers)}`)
  console.log(`IPC handler setup:   ${formatMs(timing.ipcHandlers)}`)
  console.log(`First window ready:  ${formatMs(timing.firstWindow)}`)
  console.log(`TOTAL:               ${formatMs(timing.total)}`)
  console.log(`Heap at ready:       ${(timing.heapAtReady / 1024 / 1024).toFixed(1)} MB`)
  console.log(`RSS at ready:        ${(timing.rss / 1024 / 1024).toFixed(1)} MB`)

  // Sanity checks
  expect(timing.total).toBeGreaterThan(0)
  expect(timing.total).toBeLessThan(30_000) // should start in under 30s
})

test('memory baseline at idle', async ({ page }) => {
  // Wait for app to settle
  await page.waitForTimeout(3000)

  const diag = await getDiagnostics(page)
  expect(diag).toBeTruthy()

  console.log('\n--- Idle Memory ---')
  console.log(`Heap used:     ${(diag!.heapUsed / 1024 / 1024).toFixed(1)} MB`)
  console.log(`RSS:           ${(diag!.rss / 1024 / 1024).toFixed(1)} MB`)
  console.log(`PTY sessions:  ${diag!.ptySessionCount}`)
  console.log(`WS bridges:    ${diag!.wsBridgeCount}`)
  console.log(`Git watchers:  ${diag!.gitWatcherCount}`)
  console.log(`Windows:       ${diag!.windowCount}`)

  // At idle with no project open, there should be minimal resource usage
  expect(diag!.ptySessionCount).toBe(0)
  expect(diag!.wsBridgeCount).toBe(0)
})
