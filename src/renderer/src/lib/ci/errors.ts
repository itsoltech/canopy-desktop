import { ipcErrorMessage as formatIpcErrorMessage } from '../taskTracker/ipcErrorMessage'

/** CI surfaces share the common formatter but retain their domain-specific fallback. */
export function ipcErrorMessage(error: unknown, fallback = 'Request failed'): string {
  return formatIpcErrorMessage(error, fallback)
}
