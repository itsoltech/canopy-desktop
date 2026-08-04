import { describe, expect, it } from 'vitest'
import { isSafeBranchRef } from './prCreation'

describe('isSafeBranchRef', () => {
  it.each(['feature/ISSUE-123-description', 'release/1.2.3', 'dependabot/npm_and_yarn/pkg-2.0.0'])(
    'accepts safe branch ref %s',
    (branch) => {
      expect(isSafeBranchRef(branch)).toBe(true)
    },
  )

  it.each([
    '',
    ' feature/leading-space',
    'feature/trailing-space ',
    '-feature/flag',
    'feature//double',
    'feature/../escape',
    'feature/query?value',
  ])('rejects unsafe branch ref %j', (branch) => {
    expect(isSafeBranchRef(branch)).toBe(false)
  })
})
