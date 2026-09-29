import { describe, expect, it } from 'vitest'
import { parseBranchNames } from './branchNames'

describe('parseBranchNames', () => {
  it('strips the current-branch and linked-worktree markers', () => {
    expect(parseBranchNames('* main\n+ feature/login\n  fix/typo\n')).toEqual([
      'main',
      'feature/login',
      'fix/typo',
    ])
  })

  it('returns nothing for empty output', () => {
    expect(parseBranchNames('')).toEqual([])
  })
})
