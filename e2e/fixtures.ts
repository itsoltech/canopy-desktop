import { test as base, type ElectronApplication, type Page, _electron } from '@playwright/test'
import { resolve, join } from 'path'
import { mkdtemp, rm } from 'fs/promises'
import { tmpdir } from 'os'

const appDir = resolve(__dirname, '..')

interface CanopyWindow extends Window {
  api: { getPref: (k: string) => Promise<string | null> }
}

type ElectronFixtures = {
  electronApp: ElectronApplication
  page: Page
}

export const test = base.extend<ElectronFixtures>({
  // eslint-disable-next-line no-empty-pattern
  electronApp: async ({}, use) => {
    const userDataDir = await mkdtemp(join(tmpdir(), 'canopy-e2e-data-'))
    const app = await _electron.launch({
      args: [resolve(appDir, 'out/main/index.js')],
      env: {
        ...process.env,
        NODE_ENV: 'test',
        CANOPY_TEST_USER_DATA: userDataDir,
        CANOPY_E2E: '1',
      },
    })
    await use(app)
    // Force-kill if graceful close hangs (e.g. workspace path deleted mid-test)
    const pid = app.process().pid
    await Promise.race([
      app.close(),
      new Promise<void>((resolve) =>
        setTimeout(() => {
          try {
            process.kill(pid!)
          } catch {
            /* process already exited */
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
})

export { expect } from '@playwright/test'

/** Open the Settings dialog via Cmd+, */
export async function openSettings(page: Page): Promise<void> {
  await page.waitForSelector('.app', { state: 'visible' })
  const modifier = process.platform === 'darwin' ? 'Meta' : 'Control'
  await page.keyboard.press(`${modifier}+,`)
  await page.waitForSelector('.prefs-container', { state: 'visible' })
}

/** Switch to a settings tab by name */
export async function switchTab(page: Page, tab: string): Promise<void> {
  await page.locator('.prefs-tab', { hasText: tab }).click()
}

/** Select a value from a CustomSelect dropdown (portal-based).
 *  `trigger` can be a Locator for the trigger button. */
export async function selectOption(
  page: Page,
  trigger: import('@playwright/test').Locator,
  label: string,
): Promise<void> {
  await trigger.click()
  await page.locator('.custom-select-dropdown [role="option"]', { hasText: label }).click()
}

/** Read a persisted preference value through the preload bridge */
export async function getStoredPref(page: Page, key: string): Promise<string | null> {
  return page.evaluate(
    (k) =>
      (
        window as unknown as { api: { getPref: (k: string) => Promise<string | null> } }
      ).api.getPref(k),
    key,
  )
}

/** Open a project by sending url:action IPC. Waits for MainLayout's effect listeners to register first. */
export async function openProject(
  electronApp: ElectronApplication,
  page: Page,
  projectPath: string,
): Promise<void> {
  await page.waitForSelector('.app', { state: 'visible' })
  // Wait for Svelte effects (including onUrlAction listener) to settle
  await page.waitForFunction(
    () =>
      !!(window as unknown as CanopyWindow).api &&
      typeof (window as unknown as CanopyWindow).api.getPref === 'function',
  )

  await electronApp.evaluate(({ BrowserWindow }, path) => {
    const win = BrowserWindow.getAllWindows()[0]
    win.webContents.send('url:action', { action: 'open', path })
  }, projectPath)
}
