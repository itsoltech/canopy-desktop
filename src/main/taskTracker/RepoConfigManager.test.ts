import { describe, expect, it, vi, beforeEach } from 'vitest'
import { dirname, join } from 'path'

// `exists()` is the gate two destructive paths key on: `performConfigWrite`
// initializes defaults when it reports false, and binding-pruning drops the repo
// from the live set. Only a genuine ENOENT may report absent.
const access = vi.fn()
const open = vi.fn()
const mkdir = vi.fn()
const readdir = vi.fn()
const rename = vi.fn()
const stat = vi.fn()
const unlink = vi.fn()
const writeFile = vi.fn()
const delay = vi.fn()
vi.mock('fs/promises', () => ({
  access: (path: string) => access(path),
  open: (...args: unknown[]) => open(...args),
  readFile: vi.fn(),
  writeFile: (...args: unknown[]) => writeFile(...args),
  mkdir: (...args: unknown[]) => mkdir(...args),
  readdir: (...args: unknown[]) => readdir(...args),
  rename: (...args: unknown[]) => rename(...args),
  stat: (...args: unknown[]) => stat(...args),
  unlink: (...args: unknown[]) => unlink(...args),
}))
vi.mock('crypto', () => ({ randomUUID: () => 'config-write-id' }))
vi.mock('timers/promises', () => ({ setTimeout: (...args: unknown[]) => delay(...args) }))

import { RepoConfigManager } from './RepoConfigManager'
import { defaultConfig } from './configDefaults'

describe('RepoConfigManager.load', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('rejects an oversized repository config without reading beyond the limit', async () => {
    const read = vi.fn(async (buffer: Buffer) => {
      buffer.fill(0x78)
      return { bytesRead: buffer.length, buffer }
    })
    const close = vi.fn(async () => undefined)
    open.mockResolvedValueOnce({ read, close })

    const result = await new RepoConfigManager().load('/repo')

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'ConfigParseError',
      reason: 'Configuration file exceeds the 1 MiB size limit',
    })
    expect(read).toHaveBeenCalledOnce()
    expect(close).toHaveBeenCalledOnce()
  })
})

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
    readdir.mockResolvedValue([])
    writeFile.mockResolvedValue(undefined)
    rename.mockResolvedValue(undefined)
    stat.mockResolvedValue({ mtimeMs: Date.now() })
    unlink.mockResolvedValue(undefined)
    delay.mockResolvedValue(undefined)
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

  it('retries transient Windows destination locks before publishing', async () => {
    rename
      .mockRejectedValueOnce(Object.assign(new Error('locked'), { code: 'EPERM' }))
      .mockRejectedValueOnce(Object.assign(new Error('still locked'), { code: 'EACCES' }))
      .mockResolvedValueOnce(undefined)

    const result = await new RepoConfigManager().save('/repo', defaultConfig())

    expect(result.isOk()).toBe(true)
    expect(rename).toHaveBeenCalledTimes(3)
    expect(delay).toHaveBeenNthCalledWith(1, 25)
    expect(delay).toHaveBeenNthCalledWith(2, 50)
  })

  it('removes stale orphaned temporary files before a later save', async () => {
    const orphan = '.config.json.abandoned.tmp'
    readdir.mockResolvedValueOnce([
      { name: orphan, isFile: () => true },
      { name: 'config.json', isFile: () => true },
    ])
    stat.mockResolvedValueOnce({ mtimeMs: 0 })

    const result = await new RepoConfigManager().save('/repo', defaultConfig())

    expect(result.isOk()).toBe(true)
    expect(unlink).toHaveBeenCalledOnce()
    expect(unlink).toHaveBeenCalledWith(join('/repo', '.canopy', orphan))
  })

  it('leaves fresh temporary files owned by another live save alone', async () => {
    const active = '.config.json.active.tmp'
    readdir.mockResolvedValueOnce([{ name: active, isFile: () => true }])
    stat.mockResolvedValueOnce({ mtimeMs: Date.now() })

    const result = await new RepoConfigManager().save('/repo', defaultConfig())

    expect(result.isOk()).toBe(true)
    expect(unlink).not.toHaveBeenCalled()
  })
})
