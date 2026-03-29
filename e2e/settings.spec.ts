import { test, expect, openSettings, switchTab, selectOption, getStoredPref } from './fixtures'

test.describe('Settings dialog', () => {
  test('opens via Cmd+, and shows all tabs', async ({ page }) => {
    await openSettings(page)

    const dialog = page.locator('.prefs-container')
    await expect(dialog).toBeVisible()
    await expect(dialog.locator('#prefs-dialog-title')).toHaveText('Settings')

    for (const tab of ['General', 'Appearance', 'Tools', 'Claude', 'Git', 'Shortcuts']) {
      await expect(page.locator('.prefs-tab', { hasText: tab })).toBeVisible()
    }
  })

  test('closes with Escape', async ({ page }) => {
    await openSettings(page)
    // Focus a tab so Escape bubbles through the overlay's keydown handler
    await page.locator('.prefs-tab').first().focus()
    await page.keyboard.press('Escape')
    await expect(page.locator('.prefs-container')).not.toBeVisible()
  })

  test('closes by clicking overlay', async ({ page }) => {
    await openSettings(page)
    // Click top-left corner of overlay (outside the dialog)
    await page.locator('.prefs-overlay').click({ position: { x: 10, y: 10 } })
    await expect(page.locator('.prefs-container')).not.toBeVisible()
  })

  test('switches between tabs', async ({ page }) => {
    await openSettings(page)

    await switchTab(page, 'Appearance')
    await expect(page.locator('.prefs-content .section-title')).toHaveText('Appearance')

    await switchTab(page, 'Claude')
    await expect(page.locator('.prefs-content .section-title')).toHaveText('Claude Code')

    await switchTab(page, 'Git')
    await expect(page.locator('.prefs-content .section-title')).toHaveText('Git')

    await switchTab(page, 'General')
    await expect(page.locator('.prefs-content .section-title')).toHaveText('General')
  })

  test('settings screenshot', async ({ page }) => {
    await openSettings(page)
    await expect(page).toHaveScreenshot('settings-general.png')
  })
})

test.describe('General settings', () => {
  test('toggle reopen last workspace', async ({ page }) => {
    await openSettings(page)

    const checkbox = page.locator('.checkbox-row input[type="checkbox"]')
    const wasChecked = await checkbox.isChecked()

    await checkbox.click()
    const expected = wasChecked ? 'false' : 'true'
    expect(await getStoredPref(page, 'reopenLastWorkspace')).toBe(expected)

    // Toggle back
    await checkbox.click()
    const reverted = wasChecked ? 'true' : 'false'
    expect(await getStoredPref(page, 'reopenLastWorkspace')).toBe(reverted)
  })

  test('change URL open mode', async ({ page }) => {
    await openSettings(page)

    await selectOption(page, page.locator('.select-row .trigger').first(), 'Canopy Browser')
    expect(await getStoredPref(page, 'urlOpenMode')).toBe('canopy')

    await selectOption(page, page.locator('.select-row .trigger').first(), 'System browser')
    expect(await getStoredPref(page, 'urlOpenMode')).toBe('system')

    // Restore default
    await selectOption(page, page.locator('.select-row .trigger').first(), 'Always ask')
    expect(await getStoredPref(page, 'urlOpenMode')).toBe('ask')
  })
})

