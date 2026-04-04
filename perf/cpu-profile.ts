/**
 * CPU profiling.
 *
 * Captures CPU profiles during key scenarios and saves them as
 * .cpuprofile files that can be loaded into Chrome DevTools.
 *
 * Scenarios:
 * - App startup (first 5s)
 * - Project open + file tree load
 * - Terminal output flood
 */

import { writeFileSync } from 'fs'
import { join } from 'path'
import { test, expect, openProject, startCPUProfile, stopCPUProfile } from './fixtures'
import type { CpuProfile, BrowserApi } from './fixtures'

interface HotFunction {
  name: string
  url: string
  samples: number
  pct: string
}

function analyzeProfile(profile: CpuProfile): HotFunction[] {
  const nodes = profile.nodes || []
  const samples = profile.samples || []
  const sampleCounts = new Map<number, number>()
  for (const id of samples) {
    sampleCounts.set(id, (sampleCounts.get(id) || 0) + 1)
  }

  return nodes
    .filter((n) => sampleCounts.has(n.id))
    .map((n) => ({
      name: n.callFrame?.functionName || '(anonymous)',
      url: n.callFrame?.url || '',
      samples: sampleCounts.get(n.id) || 0,
      pct: (((sampleCounts.get(n.id) || 0) / samples.length) * 100).toFixed(1),
    }))
    .sort((a, b) => b.samples - a.samples)
    .slice(0, 15)
}

function printHotFunctions(fns: HotFunction[]): void {
  console.log(`\nTop 15 hot functions:`)
  for (const fn of fns) {
    const file = fn.url ? fn.url.split('/').pop() : ''
    console.log(`  ${fn.pct}%  ${fn.name} (${file})`)
  }
}

test('CPU profile: startup', async ({ cdp }) => {
  await startCPUProfile(cdp)
  await new Promise((r) => setTimeout(r, 5000))
  const profile = await stopCPUProfile(cdp)

  const outPath = join(__dirname, 'results', 'cpu-startup.cpuprofile')
  writeFileSync(outPath, JSON.stringify(profile))
  console.log(`\n--- CPU Profile: Startup ---`)
  console.log(`Saved to: ${outPath}`)
  console.log(`Open in Chrome DevTools > Performance tab > Load profile`)

  printHotFunctions(analyzeProfile(profile))
  expect(profile).toBeTruthy()
})

test('CPU profile: project open', async ({ cdp, electronApp, page, testProjectPath }) => {
  await startCPUProfile(cdp)
  await openProject(electronApp, page, testProjectPath)
  await new Promise((r) => setTimeout(r, 5000))
  const profile = await stopCPUProfile(cdp)

  const outPath = join(__dirname, 'results', 'cpu-project-open.cpuprofile')
  writeFileSync(outPath, JSON.stringify(profile))
  console.log(`\n--- CPU Profile: Project Open ---`)
  console.log(`Saved to: ${outPath}`)

  printHotFunctions(analyzeProfile(profile))
  expect(profile).toBeTruthy()
})

test('CPU profile: terminal activity', async ({ cdp, page }) => {
  const result = await page.evaluate(() =>
    (window as unknown as BrowserApi).api.spawnPty({ cols: 80, rows: 24 }),
  )
  await new Promise((r) => setTimeout(r, 1000))

  await startCPUProfile(cdp)

  await page.evaluate(
    (sid) =>
      (window as unknown as BrowserApi).api.writePty(
        sid,
        'for i in $(seq 1 500); do echo "Line $i: $(date)"; done\n',
      ),
    result.sessionId,
  )

  await new Promise((r) => setTimeout(r, 5000))
  const profile = await stopCPUProfile(cdp)

  await page.evaluate((sid) => (window as unknown as BrowserApi).api.killPty(sid), result.sessionId)

  const outPath = join(__dirname, 'results', 'cpu-terminal.cpuprofile')
  writeFileSync(outPath, JSON.stringify(profile))
  console.log(`\n--- CPU Profile: Terminal Activity ---`)
  console.log(`Saved to: ${outPath}`)

  printHotFunctions(analyzeProfile(profile))
  expect(profile).toBeTruthy()
})
