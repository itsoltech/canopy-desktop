/**
 * Renderer performance tests.
 *
 * Measures:
 * - FPS during various operations
 * - DOM node count
 * - Main thread long tasks
 */

import { test, expect, openProject, formatBytes } from './fixtures'

test('FPS monitoring during idle', async ({ page }) => {
  const SAMPLE_MS = 5000

  const fps = await page.evaluate(async (duration: number) => {
    return new Promise<number[]>((resolve) => {
      const frames: number[] = []
      let start = 0
      const measure = (ts: number): void => {
        if (!start) start = ts
        frames.push(ts)
        if (ts - start < duration) {
          requestAnimationFrame(measure)
        } else {
          const fpsPerSecond: number[] = []
          for (let sec = 0; sec < duration / 1000; sec++) {
            const secStart = start + sec * 1000
            const secEnd = secStart + 1000
            const count = frames.filter((f) => f >= secStart && f < secEnd).length
            fpsPerSecond.push(count)
          }
          resolve(fpsPerSecond)
        }
      }
      requestAnimationFrame(measure)
    })
  }, SAMPLE_MS)

  const avgFps = fps.reduce((a, b) => a + b, 0) / fps.length
  const minFps = Math.min(...fps)

  console.log('\n--- FPS at Idle ---')
  console.log(`Average: ${avgFps.toFixed(1)} FPS`)
  console.log(`Min:     ${minFps} FPS`)
  console.log(`Samples: ${fps.join(', ')}`)

  expect(avgFps).toBeGreaterThan(20)
})

test('DOM node count at idle', async ({ page }) => {
  const count = await page.evaluate(() => document.querySelectorAll('*').length)

  console.log('\n--- DOM Node Count (idle, no project) ---')
  console.log(`Total nodes: ${count}`)

  expect(count).toBeLessThan(5000)
})

test('DOM node count with project', async ({ electronApp, page, testProjectPath }) => {
  const beforeCount = await page.evaluate(() => document.querySelectorAll('*').length)

  await openProject(electronApp, page, testProjectPath)
  await page.waitForTimeout(3000)

  const afterCount = await page.evaluate(() => document.querySelectorAll('*').length)

  console.log('\n--- DOM Node Count (with project) ---')
  console.log(`Before project: ${beforeCount}`)
  console.log(`After project:  ${afterCount}`)
  console.log(`Delta:          +${afterCount - beforeCount}`)

  expect(afterCount).toBeLessThan(10_000)
})

interface LongTaskEntry {
  duration: number
  startTime: number
  name: string
}

test('long task detection', async ({ page, electronApp, testProjectPath }) => {
  await page.evaluate(() => {
    const w = window as unknown as Record<string, unknown>
    w.__longTasks = []
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        ;(w.__longTasks as LongTaskEntry[]).push({
          duration: entry.duration,
          startTime: entry.startTime,
          name: entry.name,
        })
      }
    })
    observer.observe({ type: 'longtask', buffered: true })
    w.__longTaskObserver = observer
  })

  await openProject(electronApp, page, testProjectPath)
  await page.waitForTimeout(5000)

  const longTasks: LongTaskEntry[] = await page.evaluate(
    () => (window as unknown as Record<string, unknown>).__longTasks as LongTaskEntry[],
  )

  console.log('\n--- Long Tasks (>50ms) ---')
  console.log(`Total long tasks: ${longTasks.length}`)

  if (longTasks.length > 0) {
    const sorted = longTasks.sort((a, b) => b.duration - a.duration)
    console.log('Top 10 longest tasks:')
    for (const task of sorted.slice(0, 10)) {
      console.log(`  ${task.duration.toFixed(0)}ms at ${task.startTime.toFixed(0)}ms`)
    }

    const totalBlocked = longTasks.reduce((sum, t) => sum + t.duration, 0)
    console.log(`Total blocked time: ${totalBlocked.toFixed(0)}ms`)
  }

  await page.evaluate(() => {
    const w = window as unknown as Record<string, unknown>
    ;(w.__longTaskObserver as PerformanceObserver)?.disconnect()
  })
})

interface CdpMetric {
  name: string
  value: number
}

test('renderer memory usage', async ({ cdp }) => {
  await cdp.send('Performance.enable')
  const { metrics } = await cdp.send('Performance.getMetrics')

  const interesting = [
    'JSHeapUsedSize',
    'JSHeapTotalSize',
    'Nodes',
    'LayoutCount',
    'RecalcStyleCount',
    'JSEventListeners',
  ]

  console.log('\n--- Renderer Metrics ---')
  for (const name of interesting) {
    const metric = (metrics as CdpMetric[]).find((m) => m.name === name)
    if (metric) {
      const value =
        name.includes('Size') || name.includes('Heap') ? formatBytes(metric.value) : metric.value
      console.log(`  ${name}: ${value}`)
    }
  }

  const listenerCount =
    (metrics as CdpMetric[]).find((m) => m.name === 'JSEventListeners')?.value ?? 0
  console.log(`\nEvent listener count: ${listenerCount}`)

  if (listenerCount > 500) {
    console.log('WARNING: High event listener count, possible leak')
  }

  await cdp.send('Performance.disable')
})
