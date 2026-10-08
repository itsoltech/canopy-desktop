import { describe, expect, it } from 'vitest'
import path from 'path'
import { classifyWorktreeRemoveError, isProtectedRemovalTarget } from './worktreeRemoval'

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

  it('classifies a broken .git link (field ghost state) as broken-link', () => {
    expect(
      classifyWorktreeRemoveError(
        "fatal: validation failed, cannot remove working tree: 'C:/x/wt/.git' does not exist",
      ),
    ).toBe('broken-link')
  })

  it('classifies the submodule refusal as force-required (git documents --force)', () => {
    expect(
      classifyWorktreeRemoveError(
        'fatal: working trees containing submodules cannot be moved or removed',
      ),
    ).toBe('force-required')
  })

  it('falls through to other for unrecognized failures', () => {
    expect(classifyWorktreeRemoveError('fatal: repository is corrupt')).toBe('other')
  })
})

describe('isProtectedRemovalTarget', () => {
  const posix = { pathApi: path.posix, caseInsensitive: false }
  const protectedPaths = ['/home/u/Projects/app', '/home/u']

  it('refuses a listed "worktree" that contains the repository or the home folder', () => {
    // `.git/worktrees/*/gitdir` is repository content: a crafted entry can list any directory.
    expect(isProtectedRemovalTarget('/home/u/Projects', protectedPaths, posix)).toBe(true)
    expect(isProtectedRemovalTarget('/home/u/Projects/app', protectedPaths, posix)).toBe(true)
    expect(isProtectedRemovalTarget('/home', protectedPaths, posix)).toBe(true)
    expect(isProtectedRemovalTarget('/', protectedPaths, posix)).toBe(true)
  })

  it('accepts ordinary worktree locations, including ones nested under the repository', () => {
    expect(isProtectedRemovalTarget('/home/u/Projects/app-wt', protectedPaths, posix)).toBe(false)
    expect(
      isProtectedRemovalTarget('/home/u/canopy/worktrees/app/feat', protectedPaths, posix),
    ).toBe(false)
    expect(isProtectedRemovalTarget('/home/u/Projects/app/.wt/feat', protectedPaths, posix)).toBe(
      false,
    )
  })

  it('compares Windows paths case-insensitively and treats a drive root as protected', () => {
    const win = { pathApi: path.win32, caseInsensitive: true }
    const winProtected = ['C:\\Users\\u\\src\\app', 'C:\\Users\\u']
    expect(isProtectedRemovalTarget('c:\\users\\U\\SRC', winProtected, win)).toBe(true)
    expect(isProtectedRemovalTarget('C:\\', winProtected, win)).toBe(true)
    expect(isProtectedRemovalTarget('C:\\Users\\u\\src\\app-wt', winProtected, win)).toBe(false)
  })
})
