import type { SessionState } from './session'

/**
 * Whether the connection banner should be shown. We only surface states that
 * are actionable or worth interrupting the layout for — `awaiting-accept` (the
 * user must approve on the desktop), `error`, `reconnecting`, `disconnected`.
 * The transient `signaling`/`pairing`/`rtc` phases are conveyed by the loading
 * skeleton instead, so the banner doesn't appear-then-vanish on the happy path.
 */
export function shouldShowBanner(state: SessionState): boolean {
  switch (state.kind) {
    case 'connecting':
      return state.phase === 'awaiting-accept'
    case 'error':
    case 'reconnecting':
    case 'disconnected':
      return true
    default:
      return false
  }
}

/**
 * Maps the flat {@link SessionState} to a short human-readable banner string,
 * or `null` when no banner should show (idle / ready). Single source of truth
 * for the connection banner copy shown on the instance detail screen.
 */
export function sessionBannerText(state: SessionState): string | null {
  switch (state.kind) {
    case 'idle':
      return null
    case 'connecting':
      switch (state.phase) {
        case 'signaling':
          return 'Connecting to host…'
        case 'pairing':
          return 'Validating pairing token…'
        case 'awaiting-accept':
          return 'Waiting for approval on desktop…'
        case 'rtc':
          return 'Establishing WebRTC connection…'
      }
      return 'Connecting…'
    case 'reconnecting':
      return 'Reconnecting…'
    case 'ready':
      return null
    case 'error':
      return `Connection error: ${state.message}`
    case 'disconnected':
      return 'Disconnected'
  }
}