test.describe('Appearance settings', () => {
  test('change theme', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Appearance')

    // Default should be active
    await expect(page.locator('.theme-btn.active')).toHaveText('Default')

    // Select Dracula
    await page.locator('.theme-btn', { hasText: 'Dracula' }).click()
    await expect(page.locator('.theme-btn.active')).toHaveText('Dracula')
    expect(await getStoredPref(page, 'theme')).toBe('Dracula')

    // Restore
    await page.locator('.theme-btn', { hasText: 'Default' }).click()
    expect(await getStoredPref(page, 'theme')).toBe('Default')
  })

  test('change font family', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Appearance')

    const input = page.locator('#font-family')
    const original = await input.inputValue()

    await input.fill('Fira Code, monospace')
    await input.dispatchEvent('change')
    expect(await getStoredPref(page, 'fontFamily')).toBe('Fira Code, monospace')

    // Restore
    await input.fill(original)
    await input.dispatchEvent('change')
  })

  test('change font size via spinner buttons', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Appearance')

    const wrapper = page.locator('.number-wrapper')
    const display = wrapper.locator('.number-field')
    const increment = wrapper.locator('button[aria-label="Increase"]')
    const decrement = wrapper.locator('button[aria-label="Decrease"]')

    const original = await display.inputValue()

    await increment.click()
    const increased = String(Number(original) + 1)
    expect(await getStoredPref(page, 'fontSize')).toBe(increased)

    await decrement.click()
    expect(await getStoredPref(page, 'fontSize')).toBe(original)
  })

  test('font size respects min/max bounds', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Appearance')

    const display = page.locator('.number-field')

    // Type a value below min (8)
    await display.click({ clickCount: 3 })
    await display.fill('5')
    await display.blur()
    expect(await getStoredPref(page, 'fontSize')).toBe('8')

    // Type a value above max (24)
    await display.click({ clickCount: 3 })
    await display.fill('30')
    await display.blur()
    expect(await getStoredPref(page, 'fontSize')).toBe('24')

    // Restore default
    await display.click({ clickCount: 3 })
    await display.fill('13')
    await display.blur()
  })
})

test.describe('Claude settings', () => {
  test('set model', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Claude')

    const input = page.locator('#claude-model')
    await input.fill('opus')
    await input.dispatchEvent('change')
    expect(await getStoredPref(page, 'claude.model')).toBe('opus')

    // Clear
    await input.fill('')
    await input.dispatchEvent('change')
  })

  test('change permission mode', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Claude')

    await selectOption(page, page.locator('#claude-perm'), 'Plan')
    expect(await getStoredPref(page, 'claude.permissionMode')).toBe('plan')

    await selectOption(page, page.locator('#claude-perm'), 'Auto')
    expect(await getStoredPref(page, 'claude.permissionMode')).toBe('auto')

    // Restore default
    await selectOption(page, page.locator('#claude-perm'), 'Default')
    expect(await getStoredPref(page, 'claude.permissionMode')).toBe('')
  })

  test('change effort level', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Claude')

    await selectOption(page, page.locator('#claude-effort'), 'High')
    expect(await getStoredPref(page, 'claude.effortLevel')).toBe('high')

    await selectOption(page, page.locator('#claude-effort'), 'Low')
    expect(await getStoredPref(page, 'claude.effortLevel')).toBe('low')

    // Restore
    await selectOption(page, page.locator('#claude-effort'), 'Default')
    expect(await getStoredPref(page, 'claude.effortLevel')).toBe('')
  })

  test('set API key (password field)', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Claude')

    const input = page.locator('#claude-apikey')
    await expect(input).toHaveAttribute('type', 'password')

    await input.fill('sk-ant-test-key-123')
    await input.dispatchEvent('change')
    expect(await getStoredPref(page, 'claude.apiKey')).toBeTruthy()

    // Clear
    await input.fill('')
    await input.dispatchEvent('change')
  })

  test('set base URL', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Claude')

    const input = page.locator('#claude-baseurl')
    await input.fill('https://custom.api.example.com')
    await input.dispatchEvent('change')
    expect(await getStoredPref(page, 'claude.baseUrl')).toBe('https://custom.api.example.com')

    // Clear
    await input.fill('')
    await input.dispatchEvent('change')
  })

  test('change provider', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Claude')

    await selectOption(page, page.locator('#claude-provider'), 'AWS Bedrock')
    expect(await getStoredPref(page, 'claude.provider')).toBe('bedrock')

    await selectOption(page, page.locator('#claude-provider'), 'Google Vertex AI')
    expect(await getStoredPref(page, 'claude.provider')).toBe('vertex')

    // Restore
    await selectOption(page, page.locator('#claude-provider'), 'Default (Anthropic)')
    expect(await getStoredPref(page, 'claude.provider')).toBe('')
  })

  test('set system prompt', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Claude')

    const textarea = page.locator('#claude-sysprompt')
    await textarea.fill('Always respond in Polish')
    await textarea.dispatchEvent('change')
    expect(await getStoredPref(page, 'claude.appendSystemPrompt')).toBe('Always respond in Polish')

    // Clear
    await textarea.fill('')
    await textarea.dispatchEvent('change')
  })

  test('add and remove environment variable', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Claude')

    // Scroll down to env vars section
    const content = page.locator('.prefs-content')
    await content.evaluate((el) => (el.scrollTop = el.scrollHeight))

    // Click "+ Add variable"
    await page.locator('.btn-add-item').click()

    // Fill in key and value
    await page.locator('.form-input').first().fill('MY_TEST_VAR')
    await page.locator('.form-input').nth(1).fill('test_value')
    await page.locator('.btn-add').click()

    // Verify saved
    const stored = await getStoredPref(page, 'claude.customEnv')
    expect(JSON.parse(stored!)).toEqual({ MY_TEST_VAR: 'test_value' })

    // Verify it appears in the list
    await expect(page.locator('.env-key')).toHaveText('MY_TEST_VAR')
    await expect(page.locator('.env-value')).toHaveText('test_value')

    // Remove it
    await page.locator('.remove-btn', { hasText: 'Remove' }).click()
    expect(await getStoredPref(page, 'claude.customEnv')).toBe('')
  })

  test('set settings JSON', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Claude')

    const content = page.locator('.prefs-content')
    await content.evaluate((el) => (el.scrollTop = el.scrollHeight))

    const textarea = page.locator('#claude-settings')
    const json = '{"language": "polish"}'
    await textarea.fill(json)
    await textarea.dispatchEvent('change')
    expect(await getStoredPref(page, 'claude.settingsJson')).toBe(json)

    // Clear
    await textarea.fill('')
    await textarea.dispatchEvent('change')
  })
})

