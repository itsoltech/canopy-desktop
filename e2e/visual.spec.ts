import { test, expect } from './fixtures'

test('full page visual regression', async ({ page }) => {
  await page.waitForSelector('.app', { state: 'visible' })
  await expect(page).toHaveScreenshot('full-page.png', {
    fullPage: true,
  })
})
