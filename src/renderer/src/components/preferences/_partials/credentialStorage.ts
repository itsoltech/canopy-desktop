import { match } from 'ts-pattern'

// Credentials are encrypted at rest by Electron safeStorage, whose backend differs per OS. These
// helpers describe WHERE/HOW the token is stored, accurately for the running platform, so the
// wording stays consistent across confirm dialogs and the storage note.

/** OS-specific name of the encryption backend safeStorage uses. */
export function credentialStorageMechanism(platform: string): string {
  return match(platform)
    .with('win32', () => 'Windows DPAPI')
    .with('darwin', () => 'your macOS Keychain')
    .otherwise(() => 'your system keyring')
}

/** "stored ..." clause, OS-aware and honest about the no-keyring fallback. */
export function credentialStorageClause(platform: string, encryptionAvailable: boolean): string {
  return encryptionAvailable
    ? `encrypted via ${credentialStorageMechanism(platform)}`
    : 'unencrypted (no OS keyring is available on this machine)'
}
