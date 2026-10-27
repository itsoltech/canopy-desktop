import type { TaskTrackerConnection } from './types'

/**
 * Preference key holding a legacy connection's own token, or null when the connection does not
 * point at it. Connections live in the renderer-writable `taskTracker.connections` preference, so
 * `authPrefKey` is untrusted: following it blindly would read, overwrite or delete any stored
 * secret (agent API keys, credential-registry secrets) and send it to the connection's base URL.
 * Connection ids are UUIDs — a `:` would alias the `<provider>:<baseUrl>` tracker-token keys.
 */
export function legacyConnectionTokenKey(
  connection: Pick<TaskTrackerConnection, 'id' | 'authPrefKey'>,
): string | null {
  if (typeof connection.id !== 'string' || !connection.id || connection.id.includes(':')) {
    return null
  }
  const key = `taskTracker.token.${connection.id}`
  return connection.authPrefKey === key ? key : null
}
