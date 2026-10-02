import { execFileSync } from 'child_process'
import { mkdtempSync, rmSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { GitRepository } from './GitRepository'

describe('GitRepository.getDiffParsed', () => {
  let repo: string

  const git = (...args: string[]): void => {
    execFileSync('git', args, { cwd: repo, stdio: 'ignore' })
  }

  beforeEach(() => {
    repo = mkdtempSync(join(tmpdir(), 'canopy-git-diff-'))
    git('init', '-q')
    git('config', 'user.email', 'canopy@example.com')
    git('config', 'user.name', 'Canopy')
    git('config', 'commit.gpgsign', 'false')
    // Git's default: non-ASCII bytes in paths are printed as quoted octal escapes.
    git('config', 'core.quotePath', 'true')
    writeFileSync(join(repo, 'café.txt'), 'one\n')
    git('add', '.')
    git('commit', '-q', '-m', 'init')
  })

  afterEach(() => {
    rmSync(repo, { recursive: true, force: true })
  })

  it('reports tracked and untracked paths with non-ASCII names verbatim', async () => {
    writeFileSync(join(repo, 'café.txt'), 'two\n')
    writeFileSync(join(repo, 'żółw.txt'), 'new\n')

    const result = await GitRepository.getDiffParsed(repo)

    if (result.isErr()) throw new Error(JSON.stringify(result.error))
    expect(result.value.files.map((file) => file.path).sort()).toEqual(['café.txt', 'żółw.txt'])
  })
})
