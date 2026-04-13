import { app } from 'electron'
import { ok, err, errAsync, okAsync, type Result, type ResultAsync } from 'neverthrow'
import { match, P } from 'ts-pattern'
import type { Database } from '../db/Database'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { CredentialStore } from '../db/CredentialStore'
import type { ToolRegistry } from '../tools/ToolRegistry'
import type { ProfileStore } from '../profiles/ProfileStore'
import { KNOWN_AGENT_TYPES, type AgentType, type ProfilePrefs } from '../profiles/types'
import {
  SETTINGS_EXPORT_VERSION,
  type ExportFile,
  type ExportedCredential,
  type ExportedCustomTool,
  type ExportedProfile,
  type ImportCounts,
} from './types'
import type { SettingsExportError } from './errors'

const KNOWN_AGENT_SET = new Set<string>(KNOWN_AGENT_TYPES)

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export class SettingsExportService {
  constructor(
    private database: Database,
    private preferencesStore: PreferencesStore,
    private profileStore: ProfileStore,
    private credentialStore: CredentialStore,
    private toolRegistry: ToolRegistry,
  ) {}

  buildExport(): ResultAsync<ExportFile, SettingsExportError> {
    try {
      const profiles: ExportedProfile[] = this.profileStore.listInternal().map((p) => ({
        agentType: p.agentType,
        name: p.name,
        isDefault: p.isDefault,
        sortIndex: p.sortIndex,
        prefs: p.prefs,
        apiKey: p.apiKey,
      }))

      const credentials: ExportedCredential[] = this.credentialStore
        .listInternalDecrypted()
        .map((c) => ({
          domain: c.domain,
          username: c.username,
          title: c.title,
          password: c.password,
        }))

      const customTools: ExportedCustomTool[] = this.toolRegistry.listCustom().map((t) => ({
        id: t.id,
        name: t.name,
        command: t.command,
        args: t.args,
        icon: t.icon,
        category: t.category,
      }))

      const file: ExportFile = {
        version: SETTINGS_EXPORT_VERSION,
        exportedAt: new Date().toISOString(),
        appVersion: app.getVersion(),
        preferences: this.preferencesStore.getAllDecrypted(),
        profiles,
        credentials,
        customTools,
      }
      return okAsync(file)
    } catch (e) {
      return errAsync({
        _tag: 'ExportReadError',
        reason: e instanceof Error ? e.message : String(e),
      })
    }
  }

  applyImport(raw: unknown): ResultAsync<ImportCounts, SettingsExportError> {
    const parsed = this.validate(raw)
    if (parsed.isErr()) return errAsync(parsed.error)
    const file = parsed.value

    const counts: ImportCounts = {
      preferences: 0,
      profiles: 0,
      credentials: 0,
      customTools: 0,
    }

    const txn = this.database.db.transaction(() => {
      counts.preferences = this.preferencesStore.setMany(file.preferences)
      counts.profiles = this.profileStore.upsertForImport(file.profiles)
      counts.credentials = this.credentialStore.upsertForImport(file.credentials)
      counts.customTools = this.toolRegistry.upsertCustomForImport(file.customTools)
    })

    try {
      txn()
    } catch (e) {
      return errAsync({
        _tag: 'ImportWriteError',
        reason: e instanceof Error ? e.message : String(e),
      })
    }
    return okAsync(counts)
  }

  private validate(raw: unknown): Result<ExportFile, SettingsExportError> {
    if (!isRecord(raw)) {
      return err({ _tag: 'ImportValidationError', reason: 'top-level value is not an object' })
    }

    if (typeof raw.version !== 'number') {
      return err({ _tag: 'ImportValidationError', reason: 'missing or non-numeric "version"' })
    }
    if (raw.version !== SETTINGS_EXPORT_VERSION) {
      return err({
        _tag: 'ImportVersionMismatch',
        found: raw.version,
        expected: SETTINGS_EXPORT_VERSION,
      })
    }

    if (!isRecord(raw.preferences)) {
      return err({ _tag: 'ImportValidationError', reason: '"preferences" must be an object' })
    }
    const preferences: Record<string, string> = {}
    for (const [key, value] of Object.entries(raw.preferences)) {
      if (typeof value !== 'string') {
        return err({
          _tag: 'ImportValidationError',
          reason: `preferences["${key}"] must be a string`,
        })
      }
      preferences[key] = value
    }

    const profilesResult = this.validateProfiles(raw.profiles)
    if (profilesResult.isErr()) return err(profilesResult.error)

    const credentialsResult = this.validateCredentials(raw.credentials)
    if (credentialsResult.isErr()) return err(credentialsResult.error)

    const toolsResult = this.validateCustomTools(raw.customTools)
    if (toolsResult.isErr()) return err(toolsResult.error)

    return ok({
      version: SETTINGS_EXPORT_VERSION,
      exportedAt: typeof raw.exportedAt === 'string' ? raw.exportedAt : '',
      appVersion: typeof raw.appVersion === 'string' ? raw.appVersion : '',
      preferences,
      profiles: profilesResult.value,
      credentials: credentialsResult.value,
      customTools: toolsResult.value,
    })
  }

  private validateProfiles(raw: unknown): Result<ExportedProfile[], SettingsExportError> {
    if (!Array.isArray(raw)) {
      return err({ _tag: 'ImportValidationError', reason: '"profiles" must be an array' })
    }
    const out: ExportedProfile[] = []
    for (const [i, entry] of raw.entries()) {
      if (!isRecord(entry)) {
        return err({ _tag: 'ImportValidationError', reason: `profiles[${i}] is not an object` })
      }
      if (typeof entry.agentType !== 'string' || !KNOWN_AGENT_SET.has(entry.agentType)) {
        return err({
          _tag: 'ImportValidationError',
          reason: `profiles[${i}].agentType is not a known agent type`,
        })
      }
      if (typeof entry.name !== 'string' || entry.name.trim() === '') {
        return err({ _tag: 'ImportValidationError', reason: `profiles[${i}].name is empty` })
      }
      if (!isRecord(entry.prefs)) {
        return err({
          _tag: 'ImportValidationError',
          reason: `profiles[${i}].prefs must be an object`,
        })
      }
      const apiKey = match(entry.apiKey)
        .with(P.string, (s) => s)
        .with(null, () => null)
        .with(undefined, () => null)
        .otherwise(() => null)

      out.push({
        agentType: entry.agentType as AgentType,
        name: entry.name,
        isDefault: entry.isDefault === true,
        sortIndex: typeof entry.sortIndex === 'number' ? entry.sortIndex : 0,
        prefs: entry.prefs as ProfilePrefs,
        apiKey,
      })
    }
    return ok(out)
  }

  private validateCredentials(raw: unknown): Result<ExportedCredential[], SettingsExportError> {
    if (!Array.isArray(raw)) {
      return err({ _tag: 'ImportValidationError', reason: '"credentials" must be an array' })
    }
    const out: ExportedCredential[] = []
    for (const [i, entry] of raw.entries()) {
      if (!isRecord(entry)) {
        return err({ _tag: 'ImportValidationError', reason: `credentials[${i}] is not an object` })
      }
      if (typeof entry.domain !== 'string' || entry.domain === '') {
        return err({ _tag: 'ImportValidationError', reason: `credentials[${i}].domain is empty` })
      }
      if (typeof entry.username !== 'string' || entry.username === '') {
        return err({
          _tag: 'ImportValidationError',
          reason: `credentials[${i}].username is empty`,
        })
      }
      if (typeof entry.password !== 'string') {
        return err({
          _tag: 'ImportValidationError',
          reason: `credentials[${i}].password must be a string`,
        })
      }
      out.push({
        domain: entry.domain,
        username: entry.username,
        password: entry.password,
        title: typeof entry.title === 'string' ? entry.title : '',
      })
    }
    return ok(out)
  }

  private validateCustomTools(raw: unknown): Result<ExportedCustomTool[], SettingsExportError> {
    if (!Array.isArray(raw)) {
      return err({ _tag: 'ImportValidationError', reason: '"customTools" must be an array' })
    }
    const out: ExportedCustomTool[] = []
    for (const [i, entry] of raw.entries()) {
      if (!isRecord(entry)) {
        return err({ _tag: 'ImportValidationError', reason: `customTools[${i}] is not an object` })
      }
      if (typeof entry.id !== 'string' || entry.id === '') {
        return err({ _tag: 'ImportValidationError', reason: `customTools[${i}].id is empty` })
      }
      if (typeof entry.name !== 'string') {
        return err({
          _tag: 'ImportValidationError',
          reason: `customTools[${i}].name must be a string`,
        })
      }
      if (typeof entry.command !== 'string') {
        return err({
          _tag: 'ImportValidationError',
          reason: `customTools[${i}].command must be a string`,
        })
      }
      const args =
        Array.isArray(entry.args) && entry.args.every((a) => typeof a === 'string')
          ? (entry.args as string[])
          : []
      out.push({
        id: entry.id,
        name: entry.name,
        command: entry.command,
        args,
        icon: typeof entry.icon === 'string' ? entry.icon : 'terminal',
        category: typeof entry.category === 'string' ? entry.category : 'system',
      })
    }
    return ok(out)
  }
}
