import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { codexAdapter } from './codex'

describe('codexAdapter.setupSettings', () => {
  let root: string
  let worktree: string
  let outside: string

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'canopy-codex-'))
    worktree = join(root, 'repo')
    outside = join(root, 'bashrc')
    mkdirSync(worktree)
    writeFileSync(outside, 'export PATH=$HOME/bin:$PATH\n')
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('refuses a committed hooks.json symlink that points outside the worktree', () => {
    mkdirSync(join(worktree, '.codex'))
    symlinkSync('../../bashrc', join(worktree, '.codex', 'hooks.json'))

    expect(() =>
      codexAdapter.setupSettings('unused', worktree, '/canopy/hook.sh', null, undefined),
    ).toThrow()
    expect(readFileSync(outside, 'utf-8')).toBe('export PATH=$HOME/bin:$PATH\n')
  })

  it('leaves a symlinked .gitignore that points outside the worktree untouched', () => {
    symlinkSync('../bashrc', join(worktree, '.gitignore'))

    const setup = codexAdapter.setupSettings('unused', worktree, '/canopy/hook.sh', null)
    const duringSession = readFileSync(outside, 'utf-8')
    setup.cleanup()

    expect(duringSession).toBe('export PATH=$HOME/bin:$PATH\n')
  })
})
