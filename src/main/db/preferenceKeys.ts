const ENCRYPTED_KEYS = new Set([
  'claude.apiKey',
  'gemini.apiKey',
  'opencode.apiKey',
  'codex.apiKey',
  'worktrees.baseDir.trustedResolved',
  // deviceId is the sole auth factor for trusted-device auto-accept, so this list must be
  // encrypted at rest and excluded from the renderer-facing preferences blob.
  'remote.trustedDevices',
])

const ENCRYPTED_KEY_PREFIXES = ['taskTracker.token.', 'credential.secret.v2.']

/** Machine-bound state must not be restored elsewhere, where it would corrupt local state. */
const NON_EXPORTABLE_KEYS = new Set([
  'app.lastSeenVersion',
  'openWindowConfigs',
  'telemetry.lastPingDate',
  'remote.lastPort',
  'remote.trustedDevices',
  'taskTracker.migratedToGlobalConfig',
  'worktrees.baseDir.trustedResolved',
])

const NON_EXPORTABLE_PREFIXES = [
  'workspace:',
  'taskTracker.token.',
  'credential.registry.',
  'credential.bindings.',
  'credential.secret.',
]

const MAIN_PROCESS_ONLY_PREFIXES = ['credential.registry.', 'credential.bindings.']

export function isEncryptedPreferenceKey(key: string): boolean {
  if (ENCRYPTED_KEYS.has(key)) return true
  return ENCRYPTED_KEY_PREFIXES.some((prefix) => key.startsWith(prefix))
}

export function isExportablePreferenceKey(key: string): boolean {
  if (NON_EXPORTABLE_KEYS.has(key)) return false
  return !NON_EXPORTABLE_PREFIXES.some((prefix) => key.startsWith(prefix))
}

/** Secrets and credential authorization metadata must never cross renderer preference IPC. */
export function isMainProcessOnlyPreferenceKey(key: string): boolean {
  return (
    isEncryptedPreferenceKey(key) ||
    MAIN_PROCESS_ONLY_PREFIXES.some((prefix) => key.startsWith(prefix))
  )
}
