import { safeStorage } from 'electron'
import { randomUUID } from 'crypto'
import { okAsync, errAsync, type ResultAsync } from 'neverthrow'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from '../db/Database'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { AgentType, PreferencesReader } from '../agents/types'
import {
  KNOWN_AGENT_TYPES,
  LEGACY_PREF_FIELDS,
  type AgentProfile,
  type AgentProfileMasked,
  type ProfileInput,
  type ProfilePrefs,
} from './types'
import type { ProfileError } from './errors'

interface ProfileRow {
  id: string
  agent_type: string
  name: string
  is_default: number
  sort_index: number
  prefs_json: string
  api_key_enc: string | null
  created_at: string
  updated_at: string
}

function isAgentType(value: string): value is AgentType {
  return (KNOWN_AGENT_TYPES as readonly string[]).includes(value)
}

export class ProfileStore {
  constructor(
    private database: Database,
    private preferencesStore: PreferencesStore,
  ) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  // --- Encryption (mirrors CredentialStore) ---

  private encrypt(plaintext: string): string {
    if (safeStorage.isEncryptionAvailable()) {
      return safeStorage.encryptString(plaintext).toString('base64')
    }
    console.warn(
      '[ProfileStore] safeStorage encryption unavailable — API keys stored without OS-level encryption.',
    )
    return Buffer.from(plaintext).toString('base64')
  }

  private decrypt(stored: string): string {
    if (safeStorage.isEncryptionAvailable()) {
      try {
        return safeStorage.decryptString(Buffer.from(stored, 'base64'))
      } catch {
        return Buffer.from(stored, 'base64').toString()
      }
    }
    return Buffer.from(stored, 'base64').toString()
  }

  // --- Row mapping ---

  private rowToProfile(row: ProfileRow): AgentProfile {
    let prefs: ProfilePrefs = {}
    try {
      prefs = JSON.parse(row.prefs_json) as ProfilePrefs
    } catch {
      prefs = {}
    }
    return {
      id: row.id,
      agentType: isAgentType(row.agent_type) ? row.agent_type : 'claude',
      name: row.name,
      isDefault: row.is_default === 1,
      sortIndex: row.sort_index,
      prefs,
      apiKey: row.api_key_enc ? this.decrypt(row.api_key_enc) : null,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    }
  }

  toMasked(profile: AgentProfile): AgentProfileMasked {
    return {
      id: profile.id,
      agentType: profile.agentType,
      name: profile.name,
      isDefault: profile.isDefault,
      sortIndex: profile.sortIndex,
      prefs: profile.prefs,
      hasApiKey: profile.apiKey !== null && profile.apiKey.length > 0,
      createdAt: profile.createdAt,
      updatedAt: profile.updatedAt,
    }
  }

  // --- Read ---

  list(agentType?: AgentType): ResultAsync<AgentProfileMasked[], ProfileError> {
    try {
      const rows = agentType
        ? (this.db
            .prepare('SELECT * FROM agent_profiles WHERE agent_type = ? ORDER BY sort_index, name')
            .all(agentType) as ProfileRow[])
        : (this.db
            .prepare('SELECT * FROM agent_profiles ORDER BY agent_type, sort_index, name')
            .all() as ProfileRow[])
      return okAsync(rows.map((r) => this.toMasked(this.rowToProfile(r))))
    } catch (e) {
      return errAsync({
        _tag: 'ProfileWriteError',
        reason: e instanceof Error ? e.message : String(e),
      })
    }
  }

  /** Internal — returns full profile WITH decrypted apiKey. Never expose via IPC. */
  getInternal(id: string): ResultAsync<AgentProfile, ProfileError> {
    try {
      const row = this.db.prepare('SELECT * FROM agent_profiles WHERE id = ?').get(id) as
        | ProfileRow
        | undefined
      if (!row) return errAsync({ _tag: 'ProfileNotFound', id })
      return okAsync(this.rowToProfile(row))
    } catch (e) {
      return errAsync({
        _tag: 'ProfileWriteError',
        reason: e instanceof Error ? e.message : String(e),
      })
    }
  }

  /** Masked variant for IPC. */
  get(id: string): ResultAsync<AgentProfileMasked | null, ProfileError> {
    return this.getInternal(id)
      .map((p) => this.toMasked(p))
      .orElse((error) => (error._tag === 'ProfileNotFound' ? okAsync(null) : errAsync(error)))
  }

  // --- Write ---

