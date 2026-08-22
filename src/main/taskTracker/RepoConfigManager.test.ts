import { describe, expect, it, vi, beforeEach } from 'vitest'
import { dirname } from 'path'

// `exists()` is the gate two destructive paths key on: `performConfigWrite`
// initializes defaults when it reports false, and binding-pruning drops the repo
// from the live set. Only a genuine ENOENT may report absent.
const access = vi.fn()
const mkdir = vi.fn()
const rename = vi.fn()
const unlink = vi.fn()
const writeFile = vi.fn()
vi.mock('fs/promises', () => ({
  access: (path: string) => access(path),
  readFile: vi.fn(),
  writeFile: (...args: unknown[]) => writeFile(...args),
  mkdir: (...args: unknown[]) => mkdir(...args),
  rename: (...args: unknown[]) => rename(...args),
  unlink: (...args: unknown[]) => unlink(...args),
}))
vi.mock('crypto', () => ({ randomUUID: () => 'config-write-id' }))

import { RepoConfigManager } from './RepoConfigManager'
import { defaultConfig } from './configDefaults'

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

describe('RepoConfigManager.save', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mkdir.mockResolvedValue(undefined)
    writeFile.mockResolvedValue(undefined)
    rename.mockResolvedValue(undefined)
    unlink.mockResolvedValue(undefined)
  })

  it('writes a unique same-directory temporary file before publishing it with rename', async () => {
    const result = await new RepoConfigManager().save('/repo', defaultConfig())

    expect(result.isOk()).toBe(true)
    expect(writeFile).toHaveBeenCalledWith(
      expect.stringContaining('.config.json.config-write-id.tmp'),
      expect.stringContaining('"version": 1'),
      { encoding: 'utf-8', flag: 'wx' },
    )
    const temporary = writeFile.mock.calls[0]?.[0]
    const destination = rename.mock.calls[0]?.[1]
    expect(rename).toHaveBeenCalledWith(temporary, expect.stringMatching(/config\.json$/))
    expect(dirname(String(temporary))).toBe(dirname(String(destination)))
    expect(writeFile.mock.invocationCallOrder[0]).toBeLessThan(rename.mock.invocationCallOrder[0])
    expect(unlink).not.toHaveBeenCalled()
  })

  it('removes the temporary file when publishing fails', async () => {
    rename.mockRejectedValueOnce(new Error('rename failed'))

    const result = await new RepoConfigManager().save('/repo', defaultConfig())

    expect(result.isErr()).toBe(true)
    expect(unlink).toHaveBeenCalledWith(writeFile.mock.calls[0]?.[0])
  })
})
