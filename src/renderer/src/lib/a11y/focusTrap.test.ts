import { afterEach, describe, expect, it, vi } from 'vitest'
import { captureFocusReturn, cycleFocus } from './focusTrap'

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('cycleFocus', () => {
  it('wraps Shift+Tab to the last control while the dialog container itself has focus', () => {
    const first = { focus: vi.fn() }
    const last = { focus: vi.fn() }
    // Dialogs focus their tabindex="-1" container on open; contains() is true for itself.
    const container = { querySelectorAll: () => [first, last], contains: () => true }
    vi.stubGlobal('document', { activeElement: container })
    const event = { shiftKey: true, preventDefault: vi.fn() }

    cycleFocus(container as unknown as HTMLElement, event as unknown as KeyboardEvent)

    expect(event.preventDefault).toHaveBeenCalledOnce()
    expect(last.focus).toHaveBeenCalledOnce()
  })
})

describe('captureFocusReturn', () => {
  it('restores the connected opener on cleanup', () => {
    const opener = { isConnected: true, focus: vi.fn() }
    vi.stubGlobal('document', { activeElement: opener })

    const restoreFocus = captureFocusReturn()

    restoreFocus()
    expect(opener.focus).toHaveBeenCalledOnce()
  })

  it('keeps the original opener across intermediate modal focus changes', () => {
    const opener = { isConnected: true, focus: vi.fn() }
    const documentStub = { activeElement: opener as unknown }
    vi.stubGlobal('document', documentStub)

    const restoreFocus = captureFocusReturn()
    documentStub.activeElement = { isConnected: false, focus: vi.fn() }
    documentStub.activeElement = { isConnected: true, focus: vi.fn() }
    restoreFocus()

    expect(opener.focus).toHaveBeenCalledOnce()
  })

  it('does not focus an opener that was removed while the modal was open', () => {
    const opener = { isConnected: false, focus: vi.fn() }
    vi.stubGlobal('document', { activeElement: opener })

    const restoreFocus = captureFocusReturn()
    restoreFocus()

    expect(opener.focus).not.toHaveBeenCalled()
  })
})