  save(input: ProfileInput): ResultAsync<AgentProfile, ProfileError> {
    const trimmedName = input.name.trim()
    if (!trimmedName) {
      return errAsync({ _tag: 'ProfileValidationError', reason: 'Name is required' })
    }
    if (!isAgentType(input.agentType)) {
      return errAsync({
        _tag: 'ProfileValidationError',
        reason: `Unknown agent type: ${input.agentType}`,
      })
    }

    try {
      // Name conflict (excluding self on update)
      const conflict = this.db
        .prepare('SELECT id FROM agent_profiles WHERE agent_type = ? AND name = ? AND id != ?')
        .get(input.agentType, trimmedName, input.id ?? '') as { id: string } | undefined
      if (conflict) {
        return errAsync({
          _tag: 'ProfileNameConflict',
          agentType: input.agentType,
          name: trimmedName,
        })
      }

      const prefsJson = JSON.stringify(input.prefs ?? {})

      if (input.id) {
        const existing = this.db
          .prepare('SELECT * FROM agent_profiles WHERE id = ?')
          .get(input.id) as ProfileRow | undefined
        if (!existing) return errAsync({ _tag: 'ProfileNotFound', id: input.id })

        // apiKey: undefined = keep, null = clear, string = replace
        let apiKeyEnc: string | null = existing.api_key_enc
        if (input.apiKey === null) {
          apiKeyEnc = null
        } else if (typeof input.apiKey === 'string' && input.apiKey.length > 0) {
          apiKeyEnc = this.encrypt(input.apiKey)
        }

        this.db
          .prepare(
            `UPDATE agent_profiles
             SET name = ?, prefs_json = ?, api_key_enc = ?, sort_index = ?, updated_at = datetime('now')
             WHERE id = ?`,
          )
          .run(trimmedName, prefsJson, apiKeyEnc, input.sortIndex ?? existing.sort_index, input.id)

        return this.getInternal(input.id)
      }

      // Insert
      const id = randomUUID()
      const apiKeyEnc =
        typeof input.apiKey === 'string' && input.apiKey.length > 0
          ? this.encrypt(input.apiKey)
          : null

      // Compute sort_index: append at end of agent group
      const maxRow = this.db
        .prepare(
          'SELECT COALESCE(MAX(sort_index), -1) as max FROM agent_profiles WHERE agent_type = ?',
        )
        .get(input.agentType) as { max: number }
      const sortIndex = input.sortIndex ?? maxRow.max + 1

      this.db
        .prepare(
          `INSERT INTO agent_profiles
             (id, agent_type, name, is_default, sort_index, prefs_json, api_key_enc)
           VALUES (?, ?, ?, 0, ?, ?, ?)`,
        )
        .run(id, input.agentType, trimmedName, sortIndex, prefsJson, apiKeyEnc)

      return this.getInternal(id)
    } catch (e) {
      return errAsync({
        _tag: 'ProfileWriteError',
        reason: e instanceof Error ? e.message : String(e),
      })
    }
  }

  delete(id: string): ResultAsync<void, ProfileError> {
    try {
      const row = this.db.prepare('SELECT agent_type FROM agent_profiles WHERE id = ?').get(id) as
        | { agent_type: string }
        | undefined
      if (!row) return errAsync({ _tag: 'ProfileNotFound', id })

      const countRow = this.db
        .prepare('SELECT COUNT(*) as count FROM agent_profiles WHERE agent_type = ?')
        .get(row.agent_type) as { count: number }
      if (countRow.count <= 1) {
        return errAsync({ _tag: 'ProfileLastDeletion', id })
      }

      this.db.prepare('DELETE FROM agent_profiles WHERE id = ?').run(id)
      return okAsync(undefined)
    } catch (e) {
      return errAsync({
        _tag: 'ProfileWriteError',
        reason: e instanceof Error ? e.message : String(e),
      })
    }
  }

  // --- Migration of legacy global prefs into Default profiles ---

  ensureDefaults(): void {
    for (const agentType of KNOWN_AGENT_TYPES) {
      const countRow = this.db
        .prepare('SELECT COUNT(*) as count FROM agent_profiles WHERE agent_type = ?')
        .get(agentType) as { count: number }
      if (countRow.count > 0) continue

      const prefs: ProfilePrefs = {}
      for (const field of LEGACY_PREF_FIELDS[agentType]) {
        const value = this.preferencesStore.get(`${agentType}.${field}`)
        if (value !== null) {
          ;(prefs as Record<string, string>)[field] = value
        }
      }

      const apiKey = this.preferencesStore.get(`${agentType}.apiKey`)
      const apiKeyEnc =
        typeof apiKey === 'string' && apiKey.length > 0 ? this.encrypt(apiKey) : null

      this.db
        .prepare(
          `INSERT INTO agent_profiles
             (id, agent_type, name, is_default, sort_index, prefs_json, api_key_enc)
           VALUES (?, ?, 'Default', 1, 0, ?, ?)`,
        )
        .run(randomUUID(), agentType, JSON.stringify(prefs), apiKeyEnc)
    }
  }
}

/**
 * Build a PreferencesReader that exposes a profile's values for `${agentType}.*`
 * keys, falling back to the global `preferences` table for everything else.
 *
 * This is the seam: adapters keep reading `claude.apiKey` etc. exactly as today;
 * the shim translates to profile fields. Zero changes needed in adapters.
 */
export function profileToReader(
  profile: AgentProfile,
  fallback: PreferencesReader,
): PreferencesReader {
  const prefix = `${profile.agentType}.`
  return {
    get(key: string): string | null {
      if (!key.startsWith(prefix)) return fallback.get(key)
      const field = key.slice(prefix.length)
      if (field === 'apiKey') return profile.apiKey
      const value = (profile.prefs as Record<string, unknown>)[field]
      return typeof value === 'string' ? value : null
    },
  }
}
