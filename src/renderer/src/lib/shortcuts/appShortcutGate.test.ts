import { describe, expect, it, vi } from 'vitest'
import { guardAppShortcut, isAppShortcut, shouldBlockAppShortcuts } from './appShortcutGate'

describe('shouldBlockAppShortcuts', () => {
  it('blocks Cmd/Ctrl+W while the CI run dialog is open', () => {
    expect(shouldBlockAppShortcuts('ciRunJob', false)).toBe(true)
  })

  it('blocks shortcuts while a confirmation is open', () => {
    expect(shouldBlockAppShortcuts('none', true)).toBe(true)
  })

  it('allows shortcuts without a modal overlay', () => {
    expect(shouldBlockAppShortcuts('none', false)).toBe(false)
  })

  it.each([
    ['w', false, false],
    ['k', false, false],
    ['p', false, false],
    ['b', false, false],
    [',', false, false],
    ['n', true, false],
    ['ArrowLeft', false, true],
  ])('recognizes app shortcut %s so a modal can consume it', (key, shift, alt) => {
    expect(isAppShortcut(key, shift, alt)).toBe(true)
  })

  it('does not consume ordinary copy/paste shortcuts behind a modal', () => {
    expect(isAppShortcut('c', false, false)).toBe(false)
    expect(isAppShortcut('v', false, false)).toBe(false)
  })

  it('consumes Cmd/Ctrl+W without closing content behind the CI run dialog', () => {
    const preventDefault = vi.fn()

    expect(
      guardAppShortcut('ciRunJob', false, true, {
        key: 'w',
        shiftKey: false,
        altKey: false,
        preventDefault,
      }),
    ).toBe(true)
    expect(preventDefault).toHaveBeenCalledOnce()
  })
})
