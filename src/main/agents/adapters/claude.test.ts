import { mkdtempSync, readFileSync, rmSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { claudeAdapter } from './claude'

describe('claudeAdapter.setupSettings', () => {
  let dir: string

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'canopy-claude-settings-'))
  })

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true })
  })

  function writtenSettings(overrides?: Record<string, unknown>): Record<string, unknown> {
    const settingsPath = join(dir, 'settings.json')
    claudeAdapter.setupSettings(
      settingsPath,
      dir,
      '/canopy/hook.sh',
      '/canopy/status.sh',
      overrides,
    )
    return JSON.parse(readFileSync(settingsPath, 'utf-8')) as Record<string, unknown>
  }

  // Same mapping as Claude Code 2.1.281's parser; older CLIs skip a file holding the boolean.
  it.each([
    [false, { commit: '', pr: '', sessionUrl: false }],
    [true, {}],
  ])('writes attribution %s in the object form every CLI version accepts', (value, expected) => {
    expect(writtenSettings({ attribution: value }).attribution).toEqual(expected)
  })

  it('passes an object-form attribution through unchanged', () => {
    const attribution = { commit: 'Co-Authored-By: Someone <someone@example.com>', pr: '' }
    expect(writtenSettings({ attribution }).attribution).toEqual(attribution)
  })

  it('adds no attribution key when the profile sets none', () => {
    expect(writtenSettings({ language: 'japanese' })).not.toHaveProperty('attribution')
  })

  it('keeps Canopy hooks and status line alongside a desugared attribution', () => {
    const settings = writtenSettings({ attribution: false, hooks: { Stop: [] } })
    const hooks = settings.hooks as Record<string, unknown>
    expect(hooks.Stop).toEqual([
      { matcher: '', hooks: [{ type: 'command', command: '/canopy/hook.sh' }] },
    ])
    expect(settings.statusLine).toEqual({ type: 'command', command: '/canopy/status.sh' })
  })
})
