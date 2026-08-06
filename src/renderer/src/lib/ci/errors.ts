/**
 * Presentation of failures that reach the renderer as thrown IPC errors.
 *
 * `ipcRenderer.invoke` rejects with Electron's own wrapper around the main-process
 * message, so every CI failure arrived on screen carrying transport plumbing the user
 * has no use for and cannot act on.
 */

// Electron's exact shape: `Error invoking remote method 'ci:activity': Error: <message>`.
// The trailing `Error:` is the serialized error's own class name, repeated for every
// nested rethrow, so strip it as many times as it appears.
const IPC_WRAPPER_RE = /^Error invoking remote method '[^']*':\s*/
const ERROR_CLASS_PREFIX_RE = /^(?:[A-Za-z]*Error):\s*/

/** The message worth showing: the main process's own words, with the transport removed. */
export function ipcErrorMessage(error: unknown, fallback = 'Request failed'): string {
  const raw = error instanceof Error ? error.message : typeof error === 'string' ? error : ''
  let message = raw.replace(IPC_WRAPPER_RE, '')
  let previous = ''
  while (message !== previous) {
    previous = message
    message = message.replace(ERROR_CLASS_PREFIX_RE, '')
  }
  return message.trim() || fallback
}

// Same intent as the task tracker's AUTH_ERROR_RE: the status is lost crossing IPC as a
// string, and a rejected token is the one CI failure with a concrete user action attached.
const AUTH_FAILURE_RE = /\b(401|403)\b|unauthoriz|could not authenticate|invalid token|forbidden/i

/** True when the reason is the server refusing our credentials rather than a transient fault. */
export function isCiAuthFailure(message: string | undefined | null): boolean {
  return !!message && AUTH_FAILURE_RE.test(message)
}
