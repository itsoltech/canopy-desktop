import { describe, expect, it, vi, beforeEach } from 'vitest'

// `exists()` is the gate two destructive paths key on: `performConfigWrite`
// initializes defaults when it reports false, and binding-pruning drops the repo
// from the live set. Only a genuine ENOENT may report absent.
const access = vi.fn()
vi.mock('fs/promises', () => ({
  access: (path: string) => access(path),
  readFile: vi.fn(),
  writeFile: vi.fn(),
  mkdir: vi.fn(),
}))

import { RepoConfigManager } from './RepoConfigManager'

describe('RepoConfigManager.exists', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('reports absent only for ENOENT', async () => {
    access.mockRejectedValueOnce(Object.assign(new Error('no such file'), { code: 'ENOENT' }))
    expect(await new RepoConfigManager().exists('/repo')).toBe(false)
  })

  it('reports present when the file is there', async () => {
    access.mockResolvedValueOnce(undefined)
    expect(await new RepoConfigManager().exists('/repo')).toBe(true)
  })

  it('reports present for an unreachable path, so nothing overwrites or unbinds it', async () => {
    // EACCES on the directory, or a transient EMFILE: the file may well exist.
    // Guessing "absent" here would let performConfigWrite init defaults over a
    // committed config and let pruning unbind a credential still in use.
    for (const code of ['EACCES', 'EMFILE', 'EIO']) {
      access.mockRejectedValueOnce(Object.assign(new Error(code), { code }))
      expect(await new RepoConfigManager().exists('/repo')).toBe(true)
    }
  })
})
