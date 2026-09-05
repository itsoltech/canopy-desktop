/**
 * Electron wraps every rejected IPC invoke in "Error invoking remote method 'channel': Error: …".
 * That transport noise means nothing to the user — strip it so error boxes show only the actual
 * message (e.g. the tracker's own error text).
 */
const IPC_WRAPPER_RE = /^Error invoking remote method '[^']*':\s*/
const ERROR_CLASS_PREFIX_RE = /^(?:[A-Za-z]*Error):\s*/

export function ipcErrorMessage(error: unknown, fallback = 'Unexpected error'): string {
  const raw = error instanceof Error ? error.message : typeof error === 'string' ? error : ''
  let message = raw.replace(IPC_WRAPPER_RE, '')
  let previous = ''
  while (message !== previous) {
    previous = message
    message = message.replace(ERROR_CLASS_PREFIX_RE, '')
  }
  return message.trim() || fallback
}
