/**
 * IPC traffic monitor.
 *
 * Uses main-process IPC logging (gated behind CANOPY_PERF=1) that wraps
 * ipcMain.handle/on for incoming calls and webContents.send for outgoing
 * broadcasts. Retrieves the log via perf:ipcLog IPC handler.
 *
 * Analyzes for:
 * - Broadcast storms (>10 msgs/sec on same channel)
 * - Large payloads (>50KB)
 * - Redundant calls (<100ms apart on same channel)
 */

import { writeFileSync } from 'fs'
import { join } from 'path'
import { test, openProject } from './fixtures'

interface IpcMessage {
  channel: string
  size: number
  ts: number
  dir: 'in' | 'out'
}

interface ChannelStats {
  channel: string
  count: number
  totalBytes: number
  avgSize: number
  maxSize: number
  msgsPerSec: number
  redundantCount: number
}

function analyzeTraffic(messages: IpcMessage[], durationMs: number): ChannelStats[] {
  const byChannel = new Map<string, IpcMessage[]>()

  for (const msg of messages) {
    const list = byChannel.get(msg.channel) || []
    list.push(msg)
    byChannel.set(msg.channel, list)
  }

  const stats: ChannelStats[] = []

  for (const [channel, msgs] of byChannel) {
    const sizes = msgs.map((m) => m.size)
    const totalBytes = sizes.reduce((a, b) => a + b, 0)

    let redundant = 0
    const sorted = [...msgs].sort((a, b) => a.ts - b.ts)
    for (let i = 1; i < sorted.length; i++) {
      if (sorted[i].ts - sorted[i - 1].ts < 100) redundant++
    }

    stats.push({
      channel,
      count: msgs.length,
      totalBytes,
      avgSize: Math.round(totalBytes / msgs.length),
      maxSize: Math.max(...sizes),
      msgsPerSec: msgs.length / (durationMs / 1000),
      redundantCount: redundant,
    })
  }

  return stats.sort((a, b) => b.count - a.count)
}

function printStats(stats: ChannelStats[], limit: number): void {
  for (const s of stats.slice(0, limit)) {
    const flags: string[] = []
    if (s.msgsPerSec > 10) flags.push('STORM')
    if (s.maxSize > 50_000) flags.push('LARGE')
    if (s.redundantCount > 0) flags.push(`${s.redundantCount} redundant`)
    const flagStr = flags.length > 0 ? ` [${flags.join(', ')}]` : ''
    console.log(
      `  ${s.channel}: ${s.count} calls, ${s.msgsPerSec.toFixed(1)}/s, avg ${s.avgSize}B, max ${s.maxSize}B${flagStr}`,
    )
  }
}

/** Drain and return the IPC log from the main process */
async function drainIpcLog(page: import('@playwright/test').Page): Promise<IpcMessage[]> {
  return page.evaluate(() =>
    (
      window as unknown as {
        api: {
          perfIpcLog: () => Promise<IpcMessage[]>
        }
      }
    ).api.perfIpcLog(),
  )
}

test('capture IPC traffic at idle', async ({ page }) => {
  const CAPTURE_MS = 10_000

  // Drain any startup traffic
  await drainIpcLog(page)

  console.log(`\n--- IPC Traffic Capture (${CAPTURE_MS / 1000}s idle) ---`)
  await page.waitForTimeout(CAPTURE_MS)

  const messages = await drainIpcLog(page)
  const stats = analyzeTraffic(messages, CAPTURE_MS)

  console.log(`Total messages: ${messages.length}`)
  if (stats.length > 0) {
    console.log(`\nTop channels by frequency:`)
    printStats(stats, 15)
  }

  const reportPath = join(__dirname, 'results', 'ipc-idle.json')
  writeFileSync(reportPath, JSON.stringify({ durationMs: CAPTURE_MS, messages, stats }, null, 2))
  console.log(`\nFull report: ${reportPath}`)
})

test('capture IPC traffic with project open', async ({ electronApp, page, testProjectPath }) => {
  const CAPTURE_MS = 15_000

  // Open project to generate traffic
  await openProject(electronApp, page, testProjectPath)
  await page.waitForTimeout(2000)

  // Clear startup/project-load traffic
  await drainIpcLog(page)

  console.log(`\n--- IPC Traffic Capture (${CAPTURE_MS / 1000}s with project) ---`)

  // Spawn a terminal and interact
  const ptyResult: { sessionId: string } = await page.evaluate(async () => {
    return (
      window as unknown as {
        api: { spawnPty: (o: { cols: number; rows: number }) => Promise<{ sessionId: string }> }
      }
    ).api.spawnPty({ cols: 80, rows: 24 })
  })

  await page.waitForTimeout(1000)

  for (let i = 0; i < 5; i++) {
    await page.evaluate(async (sid: string) => {
      await (
        window as unknown as {
          api: { writePty: (sid: string, data: string) => Promise<void> }
        }
      ).api.writePty(sid, 'echo hello\n')
    }, ptyResult.sessionId)
    await page.waitForTimeout(200)
  }

  await page.waitForTimeout(CAPTURE_MS - 3000)

  // Cleanup terminal
  await page.evaluate(async (sid: string) => {
    await (
      window as unknown as {
        api: { killPty: (sid: string) => Promise<void> }
      }
    ).api.killPty(sid)
  }, ptyResult.sessionId)
  await page.waitForTimeout(500)

  const messages = await drainIpcLog(page)
  const stats = analyzeTraffic(messages, CAPTURE_MS)

  console.log(`Total messages: ${messages.length}`)
  console.log(`\nTop channels by frequency:`)
  printStats(stats, 20)

  const inCount = messages.filter((m) => m.dir === 'in').length
  const outCount = messages.filter((m) => m.dir === 'out').length
  console.log(
    `\nDirection: ${inCount} incoming (renderer->main), ${outCount} outgoing (main->renderer)`,
  )

  const storms = stats.filter((s) => s.msgsPerSec > 10)
  const largePayloads = stats.filter((s) => s.maxSize > 50_000)

  if (storms.length > 0) {
    console.log(`\nBROADCAST STORMS detected:`)
    for (const s of storms) {
      console.log(`  ${s.channel}: ${s.msgsPerSec.toFixed(1)} msgs/sec`)
    }
  }

  if (largePayloads.length > 0) {
    console.log(`\nLARGE PAYLOADS detected:`)
    for (const s of largePayloads) {
      console.log(`  ${s.channel}: max ${s.maxSize} bytes`)
    }
  }

  const reportPath = join(__dirname, 'results', 'ipc-active.json')
  writeFileSync(reportPath, JSON.stringify({ durationMs: CAPTURE_MS, messages, stats }, null, 2))
  console.log(`\nFull report: ${reportPath}`)
})
