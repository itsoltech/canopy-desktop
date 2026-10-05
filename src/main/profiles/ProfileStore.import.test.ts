import { beforeEach, describe, expect, it, vi } from 'vitest'
// Real SQLite via node:sqlite (better-sqlite3 is built for the Electron ABI and cannot load
// under vitest's Node runtime); the statements ProfileStore runs are plain SQLite.
import { DatabaseSync } from 'node:sqlite'
import type { Database } from '../db/Database'
import type { PreferencesStore } from '../db/PreferencesStore'
import { buildMigrations } from '../db/migrations'
import { ProfileStore } from './ProfileStore'

vi.mock('electron', () => ({
  safeStorage: {
    isEncryptionAvailable: () => false,
    encryptString: vi.fn(),
    decryptString: vi.fn(),
  },
}))

function storedKey(db: DatabaseSync, name: string): string | null {
  const row = db
    .prepare("SELECT api_key_enc FROM agent_profiles WHERE agent_type = 'claude' AND name = ?")
    .get(name) as { api_key_enc: string | null }
  return row.api_key_enc === null ? null : Buffer.from(row.api_key_enc, 'base64').toString()
}

describe('ProfileStore.upsertForImport', () => {
  let db: DatabaseSync
  let store: ProfileStore

  beforeEach(() => {
    db = new DatabaseSync(':memory:')
    for (const migration of buildMigrations(false)) db.exec(migration.up)
    // ProfileStore only touches `database.db` with prepare/get/run on these paths.
    store = new ProfileStore({ db } as unknown as Database, {} as PreferencesStore)
    store.upsertForImport([
      { agentType: 'claude', name: 'Default', prefs: { model: 'opus' }, apiKey: 'sk-local' },
    ])
  })

  it('keeps the stored API key when the imported profile carries none', () => {
    const count = store.upsertForImport([
      { agentType: 'claude', name: 'Default', prefs: { model: 'sonnet' }, apiKey: null },
    ])

    expect(count).toBe(1)
    expect(storedKey(db, 'Default')).toBe('sk-local')
    const prefs = db
      .prepare("SELECT prefs_json FROM agent_profiles WHERE agent_type = 'claude' AND name = ?")
      .get('Default') as { prefs_json: string }
    expect(JSON.parse(prefs.prefs_json)).toEqual({ model: 'sonnet' })
  })

  it('replaces the stored API key when the import provides one', () => {
    store.upsertForImport([
      { agentType: 'claude', name: 'Default', prefs: {}, apiKey: 'sk-imported' },
    ])

    expect(storedKey(db, 'Default')).toBe('sk-imported')
  })

  it('creates new profiles without a key when none is imported', () => {
    store.upsertForImport([{ agentType: 'claude', name: 'Work', prefs: {}, apiKey: null }])

    expect(storedKey(db, 'Work')).toBeNull()
  })
})
