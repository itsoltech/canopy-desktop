import { test, expect, type ElectronApplication, type Page, _electron } from '@playwright/test'
import { realpathSync } from 'fs'
import { mkdtemp, rm, writeFile } from 'fs/promises'
import { tmpdir } from 'os'
import { join, resolve } from 'path'

const appDir = resolve(__dirname, '..')

interface AppStateSnapshot {
  workspace: {
    projects: Array<{
      workspace: { path: string }
    }>
  }
  tabs?: {
    tabsByWorktree: Record<string, unknown[]>
  }
}

interface CanopyWindow extends Window {
  api: {
    getAppState: () => Promise<AppStateSnapshot>
    getPref: (key: string) => Promise<string | null>
    perfOpenProject?: (path: string) => Promise<void>
    tabOpenTool?: (toolId: string, worktreePath: string) => Promise<unknown>
    newWindow?: () => Promise<void>
  }
}

interface WindowConfig {
  paths: string[]
}

async function launchApp(
  userDataDir: string,
  options?: { closeLastWindowQuits?: boolean },
): Promise<ElectronApplication> {
  return _electron.launch({
    args: [resolve(appDir, 'out/main/index.js')],
    env: {
      ...process.env,
      NODE_ENV: 'test',
      CANOPY_E2E: '1',
      CANOPY_PERF: '1',
      ...(options?.closeLastWindowQuits ? { CANOPY_E2E_CLOSE_LAST_WINDOW_QUITS: '1' } : {}),
      CANOPY_TEST_USER_DATA: userDataDir,
    },
  })
}

async function waitForExit(app: ElectronApplication, label: string): Promise<void> {
  const proc = app.process()
  if (proc.exitCode !== null || proc.signalCode !== null) return
  await Promise.race([
    new Promise<void>((resolve) => proc.once('exit', () => resolve())),
    new Promise<void>((_resolve, reject) =>
      setTimeout(() => reject(new Error(`${label} did not exit`)), 10_000),
    ),
  ])
}

async function quitApp(app: ElectronApplication, label: string): Promise<void> {
  await app.evaluate(({ app }) => app.quit()).catch(() => {})
  await waitForExit(app, label)
}

async function closeApp(app: ElectronApplication | null): Promise<void> {
  if (!app) return
  const proc = app.process()
  if (proc.exitCode !== null || proc.signalCode !== null) return

  await Promise.race([
    app.close().catch(() => {}),
    new Promise<void>((resolve) =>
      setTimeout(() => {
        if (proc.pid) {
          try {
            process.kill(proc.pid)
          } catch {
            // Process already exited.
          }
        }
        resolve()
      }, 5_000),
    ),
  ])
}

async function getAppState(page: Page): Promise<AppStateSnapshot> {
  return page.evaluate(() => (window as unknown as CanopyWindow).api.getAppState())
}

async function waitForProjectCount(page: Page, expectedCount: number): Promise<AppStateSnapshot> {
  const startedAt = Date.now()
  let lastState: AppStateSnapshot | null = null

  while (Date.now() - startedAt < 8_000) {
    lastState = await getAppState(page)
    if (lastState.workspace.projects.length === expectedCount) return lastState
    await page.waitForTimeout(100)
  }

  throw new Error(
    `Expected ${expectedCount} restored projects, got ${lastState?.workspace.projects.length ?? 0}`,
  )
}

async function waitForTabCount(
  page: Page,
  worktreePath: string,
  expectedCount: number,
): Promise<void> {
  const startedAt = Date.now()
  let lastCount = 0

  while (Date.now() - startedAt < 8_000) {
    const state = await getAppState(page)
    lastCount = state.tabs?.tabsByWorktree[worktreePath]?.length ?? 0
    if (lastCount >= expectedCount) return
    await page.waitForTimeout(100)
  }

  throw new Error(`Expected ${expectedCount} tab(s) for ${worktreePath}, got ${lastCount}`)
}

async function openProject(page: Page, projectPath: string): Promise<void> {
  await page.evaluate(async (path) => {
    const api = (window as unknown as CanopyWindow).api
    if (!api.perfOpenProject) throw new Error('perfOpenProject is unavailable')
    await api.perfOpenProject(path)
  }, projectPath)
}

