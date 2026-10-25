import { nativeRemote } from './native'
export type SshCredential = { sshPrivateKey: string; sshPublicKey?: string; sshPassphrase?: string } | { sshPassword: string }
// User-entered values are handed to the native vault once. There is no read/export API.
export async function saveSshCredential(ref: string, credential: SshCredential): Promise<void> {
  const value: Record<string, string> = {}
  for (const [name, text] of Object.entries(credential)) if (text !== undefined) value[name] = text
  await nativeRemote().saveSshCredential(ref, value)
}
export async function removeCredential(ref: string): Promise<void> { await nativeRemote().removeCredential(ref) }
