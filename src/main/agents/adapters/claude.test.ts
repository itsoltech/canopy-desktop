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

describe('claudeAdapter.normalizeEvent', () => {
  // Shape of Claude Code 2.1.282's model-switch hook input: no top-level `model`.
  const modelSwitch = {
    session_id: 'session-1',
    from_model: 'claude-opus-5-5',
    to_model: 'claude-sonnet-5',
    requested_model: 'sonnet',
    source: 'command',
  }

  it('reads the model a PostModelSwitch landed on', () => {
    const event = claudeAdapter.normalizeEvent({
      ...modelSwitch,
      hook_event_name: 'PostModelSwitch',
    })
    expect(event.model).toBe('claude-sonnet-5')
  })

  it('does not take the proposed model from PreModelSwitch', () => {
    const event = claudeAdapter.normalizeEvent({
      ...modelSwitch,
      hook_event_name: 'PreModelSwitch',
    })
    expect(event.model).toBeUndefined()
  })

  it('reads model from SessionStart', () => {
    const event = claudeAdapter.normalizeEvent({
      hook_event_name: 'SessionStart',
      session_id: 'session-1',
      source: 'startup',
      model: 'claude-opus-5-5',
    })
    expect(event.model).toBe('claude-opus-5-5')
  })
})
