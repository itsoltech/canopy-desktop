export type ConnectionStatus = 'reconnecting' | 'disconnected'

export const connectionStatus: Record<string, ConnectionStatus> = $state({})

export function setConnectionStatus(sessionId: string, status: ConnectionStatus): void {
  connectionStatus[sessionId] = status
}

export function clearConnectionStatus(sessionId: string): void {
  // Called for every PTY output chunk: skip the proxy write (and the reactive
  // version bump it causes) when there is nothing to clear.
  if (sessionId in connectionStatus) delete connectionStatus[sessionId]
}
