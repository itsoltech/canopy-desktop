import { describe, expect, it, vi } from 'vitest'
import type { Database } from './Database'
import { PreferencesStore } from './PreferencesStore'

vi.mock('electron', () => ({
  safeStorage: {
    isEncryptionAvailable: () => false,
    encryptString: vi.fn(),
    decryptString: vi.fn(),
  },
}))

describe('PreferencesStore renderer-facing reads', () => {
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
})
