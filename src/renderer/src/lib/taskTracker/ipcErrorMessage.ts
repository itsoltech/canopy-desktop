/**
 * Electron wraps every rejected IPC invoke in "Error invoking remote method 'channel': Error: …".
 * That transport noise means nothing to the user — strip it so error boxes show only the actual
 * message (e.g. the tracker's own error text).
 */
export function ipcErrorMessage(e: unknown, fallback = 'Unexpected error'): string {
  const raw = e instanceof Error ? e.message : String(e ?? '')
  return (
    raw.replace(/^Error invoking remote method '[^']+':\s*(?:Error:\s*)?/, '').trim() || fallback
  )
}
