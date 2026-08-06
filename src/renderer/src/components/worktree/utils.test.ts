import { describe, expect, it } from 'vitest'
import { initialBranchListOpen, shouldReopenBranchList } from './utils'

describe('initialBranchListOpen', () => {
  it('keeps the list shut on the first frame when the parent asks for it', () => {
    // The Run job dialog: nothing is selected yet, which is normally the strongest
    // reason to open — so startCollapsed has to beat every other condition or the
    // dialog opens with a full-height branch list covering itself.
    expect(initialBranchListOpen(true, true, '', '')).toBe(false)
    expect(initialBranchListOpen(true, true, 'feature/example', 'edited')).toBe(false)
  })

  it('otherwise mirrors the reopen rule', () => {
    expect(initialBranchListOpen(false, true, '', '')).toBe(true)
    expect(initialBranchListOpen(false, true, 'feature/example', 'edited')).toBe(true)
    expect(initialBranchListOpen(false, true, 'feature/example', 'feature/example')).toBe(false)
    // Not in combobox mode: the list is always visible.
    expect(initialBranchListOpen(false, false, 'feature/example', 'feature/example')).toBe(true)
  })
})

describe('shouldReopenBranchList', () => {
  it('does not reopen the regular picker after a branch is picked', () => {
    expect(shouldReopenBranchList(false, 'feature/example', 'feature/example')).toBe(false)
    expect(shouldReopenBranchList(false, '', '')).toBe(false)
  })

  it('reopens collapsed combobox mode after its selection is reset or edited', () => {
    expect(shouldReopenBranchList(true, '', '')).toBe(true)
    expect(shouldReopenBranchList(true, 'feature/example', 'feature/edited')).toBe(true)
    expect(shouldReopenBranchList(true, 'feature/example', 'feature/example')).toBe(false)
  })
})
