import { describe, expect, it } from 'vitest'
import {
  branchPickerEnterTarget,
  initialBranchListOpen,
  newBranchNameError,
  shouldOfferExactRef,
  shouldReopenBranchList,
} from './utils'

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

describe('shouldOfferExactRef', () => {
  it('keeps exact lookup available for safe branches and tags outside the bounded list', () => {
    expect(shouldOfferExactRef('release/1.0', ['release/archive/1.0', 'release/10'])).toBe(true)
    expect(shouldOfferExactRef('v1.2.3', ['version-1.2.3'])).toBe(true)
  })

  it('does not offer exact lookup for loaded or boundary-invalid refs', () => {
    expect(shouldOfferExactRef('release/1.0', ['release/1.0'])).toBe(false)
    expect(shouldOfferExactRef('feature/', ['main'])).toBe(false)
    expect(shouldOfferExactRef('a/../b', ['main'])).toBe(false)
    expect(shouldOfferExactRef('   ', ['main'])).toBe(false)
  })
})

describe('branchPickerEnterTarget', () => {
  it('keeps Enter on the highlighted fuzzy option while exact lookup remains a separate action', () => {
    expect(branchPickerEnterTarget(['release/archive/1.0'], 0)).toBe('release/archive/1.0')
  })

  it('selects the active option even when another fuzzy result is a literal query match', () => {
    expect(branchPickerEnterTarget(['release/archive/1.0', 'release/1.0'], 0)).toBe(
      'release/archive/1.0',
    )
  })
})

describe('newBranchNameError', () => {
  const checkedOut = new Set(['main'])
  const local = ['main', 'feature/existing']

  it('accepts a valid new branch name and an empty field', () => {
    expect(newBranchNameError('feature/login-fix', checkedOut, local)).toBeNull()
    expect(newBranchNameError('', checkedOut, local)).toBeNull()
  })

  it.each([
    'feature/',
    '/feature',
    'feature//login',
    'my?branch',
    'wip*',
    'fix[1]',
    'release.',
    'topic.lock',
    'feature/.hidden',
    'a@{b}',
    '@',
  ])('rejects %s, which git refuses as a ref name', (name) => {
    expect(newBranchNameError(name, checkedOut, local)).toEqual(expect.any(String))
  })

  it('keeps the specific messages for the common mistakes', () => {
    expect(newBranchNameError('my branch', checkedOut, local)).toBe('No spaces allowed')
    expect(newBranchNameError('a..b', checkedOut, local)).toBe('Cannot contain ..')
    expect(newBranchNameError('-x', checkedOut, local)).toBe('Cannot start with -')
  })

  it('reports branches that are checked out or already exist', () => {
    expect(newBranchNameError('main', checkedOut, local)).toBe(
      'Branch is already checked out in an existing worktree',
    )
    expect(newBranchNameError('feature/existing', checkedOut, local)).toBe('Branch already exists')
  })
})
