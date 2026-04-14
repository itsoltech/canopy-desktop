import { useCallback, useSyncExternalStore } from 'react'

import {
  connect as connectSession,
  disconnect as disconnectSession,
  getSessionState,
  subscribeSession,
  type SessionState,
} from '@/lib/remote/session'
import type { SavedInstance } from '@/lib/storage/saved-instances-types'

/**
 * React-subscribable view over the `session.ts` singleton. Returns the
 * current session state plus callable `connect` / `disconnect` actions.
 *
 * The singleton design means multiple components watching this hook all
 * see the same session — connecting from the instance detail screen keeps
 * the connection warm while the user navigates into the terminal screen.
 */
export function useRemoteSession(): {
  state: SessionState
  connect: (instance: SavedInstance) => Promise<void>
  disconnect: () => Promise<void>
} {
  const state = useSyncExternalStore(subscribeSession, getSessionState, getSessionState)

  const connect = useCallback((instance: SavedInstance) => connectSession(instance), [])
  const disconnect = useCallback(() => disconnectSession(), [])

  return { state, connect, disconnect }
}
