import { match } from 'ts-pattern'
import type { TaskTrackerProvider } from './types'

export type TaskTrackerError =
  | { _tag: 'ConnectionNotFound'; connectionId: string }
  | { _tag: 'AuthTokenMissing'; connectionName: string }
  | { _tag: 'CredentialUnavailable'; connectionName: string; reason: string }
  | { _tag: 'ProviderApiError'; status: number; message: string; provider: TaskTrackerProvider }
  | { _tag: 'AttachmentDownloadFailed'; filename: string; reason: string }
  | { _tag: 'ConfigNotFound'; repoRoot: string }
  | { _tag: 'ConfigParseError'; repoRoot: string; reason: string }
  | { _tag: 'ConfigWriteError'; repoRoot: string; reason: string }
  | { _tag: 'PRCreationFailed'; reason: string }
  | { _tag: 'PRLookupFailed'; reason: string }

export function taskTrackerErrorMessage(error: TaskTrackerError): string {
  return match(error)
    .with({ _tag: 'ConnectionNotFound' }, (e) => `Connection not found: ${e.connectionId}`)
    .with({ _tag: 'AuthTokenMissing' }, (e) => `No auth token for ${e.connectionName}`)
    .with(
      { _tag: 'CredentialUnavailable' },
      (e) => `Credentials unavailable for ${e.connectionName}: ${e.reason}`,
    )
    .with({ _tag: 'ProviderApiError' }, (e) => `${e.provider} API error ${e.status}: ${e.message}`)
    .with(
      { _tag: 'AttachmentDownloadFailed' },
      (e) => `Failed to download ${e.filename}: ${e.reason}`,
    )
    .with(
      { _tag: 'ConfigNotFound' },
      (e) => `Config not found at ${e.repoRoot}/.canopy/config.json`,
    )
    .with({ _tag: 'ConfigParseError' }, (e) => `Invalid config in ${e.repoRoot}: ${e.reason}`)
    .with(
      { _tag: 'ConfigWriteError' },
      (e) => `Failed to write config in ${e.repoRoot}: ${e.reason}`,
    )
    .with({ _tag: 'PRCreationFailed' }, (e) => `PR creation failed: ${e.reason}`)
    .with({ _tag: 'PRLookupFailed' }, (e) => `PR lookup failed: ${e.reason}`)
    .exhaustive()
}
