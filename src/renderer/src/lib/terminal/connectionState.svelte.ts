export type ConnectionStatus = 'reconnecting' | 'disconnected'

export const connectionStatus: Record<string, ConnectionStatus> = $state({})

export function setConnectionStatus(sessionId: string, status: ConnectionStatus): void {
  connectionStatus[sessionId] = status
}

export function clearConnectionStatus(sessionId: string): void {
  delete connectionStatus[sessionId]
}
