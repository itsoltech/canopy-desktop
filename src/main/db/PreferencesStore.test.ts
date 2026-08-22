import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Database } from './Database'
import { PreferencesStore } from './PreferencesStore'

const safeStorageMock = vi.hoisted(() => ({
  isEncryptionAvailable: vi.fn(() => false),
  encryptString: vi.fn(),
  decryptString: vi.fn(),
}))

vi.mock('electron', () => ({
  safeStorage: safeStorageMock,
}))

describe('PreferencesStore renderer-facing reads', () => {
  beforeEach(() => {
    safeStorageMock.isEncryptionAvailable.mockReturnValue(false)
    safeStorageMock.encryptString.mockReset()
    safeStorageMock.decryptString.mockReset()
  })

  it('filters both credential secrets and authorization metadata from getAll', () => {
    const rows = [
      { key: 'sidebar.sections', value: '["git"]' },
      { key: 'credential.registry.v2', value: '[{"id":"credential-id"}]' },
      { key: 'credential.bindings.v2', value: '{"ci":"credential-id"}' },
      { key: 'credential.secret.v2.credential-id', value: 'secret' },
    ]
    const database = {
      db: {
        prepare: vi.fn(() => ({ all: () => rows })),
      },
    } as unknown as Database

    expect(new PreferencesStore(database).getAll()).toEqual({
      'sidebar.sections': '["git"]',
    })
  })

  it('encrypts credential registry metadata before storing it', () => {
    const run = vi.fn()
    const database = {
      db: {
        prepare: vi.fn(() => ({ run })),
      },
    } as unknown as Database
    const registry =
      '[{"id":"credential-id","verification":{"actions.read":{"reason":"upstream"}}}]'
    const encrypted = Buffer.from('encrypted-registry')
    safeStorageMock.isEncryptionAvailable.mockReturnValue(true)
    safeStorageMock.encryptString.mockReturnValue(encrypted)

    new PreferencesStore(database).set('credential.registry.v2', registry)

    expect(safeStorageMock.encryptString).toHaveBeenCalledWith(registry)
    expect(run).toHaveBeenCalledWith('credential.registry.v2', encrypted.toString('base64'))
  })

  it('runs grouped preference mutations through one SQLite transaction', () => {
    const operation = vi.fn(() => 'done')
    const execute = vi.fn((callback: () => string) => callback())
    const transaction = vi.fn((callback: () => string) => () => execute(callback))
    const database = { db: { transaction } } as unknown as Database

    expect(new PreferencesStore(database).runInTransaction(operation)).toBe('done')
    expect(transaction).toHaveBeenCalledWith(operation)
    expect(execute).toHaveBeenCalledOnce()
  })

  it('joins an existing SQLite transaction instead of opening a nested one', () => {
    const operation = vi.fn(() => 'done')
    const transaction = vi.fn()
    const database = { db: { inTransaction: true, transaction } } as unknown as Database

    expect(new PreferencesStore(database).runInTransaction(operation)).toBe('done')
    expect(operation).toHaveBeenCalledOnce()
    expect(transaction).not.toHaveBeenCalled()
  })
})
