import { afterEach, describe, expect, it, vi } from 'vitest'
import { focusModalAndReturnToOpener } from './focusTrap'

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('focusModalAndReturnToOpener', () => {
  it('focuses the modal and restores the connected opener on cleanup', () => {
    const opener = { isConnected: true, focus: vi.fn() }
    const modal = { focus: vi.fn() }
    vi.stubGlobal('document', { activeElement: opener })

    const restoreFocus = focusModalAndReturnToOpener(modal as unknown as HTMLElement)

    expect(modal.focus).toHaveBeenCalledOnce()
    restoreFocus()
    expect(opener.focus).toHaveBeenCalledOnce()
  })

  it('does not focus an opener that was removed while the modal was open', () => {
    const opener = { isConnected: false, focus: vi.fn() }
    vi.stubGlobal('document', { activeElement: opener })

    const restoreFocus = focusModalAndReturnToOpener(undefined)
    restoreFocus()

    expect(opener.focus).not.toHaveBeenCalled()
  })
})
