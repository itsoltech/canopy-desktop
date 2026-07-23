import { describe, expect, it } from 'vitest'
import { classifyWorktreeRemoveError } from './worktreeRemoval'

describe('classifyWorktreeRemoveError', () => {
  it('detects an already-unregistered worktree (partial success of a prior attempt)', () => {
    expect(
      classifyWorktreeRemoveError(
        "fatal: 'C:/Users/x/canopy/worktrees/gakko/wt' is not a working tree",
      ),
    ).toBe('already-removed')
  })

  it('detects dirty-tree refusals (needs --force, not a retry)', () => {
    expect(
      classifyWorktreeRemoveError(
        "fatal: 'wt' contains modified or untracked files, use --force to delete it",
      ),
    ).toBe('dirty')
  })

  it.each([
    "unable to unlink 'Apps/foo.ts': Permission denied",
    "warning: failed to delete 'C:/x/y': Directory not empty",
    'Access is denied.',
    'rm: cannot remove: Device or resource busy',
    'EBUSY: resource busy or locked',
    'EPERM: operation not permitted',
  ])('classifies Windows lock symptoms as locked: %s', (msg) => {
    expect(classifyWorktreeRemoveError(msg)).toBe('locked')
  })

  it('falls through to other for unrecognized failures', () => {
    expect(classifyWorktreeRemoveError('fatal: repository is corrupt')).toBe('other')
  })
})
