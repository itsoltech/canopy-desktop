import { test, expect } from './fixtures'

test('app launches and shows main window', async ({ electronApp, page }) => {
  const windowCount = electronApp.windows().length
  expect(windowCount).toBeGreaterThanOrEqual(1)

  const title = await page.title()
  expect(title).toBeTruthy()
})

test('main layout renders', async ({ page }) => {
  await page.waitForSelector('.app', { state: 'visible' })

  const app = page.locator('.app')
  await expect(app).toBeVisible()
})
