import { execFileSync } from 'node:child_process'
import { mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { GitRepository } from './GitRepository'

describe('GitRepository.getDiffParsed untracked files', () => {
  let repo: string

  beforeEach(() => {
    repo = mkdtempSync(join(tmpdir(), 'canopy-untracked-'))
    execFileSync('git', ['init', '-q'], { cwd: repo })
  })

  afterEach(() => {
    rmSync(repo, { recursive: true, force: true })
  })

  // Symlink creation needs extra privileges on Windows.
  it.skipIf(process.platform === 'win32')(
    'shows untracked file content but never reads through an untracked symlink',
    async () => {
      writeFileSync(join(repo, 'notes.txt'), 'hello\n')
      symlinkSync(join(repo, 'notes.txt'), join(repo, 'link.txt'))

      const { files } = (await GitRepository.getDiffParsed(repo))._unsafeUnwrap()

      expect(files.find((f) => f.path === 'notes.txt')?.additions).toBe(1)
      expect(files.find((f) => f.path === 'link.txt')?.hunks).toEqual([])
    },
  )
})
