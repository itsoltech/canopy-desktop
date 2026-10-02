import { describe, expect, it, vi, beforeEach } from 'vitest'
import { ok } from 'neverthrow'

import { GlobalConfigManager } from './GlobalConfigManager'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { KeychainTokenStore } from './KeychainTokenStore'

const MIGRATION_FLAG_KEY = 'taskTracker.migratedToGlobalConfig'
const GLOBAL_CONFIG_KEY = 'taskTracker.globalConfig'

/**
 * Minimal in-memory PreferencesStore double. `set` is a spy so a test can make
 * the one write the migration depends on (`save()`) throw.
 */
function makePreferencesStore(seed: Record<string, string> = {}): {
  store: PreferencesStore
  values: Map<string, string>
  set: ReturnType<typeof vi.fn>
} {
  const values = new Map<string, string>(Object.entries(seed))
  const set = vi.fn((key: string, value: string) => {
    values.set(key, value)
  })
  const store = {
    get: (key: string) => values.get(key) ?? null,
    set,
    delete: (key: string) => {
      values.delete(key)
    },
  } as unknown as PreferencesStore
  return { store, values, set }
}

/** Keychain double: no existing credential, and every write succeeds. */
function makeKeychainTokenStore(): KeychainTokenStore {
  return {
    getCredentials: vi.fn(() => null),
    setCredentials: vi.fn(() => ok(undefined)),
  } as unknown as KeychainTokenStore
}

const legacyConnections = JSON.stringify([
  {
    id: 'aaaaaaaa-1111-2222-3333-444444444444',
    provider: 'jira',
    baseUrl: 'https://jira.example.com',
    projectKey: 'CAN',
    authPrefKey: 'taskTracker.token.aaaaaaaa',
    username: 'dev@example.com',
  },
])

describe('GlobalConfigManager legacy migration', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.spyOn(console, 'error').mockImplementation(() => {})
  })

  it('marks migration complete once legacy connections are migrated', () => {
    const { store, values } = makePreferencesStore({
      'taskTracker.connections': legacyConnections,
      'taskTracker.token.aaaaaaaa': 'legacy-plaintext-token',
    })

    new GlobalConfigManager(store, makeKeychainTokenStore()).load()

    expect(values.get(MIGRATION_FLAG_KEY)).toBe('1')
    expect(values.get(GLOBAL_CONFIG_KEY)).toBeDefined()
    // The plaintext token is removed only after the keychain copy exists.
    expect(values.has('taskTracker.token.aaaaaaaa')).toBe(false)
  })

  it('does not mark migration complete when the migration throws', () => {
    const { store, values, set } = makePreferencesStore({
      'taskTracker.connections': legacyConnections,
      'taskTracker.token.aaaaaaaa': 'legacy-plaintext-token',
    })
    // Fail the config write the migration performs before moving tokens into
    // the keychain, leaving the legacy plaintext token behind.
    set.mockImplementation((key: string, value: string) => {
      if (key === GLOBAL_CONFIG_KEY) throw new Error('database is locked')
      values.set(key, value)
    })

    new GlobalConfigManager(store, makeKeychainTokenStore()).load()

    // The flag must stay unset so the next launch retries; otherwise the
    // plaintext token below is stranded in preferences forever.
    expect(values.get(MIGRATION_FLAG_KEY)).toBeUndefined()
    expect(values.get('taskTracker.token.aaaaaaaa')).toBe('legacy-plaintext-token')
  })

  it('retries the migration on a later run after a failure', () => {
    const { store, values, set } = makePreferencesStore({
      'taskTracker.connections': legacyConnections,
      'taskTracker.token.aaaaaaaa': 'legacy-plaintext-token',
    })
    set.mockImplementationOnce((key: string, value: string) => {
      if (key === GLOBAL_CONFIG_KEY) throw new Error('database is locked')
      values.set(key, value)
    })

    new GlobalConfigManager(store, makeKeychainTokenStore()).load()
    expect(values.get(MIGRATION_FLAG_KEY)).toBeUndefined()

    // Second construction simulates the next app launch, with the transient
    // failure gone.
    new GlobalConfigManager(store, makeKeychainTokenStore()).load()

    expect(values.get(MIGRATION_FLAG_KEY)).toBe('1')
    expect(values.get(GLOBAL_CONFIG_KEY)).toBeDefined()
    expect(values.has('taskTracker.token.aaaaaaaa')).toBe(false)
  })
})