async function openShellTab(page: Page, worktreePath: string): Promise<void> {
  await page.evaluate(async (path) => {
    const api = (window as unknown as CanopyWindow).api
    if (!api.tabOpenTool) throw new Error('tabOpenTool is unavailable')
    await api.tabOpenTool('shell', path)
  }, worktreePath)
}

async function getOpenWindowConfigs(page: Page): Promise<WindowConfig[]> {
  const raw = await page.evaluate(() =>
    (window as unknown as CanopyWindow).api.getPref('openWindowConfigs'),
  )
  return raw ? (JSON.parse(raw) as WindowConfig[]) : []
}

function expectSingleWindowConfigWithPaths(configs: WindowConfig[], paths: string[]): void {
  expect(configs).toHaveLength(1)
  expect(new Set(configs[0].paths)).toEqual(new Set(paths))
}

function sortedConfigPathSets(configs: WindowConfig[]): string[][] {
  return configs
    .map((config) => [...config.paths].sort())
    .sort((a, b) => a.join('\0').localeCompare(b.join('\0')))
}

test('restores multiple projects in the same window after quit', async () => {
  const userDataDir = await mkdtemp(join(tmpdir(), 'canopy-e2e-window-restore-data-'))
  const rawProjectA = await mkdtemp(join(tmpdir(), 'canopy-e2e-window-restore-a-'))
  const rawProjectB = await mkdtemp(join(tmpdir(), 'canopy-e2e-window-restore-b-'))
  const projectA = realpathSync(rawProjectA)
  const projectB = realpathSync(rawProjectB)
  const projectPaths = [projectA, projectB]
  let app: ElectronApplication | null = null

  try {
    await writeFile(join(rawProjectA, 'README.md'), 'A\n')
    await writeFile(join(rawProjectB, 'README.md'), 'B\n')

    app = await launchApp(userDataDir)
    let page = await app.firstWindow()
    await page.waitForLoadState('domcontentloaded')
    await page.waitForFunction(() => {
      const api = (window as unknown as CanopyWindow).api
      return Boolean(api?.perfOpenProject && api?.getAppState && api?.tabOpenTool)
    })

    await openProject(page, projectA)
    await waitForProjectCount(page, 1)
    await openProject(page, projectB)
    await waitForProjectCount(page, 2)

    await openShellTab(page, projectA)
    await waitForTabCount(page, projectA, 1)
    expectSingleWindowConfigWithPaths(await getOpenWindowConfigs(page), projectPaths)

    await app.evaluate(({ dialog }) => {
      dialog.showMessageBox = async () => ({ response: 0, checkboxChecked: false })
    })
    await quitApp(app, 'initial application')
    app = null

    app = await launchApp(userDataDir)
    page = await app.firstWindow()
    await page.waitForLoadState('domcontentloaded')
    await page.waitForFunction(() => Boolean((window as unknown as CanopyWindow).api?.getAppState))

    const restoredState = await waitForProjectCount(page, 2)
    expect(
      new Set(restoredState.workspace.projects.map((project) => project.workspace.path)),
    ).toEqual(new Set(projectPaths))

    const windowCount = await app.evaluate(
      ({ BrowserWindow }) => BrowserWindow.getAllWindows().length,
    )
    expect(windowCount).toBe(1)
    expectSingleWindowConfigWithPaths(await getOpenWindowConfigs(page), projectPaths)

    await quitApp(app, 'restored application')
    app = null
  } finally {
    await closeApp(app)
    await rm(userDataDir, { recursive: true, force: true })
    await rm(rawProjectA, { recursive: true, force: true })
    await rm(rawProjectB, { recursive: true, force: true })
  }
})

