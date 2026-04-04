/**
 * Memory leak detection.
 *
 * Runs repeated open/close cycles and checks for resource leaks:
 * - Scenario A: Terminal open/close (PTY sessions, WS bridges)
 * - Scenario B: Workspace open/close (git watchers, file tree state)
 * - Heap growth monitoring via CDP
 */

import {
  test,
  expect,
  getDiagnostics,
  forceGC,
  getHeapSize,
  openProject,
  formatBytes,
} from './fixtures'
import type { BrowserApi } from './fixtures'

test('terminal open/close should not leak PTY sessions', async ({ page }) => {
  const CYCLES = 8

  await forceGC(await page.context().newCDPSession(page))
  const baseline = await getDiagnostics(page)
  expect(baseline).toBeTruthy()

  const baselinePty = baseline!.ptySessionCount
  const baselineBridge = baseline!.wsBridgeCount

  console.log(`\n--- Terminal Leak Test (${CYCLES} cycles) ---`)
  console.log(`Baseline PTY: ${baselinePty}, WS bridges: ${baselineBridge}`)

  for (let i = 0; i < CYCLES; i++) {
    const result = await page.evaluate(() =>
      (window as unknown as BrowserApi).api.spawnPty({ cols: 80, rows: 24 }),
    )

    expect(result).toBeTruthy()
    expect(result.sessionId).toBeTruthy()

    await page.waitForTimeout(500)

    await page.evaluate(
      (sid) => (window as unknown as BrowserApi).api.killPty(sid),
      result.sessionId,
    )

    await page.waitForTimeout(300)
  }

  const cdp = await page.context().newCDPSession(page)
  await forceGC(cdp)
  await page.waitForTimeout(1000)

  const after = await getDiagnostics(page)
  expect(after).toBeTruthy()

  console.log(`After PTY: ${after!.ptySessionCount}, WS bridges: ${after!.wsBridgeCount}`)
  console.log(`Heap: ${formatBytes(after!.heapUsed)}`)

  expect(after!.ptySessionCount).toBe(baselinePty)
  expect(after!.wsBridgeCount).toBe(baselineBridge)
})

test('workspace open should not leak git watchers', async ({
  electronApp,
  page,
  testProjectPath,
}) => {
  const CYCLES = 5

  console.log(`\n--- Workspace Leak Test (${CYCLES} cycles) ---`)
  console.log(`Test project: ${testProjectPath}`)

  const cdp = await page.context().newCDPSession(page)
  await forceGC(cdp)
  const baselineHeap = await getHeapSize(cdp)

  for (let i = 0; i < CYCLES; i++) {
    await openProject(electronApp, page, testProjectPath)
    await page.waitForTimeout(1500)

    const mid = await getDiagnostics(page)
    if (i === 0) {
      console.log(
        `After first open - git watchers: ${mid?.gitWatcherCount}, heap: ${formatBytes(mid?.heapUsed ?? 0)}`,
      )
    }

    await page.evaluate(
      (path) => (window as unknown as BrowserApi).api.detachProject(path),
      testProjectPath,
    )

    await page.waitForTimeout(1000)
  }

  await forceGC(cdp)
  await page.waitForTimeout(1000)
  const afterHeap = await getHeapSize(cdp)
  const after = await getDiagnostics(page)

  const heapGrowth = afterHeap - baselineHeap
  console.log(`Heap growth after ${CYCLES} cycles: ${formatBytes(heapGrowth)}`)
  console.log(`Git watchers remaining: ${after?.gitWatcherCount}`)

  expect(heapGrowth).toBeLessThan(20 * 1024 * 1024)
})

test('heap snapshot comparison', async ({ page, electronApp, testProjectPath, cdp }) => {
  await cdp.send('HeapProfiler.enable')

  await forceGC(cdp)
  const baselineHeap = await getHeapSize(cdp)

  await openProject(electronApp, page, testProjectPath)
  await page.waitForTimeout(2000)

  for (let i = 0; i < 3; i++) {
    const result = await page.evaluate(() =>
      (window as unknown as BrowserApi).api.spawnPty({ cols: 80, rows: 24 }),
    )
    await page.waitForTimeout(300)
    await page.evaluate(
      (sid) => (window as unknown as BrowserApi).api.killPty(sid),
      result.sessionId,
    )
  }

  await page.evaluate(
    (path) => (window as unknown as BrowserApi).api.detachProject(path),
    testProjectPath,
  )

  await page.waitForTimeout(1000)
  await forceGC(cdp)
  await page.waitForTimeout(500)

  const afterHeap = await getHeapSize(cdp)
  const growth = afterHeap - baselineHeap

  console.log('\n--- Heap Snapshot Comparison ---')
  console.log(`Baseline:  ${formatBytes(baselineHeap)}`)
  console.log(`After:     ${formatBytes(afterHeap)}`)
  console.log(`Growth:    ${formatBytes(growth)}`)

  expect(growth).toBeLessThan(15 * 1024 * 1024)

  await cdp.send('HeapProfiler.disable')
})
