/**
 * Terminal replay responsiveness diagnostic.
 *
 * Exercises a near terminal-stream buffer-cap burst of output and checks that the
 * renderer stays responsive while xterm catches up.
 */

import { test, expect, formatMs } from './fixtures'
import type { BrowserApi } from './fixtures'

interface LongTaskEntry {
  duration: number
  startTime: number
}

interface ProbeStats {
  maxLatencyMs: number
  samples: number
}

type SplitSnapshot =
  | { type: 'leaf'; pane: { id: string; sessionId: string; paneType?: string; toolId: string } }
  | { type: 'split'; first: SplitSnapshot; second: SplitSnapshot }

async function installLongTaskObserver(page: import('@playwright/test').Page): Promise<void> {
  await page.evaluate(() => {
    const w = window as unknown as {
      __terminalReplayLongTasks?: LongTaskEntry[]
      __terminalReplayLongTaskObserver?: PerformanceObserver
    }
    w.__terminalReplayLongTasks = []
    w.__terminalReplayLongTaskObserver?.disconnect()
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        w.__terminalReplayLongTasks?.push({
          duration: entry.duration,
          startTime: entry.startTime,
        })
      }
    })
    observer.observe({ type: 'longtask', buffered: true })
    w.__terminalReplayLongTaskObserver = observer
  })
}

async function readLongTasks(page: import('@playwright/test').Page): Promise<LongTaskEntry[]> {
  return page.evaluate(() => {
    const w = window as unknown as { __terminalReplayLongTasks?: LongTaskEntry[] }
    return w.__terminalReplayLongTasks ?? []
  })
}

async function stopLongTaskObserver(page: import('@playwright/test').Page): Promise<void> {
  await page.evaluate(() => {
    const w = window as unknown as {
      __terminalReplayLongTaskObserver?: PerformanceObserver
    }
    w.__terminalReplayLongTaskObserver?.disconnect()
  })
}

test('near-cap terminal output replay keeps renderer responsive', async ({ page }) => {
  const OUTPUT_BYTES = 1024 * 1024
  const PROBE_DURATION_MS = 4_000
  const MAX_LONG_TASK_MS = 250
  const MAX_TOTAL_BLOCKED_MS = 2_000
  const MAX_IPC_PROBE_MS = 500
  const DONE_MARKER = 'CANOPY_REPLAY_DONE'
  const worktreePath = process.cwd()

  await page
    .getByRole('button', { name: 'Skip setup' })
    .click({ timeout: 5_000 })
    .catch(() => {})
  await page.waitForFunction(
    () =>
      !!(window as unknown as BrowserApi).api &&
      typeof (window as unknown as BrowserApi).api.perfOpenProject === 'function',
  )
  await page.evaluate(
    (path) => (window as unknown as BrowserApi).api.perfOpenProject?.(path),
    worktreePath,
  )
  await page.evaluate(
    (path) => (window as unknown as BrowserApi).api.workspaceAttachProject(path),
    worktreePath,
  )
  await page.waitForTimeout(2_000)

  const platform = await page.evaluate(() => (window as unknown as BrowserApi).api.platform)
  await page.keyboard.press(platform === 'darwin' ? 'Meta+T' : 'Control+T')

  const terminal = page.locator('.terminal-container .xterm').first()
  await terminal.waitFor({ state: 'visible' })
  const sessionId = await page.evaluate(async (worktreePath) => {
    function findFocusedPane(
      split: SplitSnapshot,
      focusedPaneId: string,
    ): { sessionId: string; paneType?: string; toolId: string } | null {
      if (split.type === 'leaf') {
        return split.pane.id === focusedPaneId ? split.pane : null
      }
      return (
        findFocusedPane(split.first, focusedPaneId) ?? findFocusedPane(split.second, focusedPaneId)
      )
    }

    const state = await (window as unknown as BrowserApi).api.getAppState()
    const tabs = state.tabs.tabsByWorktree[worktreePath] ?? []
    const activeTab =
      tabs.find((tab) => tab.id === state.tabs.activeTabIdByWorktree[worktreePath]) ??
      tabs[tabs.length - 1]
    if (!activeTab) return null
    const focusedPane = findFocusedPane(activeTab.rootSplit, activeTab.focusedPaneId)
    return focusedPane?.sessionId ?? null
  }, worktreePath)
  expect(sessionId).toBeTruthy()

  await page.waitForTimeout(500)
  await installLongTaskObserver(page)

  const disconnectedClients = await page.evaluate(() => {
    const disconnect = (window as unknown as BrowserApi).api.perfDisconnectTerminalClients
    if (!disconnect) throw new Error('CANOPY_PERF helper perfDisconnectTerminalClients is missing')
    return disconnect()
  })
  expect(disconnectedClients).toBeGreaterThan(0)
  await page.waitForTimeout(50)

  const probePromise = page.evaluate(async (durationMs) => {
    const api = (window as unknown as BrowserApi).api
    const started = performance.now()
    let maxLatencyMs = 0
    let samples = 0
    while (performance.now() - started < durationMs) {
      const before = performance.now()
      await api.perfDiagnostics()
      maxLatencyMs = Math.max(maxLatencyMs, performance.now() - before)
      samples++
      await new Promise((resolve) => setTimeout(resolve, 50))
    }
    return { maxLatencyMs, samples } satisfies ProbeStats
  }, PROBE_DURATION_MS)

  await page.evaluate(
    ({ sessionId, bytes, marker }) => {
      const markerChars = marker
        .split('')
        .map((char) => char.charCodeAt(0))
        .join(',')
      const command = `node -e "process.stdout.write('x'.repeat(${bytes}) + String.fromCharCode(${markerChars}))"\n`
      return (window as unknown as BrowserApi).api.writePty(sessionId, command)
    },
    { sessionId, bytes: OUTPUT_BYTES, marker: DONE_MARKER },
  )

  await page.waitForFunction(
    ({ sessionId, marker }) =>
      !!(window as unknown as { __canopyTerminalMarkers?: Record<string, Record<string, true>> })
        .__canopyTerminalMarkers?.[sessionId]?.[marker],
    { sessionId: sessionId!, marker: DONE_MARKER },
    { timeout: PROBE_DURATION_MS },
  )

  const probeStats = await probePromise
  const longTasks = await readLongTasks(page)
  await stopLongTaskObserver(page)

  const maxLongTask = longTasks.reduce((max, task) => Math.max(max, task.duration), 0)
  const totalBlocked = longTasks.reduce((sum, task) => sum + task.duration, 0)

  console.log('\n--- Terminal Replay Responsiveness ---')
  console.log(`Output: ${(OUTPUT_BYTES / 1024).toFixed(0)} KiB`)
  console.log(`Long tasks: ${longTasks.length}`)
  console.log(`Max long task: ${formatMs(maxLongTask)}`)
  console.log(`Total blocked: ${formatMs(totalBlocked)}`)
  console.log(`Max IPC probe: ${formatMs(probeStats.maxLatencyMs)} (${probeStats.samples} samples)`)

  expect(maxLongTask).toBeLessThanOrEqual(MAX_LONG_TASK_MS)
  expect(totalBlocked).toBeLessThanOrEqual(MAX_TOTAL_BLOCKED_MS)
  expect(probeStats.maxLatencyMs).toBeLessThanOrEqual(MAX_IPC_PROBE_MS)

  await page.evaluate(
    (sessionId) => (window as unknown as BrowserApi).api.killPty(sessionId),
    sessionId,
  )
})