test('restores multiple projects after closing the last app window', async () => {
  const userDataDir = await mkdtemp(join(tmpdir(), 'canopy-e2e-window-close-data-'))
  const rawProjectA = await mkdtemp(join(tmpdir(), 'canopy-e2e-window-close-a-'))
  const rawProjectB = await mkdtemp(join(tmpdir(), 'canopy-e2e-window-close-b-'))
  const projectA = realpathSync(rawProjectA)
  const projectB = realpathSync(rawProjectB)
  const projectPaths = [projectA, projectB]
  let app: ElectronApplication | null = null

  try {
    await writeFile(join(rawProjectA, 'README.md'), 'A\n')
    await writeFile(join(rawProjectB, 'README.md'), 'B\n')

    app = await launchApp(userDataDir, { closeLastWindowQuits: true })
    let page = await app.firstWindow()
    await page.waitForLoadState('domcontentloaded')
    await page.waitForFunction(() => {
      const api = (window as unknown as CanopyWindow).api
      return Boolean(api?.perfOpenProject && api?.getAppState && api?.tabOpenTool)
    })

    await openProject(page, projectA)
    await waitForProjectCount(page, 1)
    await openProject(page, projectB)
    await waitForProjectCount(page, 2)

    await openShellTab(page, projectA)
    await waitForTabCount(page, projectA, 1)
    expectSingleWindowConfigWithPaths(await getOpenWindowConfigs(page), projectPaths)

    await app.evaluate(({ BrowserWindow, dialog }) => {
      dialog.showMessageBox = async () => ({ response: 0, checkboxChecked: false })
      BrowserWindow.getAllWindows()[0].close()
    })
    await waitForExit(app, 'window close application')
    app = null

    app = await launchApp(userDataDir, { closeLastWindowQuits: true })
    page = await app.firstWindow()
    await page.waitForLoadState('domcontentloaded')
    await page.waitForFunction(() => Boolean((window as unknown as CanopyWindow).api?.getAppState))

    const restoredState = await waitForProjectCount(page, 2)
    expect(
      new Set(restoredState.workspace.projects.map((project) => project.workspace.path)),
    ).toEqual(new Set(projectPaths))

    const windowCount = await app.evaluate(
      ({ BrowserWindow }) => BrowserWindow.getAllWindows().length,
    )
    expect(windowCount).toBe(1)
    expectSingleWindowConfigWithPaths(await getOpenWindowConfigs(page), projectPaths)

    await quitApp(app, 'restored window close application')
    app = null
  } finally {
    await closeApp(app)
    await rm(userDataDir, { recursive: true, force: true })
    await rm(rawProjectA, { recursive: true, force: true })
    await rm(rawProjectB, { recursive: true, force: true })
  }
})

test('persists remaining window configs after closing one of multiple windows', async () => {
  const userDataDir = await mkdtemp(join(tmpdir(), 'canopy-e2e-window-close-one-data-'))
  const rawProjectA = await mkdtemp(join(tmpdir(), 'canopy-e2e-window-close-one-a-'))
  const rawProjectB = await mkdtemp(join(tmpdir(), 'canopy-e2e-window-close-one-b-'))
  const projectA = realpathSync(rawProjectA)
  const projectB = realpathSync(rawProjectB)
  let app: ElectronApplication | null = null

  try {
    await writeFile(join(rawProjectA, 'README.md'), 'A\n')
    await writeFile(join(rawProjectB, 'README.md'), 'B\n')

    app = await launchApp(userDataDir)
    const pageA = await app.firstWindow()
    await pageA.waitForLoadState('domcontentloaded')
    await pageA.waitForFunction(() => {
      const api = (window as unknown as CanopyWindow).api
      return Boolean(api?.perfOpenProject && api?.getAppState && api?.newWindow)
    })

    await openProject(pageA, projectA)
    await waitForProjectCount(pageA, 1)

    const pageBPromise = app.waitForEvent('window')
    await pageA.evaluate(() => (window as unknown as CanopyWindow).api.newWindow?.())
    const pageB = await pageBPromise
    await pageB.waitForLoadState('domcontentloaded')
    await pageB.waitForFunction(() => {
      const api = (window as unknown as CanopyWindow).api
      return Boolean(api?.perfOpenProject && api?.getAppState)
    })

    await openProject(pageB, projectB)
    await waitForProjectCount(pageB, 1)

    await expect
      .poll(async () => sortedConfigPathSets(await getOpenWindowConfigs(pageA)), {
        timeout: 8_000,
      })
      .toEqual([[projectA], [projectB]])

    const pageBClosed = pageB.waitForEvent('close')
    await pageB.evaluate(() => window.close())
    await pageBClosed

    await expect
      .poll(() => app!.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length), {
        timeout: 8_000,
      })
      .toBe(1)

    await expect
      .poll(async () => sortedConfigPathSets(await getOpenWindowConfigs(pageA)), {
        timeout: 8_000,
      })
      .toEqual([[projectA]])

    await quitApp(app, 'remaining window application')
    app = null
  } finally {
    await closeApp(app)
    await rm(userDataDir, { recursive: true, force: true })
    await rm(rawProjectA, { recursive: true, force: true })
    await rm(rawProjectB, { recursive: true, force: true })
  }
})
