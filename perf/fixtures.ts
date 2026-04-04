import { test as base, type ElectronApplication, type Page, _electron } from '@playwright/test'
import type { CDPSession } from 'playwright-core'
import { resolve, join } from 'path'
import { mkdtemp, rm, mkdir, writeFile } from 'fs/promises'
import { tmpdir } from 'os'

const appDir = resolve(__dirname, '..')

export interface PerfDiagnostics {
  ptySessionCount: number
  wsBridgeCount: number
  agentSessionCount: number
  gitWatcherCount: number
  windowCount: number
  heapUsed: number
  rss: number
  uptime: number
  marks: Array<{ name: string; startTime: number }>
}

type PerfFixtures = {
  electronApp: ElectronApplication
  page: Page
  cdp: CDPSession
  testProjectPath: string
}

/** Create a synthetic git repo for benchmarking */
async function createTestProject(baseDir: string): Promise<string> {
  const projectDir = join(baseDir, 'test-project')
  await mkdir(projectDir, { recursive: true })

  // Initialize a bare git repo
  const { execFileSync } = await import('child_process')
  execFileSync('git', ['init'], { cwd: projectDir })
  execFileSync('git', ['config', 'user.email', 'test@test.com'], { cwd: projectDir })
  execFileSync('git', ['config', 'user.name', 'Test'], { cwd: projectDir })

  // Create directory structure with files
  const dirs = ['src', 'src/components', 'src/utils', 'src/stores', 'tests', 'docs']
  for (const dir of dirs) {
    await mkdir(join(projectDir, dir), { recursive: true })
  }

  // Generate files to simulate a real project
  for (let i = 0; i < 200; i++) {
    const dir = dirs[i % dirs.length]
    const ext = i % 3 === 0 ? '.ts' : i % 3 === 1 ? '.svelte' : '.md'
    const content = `// File ${i}\n` + 'x'.repeat(500) + '\n'
    await writeFile(join(projectDir, dir, `file-${i}${ext}`), content)
  }

  execFileSync('git', ['add', '.'], { cwd: projectDir })
  execFileSync('git', ['commit', '-m', 'init'], { cwd: projectDir })

  return projectDir
}

export const test = base.extend<PerfFixtures>({
  // eslint-disable-next-line no-empty-pattern
  testProjectPath: async ({}, use) => {
    const dir = await mkdtemp(join(tmpdir(), 'canopy-perf-project-'))
    const projectPath = await createTestProject(dir)
    await use(projectPath)
    await rm(dir, { recursive: true, force: true }).catch(() => {})
  },

  // eslint-disable-next-line no-empty-pattern
  electronApp: async ({}, use) => {
    const userDataDir = await mkdtemp(join(tmpdir(), 'canopy-perf-data-'))
    const app = await _electron.launch({
      args: [resolve(appDir, 'out/main/index.js')],
      env: {
        ...process.env,
        NODE_ENV: 'test',
        CANOPY_TEST_USER_DATA: userDataDir,
        CANOPY_PERF: '1',
        CANOPY_E2E: '1',
      },
    })
    await use(app)
    const pid = app.process().pid
    await Promise.race([
      app.close(),
      new Promise<void>((resolve) =>
        setTimeout(() => {
          try {
            process.kill(pid!)
          } catch {
            /* already exited */
          }
          resolve()
        }, 5_000),
      ),
    ])
    await rm(userDataDir, { recursive: true, force: true }).catch(() => {})
  },

  page: async ({ electronApp }, use) => {
    const window = await electronApp.firstWindow()
    await window.waitForLoadState('domcontentloaded')
    await use(window)
  },

  cdp: async ({ page }, use) => {
    const session = await page.context().newCDPSession(page)
    await use(session)
  },
})

export { expect } from '@playwright/test'

// -- Helpers --

export async function getDiagnostics(page: Page): Promise<PerfDiagnostics | null> {
  return page.evaluate(() =>
    (
      window as unknown as { api: { perfDiagnostics: () => Promise<PerfDiagnostics | null> } }
    ).api.perfDiagnostics(),
  )
}

export async function forceGC(cdp: CDPSession): Promise<void> {
  await cdp.send('HeapProfiler.collectGarbage')
}

export async function getHeapSize(cdp: CDPSession): Promise<number> {
  const metrics = await cdp.send('Performance.getMetrics')
  const heap = metrics.metrics.find((m: { name: string }) => m.name === 'JSHeapUsedSize')
  return heap?.value ?? 0
}

export async function startCPUProfile(cdp: CDPSession): Promise<void> {
  await cdp.send('Profiler.enable')
  await cdp.send('Profiler.start')
}

export interface CpuProfile {
  nodes: Array<{
    id: number
    callFrame: { functionName: string; url: string }
  }>
  samples: number[]
}

export async function stopCPUProfile(cdp: CDPSession): Promise<CpuProfile> {
  const { profile } = await cdp.send('Profiler.stop')
  await cdp.send('Profiler.disable')
  return profile as unknown as CpuProfile
}

export async function takeHeapSnapshot(cdp: CDPSession): Promise<string> {
  const chunks: string[] = []
  cdp.on('HeapProfiler.addHeapSnapshotChunk', (params: { chunk: string }) => {
    chunks.push(params.chunk)
  })
  await cdp.send('HeapProfiler.takeHeapSnapshot', { reportProgress: false })
  return chunks.join('')
}

interface WindowWithApi {
  api: {
    getPref: (k: string) => Promise<string | null>
    perfDiagnostics: () => Promise<PerfDiagnostics | null>
  }
}

export async function openProject(
  electronApp: ElectronApplication,
  page: Page,
  projectPath: string,
): Promise<void> {
  await page.waitForSelector('.app', { state: 'visible' })
  await page.waitForFunction(
    () =>
      !!(window as unknown as WindowWithApi).api &&
      typeof (window as unknown as WindowWithApi).api.getPref === 'function',
  )
  await electronApp.evaluate(({ BrowserWindow }, path) => {
    const win = BrowserWindow.getAllWindows()[0]
    win.webContents.send('url:action', { action: 'open', path })
  }, projectPath)
  // Wait for workspace to load
  await page.waitForTimeout(2000)
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

export function formatMs(ms: number): string {
  return `${ms.toFixed(0)}ms`
}