test.describe('Git settings', () => {
  test('change pull strategy', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Git')

    const rebaseRadio = page.locator('.radio-row').filter({ hasText: 'Rebase' }).locator('input')
    const mergeRadio = page.locator('.radio-row').filter({ hasText: 'Merge' }).locator('input')

    // Default is rebase
    await expect(rebaseRadio).toBeChecked()

    // Switch to merge
    await mergeRadio.click()
    expect(await getStoredPref(page, 'gitPullRebase')).toBe('false')

    // Switch back
    await rebaseRadio.click()
    expect(await getStoredPref(page, 'gitPullRebase')).toBe('true')
  })

  test('set worktrees directory', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Git')

    const input = page.locator('.field-input')
    await input.fill('~/canopy/worktrees')
    // Uses oninput, not onchange, so just wait a tick
    await page.waitForTimeout(100)
    expect(await getStoredPref(page, 'worktrees.baseDir')).toBe('~/canopy/worktrees')

    // Clear
    await input.fill('')
    await page.waitForTimeout(100)
    expect(await getStoredPref(page, 'worktrees.baseDir')).toBe('')
  })
})

test.describe('Settings persistence', () => {
  test('values survive dialog close and reopen', async ({ page }) => {
    await openSettings(page)
    await switchTab(page, 'Appearance')

    // Set a non-default theme
    await page.locator('.theme-btn', { hasText: 'Nord' }).click()

    // Close and reopen
    await page.keyboard.press('Escape')
    await expect(page.locator('.prefs-container')).not.toBeVisible()

    await openSettings(page)
    await switchTab(page, 'Appearance')
    await expect(page.locator('.theme-btn.active')).toHaveText('Nord')

    // Restore default
    await page.locator('.theme-btn', { hasText: 'Default' }).click()
  })
})
