import { execFileSync } from 'child_process'
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { GitRepository } from './GitRepository'

function git(cwd: string, ...args: string[]): void {
  execFileSync('git', args, { cwd, stdio: 'ignore' })
}

describe('GitRepository.getPushInfo', () => {
  let root: string
  let repo: string

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'canopy-push-info-'))
    repo = join(root, 'repo')
    mkdirSync(repo)
    git(repo, 'init', '-q', '-b', 'main')
    git(
      repo,
      '-c',
      'user.email=a@b.c',
      '-c',
      'user.name=t',
      'commit',
      '-q',
      '--allow-empty',
      '-m',
      'i',
    )
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('does not pass a repo-configured remote starting with "-" to git as an option', async () => {
    // A cloned repository controls its own .git/config; `git config` refuses to write such a
    // value, so plant it the way a malicious checkout would ship it.
    const outDir = join(root, 'out')
    mkdirSync(outDir)
    appendFileSync(join(repo, '.git', 'config'), `[branch "main"]\n\tremote = --output=${outDir}\n`)

    const result = await GitRepository.getPushInfo(repo)

    expect(result.isErr()).toBe(true)
    expect(existsSync(join(outDir, 'main..HEAD'))).toBe(false)
  })

  it('returns null when the branch has no upstream remote', async () => {
    const result = await GitRepository.getPushInfo(repo)

    expect(result._unsafeUnwrap()).toBeNull()
  })
})
