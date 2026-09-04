import { describe, expect, it, vi, beforeEach } from 'vitest'
import { dirname, join } from 'path'

// `exists()` is the gate two destructive paths key on: `performConfigWrite`
// initializes defaults when it reports false, and binding-pruning drops the repo
// from the live set. Only a genuine ENOENT may report absent.
const access = vi.fn()
const link = vi.fn()
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
  link: (...args: unknown[]) => link(...args),
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

  it('reports only ENOENT as a missing repository config', async () => {
    open.mockRejectedValueOnce(Object.assign(new Error('no such file'), { code: 'ENOENT' }))

    const result = await new RepoConfigManager().load('/repo')

    expect(result.isErr() && result.error).toMatchObject({ _tag: 'ConfigNotFound' })
  })

  it('keeps filesystem read failures distinct from a missing repository config', async () => {
    open.mockRejectedValueOnce(Object.assign(new Error('permission denied'), { code: 'EACCES' }))

    const result = await new RepoConfigManager().load('/repo')

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'ConfigReadError',
      reason: 'permission denied',
    })
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

  it('does not publish a loaded config when pretty serialization exceeds the read limit', async () => {
    const config = defaultConfig()
    config.projectOverrides = Object.fromEntries(
      Array.from({ length: 8_000 }, (_, index) => [
        `PROJECT_${index}`,
        {
          branchTemplate: {
            template: 'feature/{taskKey}-{taskTitle}',
            customVars: { owner: 'team' },
          },
        },
      ]),
    )
    const compact = JSON.stringify(config)
    expect(Buffer.byteLength(compact, 'utf8')).toBeLessThanOrEqual(1024 * 1024)
    expect(Buffer.byteLength(JSON.stringify(config, null, 2) + '\n', 'utf8')).toBeGreaterThan(
      1024 * 1024,
    )
    let consumed = false
    const read = vi.fn(async (buffer: Buffer) => {
      if (consumed) return { bytesRead: 0, buffer }
      consumed = true
      const bytes = Buffer.from(compact)
      bytes.copy(buffer)
      return { bytesRead: bytes.length, buffer }
    })
    const close = vi.fn(async () => undefined)
    open.mockResolvedValueOnce({ read, close })
    const manager = new RepoConfigManager()
    const loaded = await manager.load('/repo')
    expect(loaded.isOk()).toBe(true)

    const result = await manager.save('/repo', loaded._unsafeUnwrap())

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'ConfigWriteError',
      reason: 'Configuration file exceeds the 1 MiB size limit',
    })
    expect(mkdir).not.toHaveBeenCalled()
    expect(writeFile).not.toHaveBeenCalled()
    expect(rename).not.toHaveBeenCalled()
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

describe('RepoConfigManager.init', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mkdir.mockResolvedValue(undefined)
    link.mockResolvedValue(undefined)
    unlink.mockResolvedValue(undefined)
    writeFile.mockResolvedValue(undefined)
  })

  it('publishes a complete temporary file with an exclusive hard link', async () => {
    const result = await new RepoConfigManager().init('/repo')

    expect(result.isOk()).toBe(true)
    const temporary = writeFile.mock.calls[0]?.[0]
    const destination = join('/repo', '.canopy', 'config.json')
    expect(temporary).not.toBe(destination)
    expect(writeFile).toHaveBeenCalledWith(
      expect.stringContaining('.config.json.config-write-id.tmp'),
      expect.stringContaining('"version": 1'),
      { encoding: 'utf-8', flag: 'wx' },
    )
    expect(link).toHaveBeenCalledWith(temporary, destination)
    expect(unlink).toHaveBeenCalledWith(temporary)
    expect(writeFile.mock.invocationCallOrder[0]).toBeLessThan(link.mock.invocationCallOrder[0])
  })

  it('cannot overwrite an existing config and removes its completed temporary file', async () => {
    link.mockRejectedValueOnce(Object.assign(new Error('file exists'), { code: 'EEXIST' }))

    const result = await new RepoConfigManager().init('/repo')

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'ConfigWriteError',
      reason: 'file exists',
    })
    const temporary = writeFile.mock.calls[0]?.[0]
    expect(link).toHaveBeenCalledWith(temporary, join('/repo', '.canopy', 'config.json'))
    expect(unlink).toHaveBeenCalledWith(temporary)
    expect(rename).not.toHaveBeenCalled()
  })

  it('removes a partial temporary file when its write fails', async () => {
    writeFile.mockRejectedValueOnce(new Error('disk full'))

    const result = await new RepoConfigManager().init('/repo')

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'ConfigWriteError',
      reason: 'disk full',
    })
    expect(link).not.toHaveBeenCalled()
    expect(unlink).toHaveBeenCalledWith(writeFile.mock.calls[0]?.[0])
  })
})
