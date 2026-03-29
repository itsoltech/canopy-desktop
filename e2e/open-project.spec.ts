import { test, expect, openProject } from './fixtures'
import { mkdtemp, rm, writeFile } from 'fs/promises'
import { join } from 'path'
import { tmpdir } from 'os'
import { execSync } from 'child_process'

let tmpDir: string

test.beforeEach(async () => {
  tmpDir = await mkdtemp(join(tmpdir(), 'canopy-e2e-'))
  execSync('git init', { cwd: tmpDir })
  execSync('git config user.email "test@test.com"', { cwd: tmpDir })
  execSync('git config user.name "Test"', { cwd: tmpDir })
  await writeFile(join(tmpDir, 'README.md'), '# Test Project\n')
  execSync('git add . && git commit -m "init"', { cwd: tmpDir })
})

test.afterEach(async () => {
  await rm(tmpDir, { recursive: true, force: true })
})

test('opens a git project and shows it in UI', async ({ electronApp, page }) => {
  // Dashboard visible before opening a project
  await page.waitForSelector('.dashboard', { state: 'visible', timeout: 10_000 })

  await openProject(electronApp, page, tmpDir)

  // Titlebar shows project name
  const projectName = tmpDir.split('/').pop()!
  await expect(page.locator('.titlebar .title')).toContainText(projectName, { timeout: 10_000 })

  // Branch name visible
  await expect(page.locator('.titlebar .branch')).toBeVisible()

  // Dashboard hidden
  await expect(page.locator('.dashboard')).not.toBeVisible()

  // Sidebar visible
  await expect(page.locator('.sidebar')).toBeVisible()
})

test('titlebar shows branch and document title updates', async ({ electronApp, page }) => {
  await openProject(electronApp, page, tmpDir)

  const projectName = tmpDir.split('/').pop()!
  await expect(page.locator('.titlebar .branch')).toBeVisible({ timeout: 10_000 })

  const title = await page.title()
  expect(title).toContain(projectName)
})

test('tab bar appears with a shell tab', async ({ electronApp, page }) => {
  await openProject(electronApp, page, tmpDir)

  await expect(page.locator('.tab-bar')).toBeVisible({ timeout: 10_000 })
})

test('non-git folder opens without branch info', async ({ electronApp, page }) => {
  const plainDir = await mkdtemp(join(tmpdir(), 'canopy-e2e-plain-'))
  await writeFile(join(plainDir, 'hello.txt'), 'Hello\n')

  await openProject(electronApp, page, plainDir)

  const projectName = plainDir.split('/').pop()!
  await expect(page.locator('.titlebar .title')).toContainText(projectName, { timeout: 10_000 })

  // No branch for non-git folders
  await expect(page.locator('.titlebar .branch')).not.toBeVisible()
  // Clean up non-git temp dir
  await rm(plainDir, { recursive: true, force: true })
})
