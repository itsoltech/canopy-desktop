/**
 * Remote-control session state surfaced to the renderer (and thus to the
 * `RemoteConnectionModal` UI). Discriminated by `kind` so the renderer can
 * pattern-match on it via `ts-pattern`.
 */
export type RemoteSessionStatus =
  | { kind: 'idle' }
  | { kind: 'starting' }
  | {
      kind: 'waiting'
      pairingUrl: string
      hostname: string
      lanIp: string
      port: number
      expiresAt: number
    }
  | {
      kind: 'peerArrived'
      pairingUrl: string
      hostname: string
      lanIp: string
      port: number
      device: PendingDevice
    }
  | {
      kind: 'paired'
      hostname: string
      lanIp: string
      port: number
      deviceName: string
      connectedAt: number
    }
  | {
      kind: 'reconnecting'
      hostname: string
      lanIp: string
      port: number
      deviceName: string
      reconnectingSince: number
    }
  | { kind: 'error'; message: string }

export interface PendingDevice {
  deviceId: string
  deviceName: string
  /** Short hex prefix of the device ID, used by the accept modal as a friendly fingerprint. */
  fingerprint: string
  publicKeyJwk?: unknown
}

export interface PairingUrlInfo {
  pairingUrl: string
  hostname: string
  lanIp: string
  port: number
  expiresAt: number
}
