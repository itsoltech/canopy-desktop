import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { GitRepository } from './GitRepository'

describe('GitRepository.getDiffParsed untracked files', () => {
  let root: string
  let repo: string

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'canopy-git-'))
    repo = join(root, 'repo')
    mkdirSync(repo)
    execFileSync('git', ['init', '-q'], { cwd: repo })
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  // Symlink creation needs extra privileges on Windows.
  it.skipIf(process.platform === 'win32')(
    'lists an untracked symlink without reading the file it points to',
    async () => {
      const outside = join(root, 'secret.txt')
      writeFileSync(outside, 'TOP-SECRET\n')
      symlinkSync(outside, join(repo, 'link'))
      writeFileSync(join(repo, 'plain.txt'), 'hello\n')

      const files = (await GitRepository.getDiffParsed(repo))._unsafeUnwrap().files

      const link = files.find((file) => file.path === 'link')
      expect(link).toMatchObject({ status: 'added', hunks: [], additions: 0 })
      expect(JSON.stringify(link)).not.toContain('TOP-SECRET')
      expect(files.find((file) => file.path === 'plain.txt')).toMatchObject({ additions: 1 })
    },
  )
})
