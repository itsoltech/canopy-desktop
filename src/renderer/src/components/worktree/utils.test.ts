import { describe, expect, it } from 'vitest'
import { shouldReopenBranchList } from './utils'

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
