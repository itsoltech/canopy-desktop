import { match } from 'ts-pattern'
import type { TaskTrackerProvider } from './types'

export type TaskTrackerError =
  | { _tag: 'ConnectionNotFound'; connectionId: string }
  | { _tag: 'AuthTokenMissing'; connectionName: string }
  | { _tag: 'ProviderApiError'; status: number; message: string; provider: TaskTrackerProvider }

export function taskTrackerErrorMessage(error: TaskTrackerError): string {
  return match(error)
    .with({ _tag: 'ConnectionNotFound' }, (e) => `Connection not found: ${e.connectionId}`)
    .with({ _tag: 'AuthTokenMissing' }, (e) => `No auth token for ${e.connectionName}`)
    .with({ _tag: 'ProviderApiError' }, (e) => `${e.provider} API error ${e.status}: ${e.message}`)
    .exhaustive()
}
