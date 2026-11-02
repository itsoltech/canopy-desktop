import type { TaskTrackerConnection } from './types'

/**
 * Stored legacy tracker connections keep their token under `taskTracker.token.<id>`, and that is
 * the only preference a connection may read, write or delete. `authPrefKey` round-trips through
 * the renderer and settings import, so trusting any other value would let a tampered connection
 * send (or delete) an unrelated stored secret such as an agent API key.
 */
export function legacyConnectionTokenKey(
  connection: Pick<TaskTrackerConnection, 'id' | 'authPrefKey'>,
): string | null {
  const expected = `taskTracker.token.${connection.id}`
  return connection.authPrefKey === expected ? expected : null
}
