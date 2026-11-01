import * as SecureStore from 'expo-secure-store'
import type { NativeRemote } from './bridge'
import { GrpcTransport } from './transport'
import { decode, fromBase64, toBase64 } from './protobuf'
import { HostRegistry, type HostConnection, type DirectRoute } from './persistence'
import type { PairingInvite, PairingStatus } from './schema'

const options = { keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY }
type Pending = { invite: string; csr: string; deviceName: string; requestId?: string; route: DirectRoute }
function pendingKey(ref: string): string {
  if (!/^[a-zA-Z0-9_-]{1,80}$/.test(ref)) throw Error('Invalid pairing reference')
  return `canopy.pairing.${ref}`
}
export function certificatePEM(der: Uint8Array): string {
  if (!der.length || der.length > 16384) throw Error('Invalid certificate size')
  return `-----BEGIN CERTIFICATE-----\n${toBase64(der).match(/.{1,64}/g)!.join('\n')}\n-----END CERTIFICATE-----\n`
}
export function parsePairingInvite(uri: string): { invite: PairingInvite; encoded: string } {
  const url = new URL(uri)
  if (url.protocol !== 'canopy:' || url.hostname !== 'pair' || url.searchParams.get('v') !== '1') throw Error('Unsupported pairing QR')
  const encoded = url.searchParams.get('data') ?? ''
  if (!/^[A-Za-z0-9_-]+$/.test(encoded) || encoded.length > 32768) throw Error('Invalid pairing QR data')
  const base64 = encoded.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - encoded.length % 4) % 4)
  const invite = decode<PairingInvite>('PairingInvite', fromBase64(base64))
  if (invite.protocol_version !== 1 || !invite.host_id || invite.server_name !== `canopy-${invite.host_id}.local` || !invite.address || invite.port < 1 || invite.port > 65535 || invite.host_certificate.length > 16384 || !invite.host_certificate.length || invite.certificate_sha256.length !== 32 || invite.one_time_token.length !== 32) throw Error('Invalid pairing identity')
  return { invite, encoded }
}
function inviteFromEncoded(encoded: string): PairingInvite {
  return parsePairingInvite(`canopy://pair?v=1&data=${encoded}`).invite
}
/** Pairing is explicitly initiated by the user; pending credentials remain in the secure store. */
export class PairingClient {
  private readonly native: NativeRemote
  readonly ref: string
  private transport?: GrpcTransport
  private polling = false
  private pending: Pending
  private readonly invite: PairingInvite
  private constructor(native: NativeRemote, ref: string, pending: Pending) {
    this.native = native; this.ref = ref; this.pending = pending; this.invite = inviteFromEncoded(pending.invite)
  }
  static async begin(native: NativeRemote, ref: string, uri: string, deviceName: string, route?: DirectRoute): Promise<PairingClient> {
    const { invite, encoded } = parsePairingInvite(uri)
    if (BigInt(invite.expires_unix) * 1000n <= BigInt(Date.now())) throw Error('Pairing QR expired')
    const key = pendingKey(ref)
    if (await SecureStore.getItemAsync(key + '.request', options) !== null) throw Error('Pending pairing exists; resume it instead')
    const identity = await native.createIdentity(ref + '-pending')
    const pending: Pending = { invite: encoded, csr: identity.csr, deviceName, route: route ?? { kind: 'direct', address: invite.address, port: invite.port } }
    try { await SecureStore.setItemAsync(key + '.request', JSON.stringify(pending), options) }
    catch (error) { await native.removeCredential(ref + '-pending'); throw error }
    return new PairingClient(native, ref, pending)
  }
  static async restore(native: NativeRemote, ref: string): Promise<PairingClient> {
    const raw = await SecureStore.getItemAsync(pendingKey(ref) + '.request', options)
    if (!raw) throw Error('Pending pairing not found')
    return new PairingClient(native, ref, JSON.parse(raw))
  }
  private async channel(): Promise<GrpcTransport> {
    if (this.transport) return this.transport
    const { invite } = this
    const transport = new GrpcTransport(this.native, `pairing-${this.ref}`)
    const route = this.pending.route
    if (!route.address || !Number.isInteger(route.port) || route.port < 1 || route.port > 65535) throw Error('Invalid pairing route')
    try {
      await transport.open({ address: route.address, port: String(route.port), serverName: invite.server_name, hostCertificate: certificatePEM(invite.host_certificate), fingerprint: Array.from(invite.certificate_sha256, b => b.toString(16).padStart(2, '0')).join('') })
    } catch (error) { await transport.dispose(); throw error }
    this.transport = transport
    return transport
  }
  async poll(signal?: AbortSignal): Promise<PairingStatus> {
    if (this.polling) throw Error('Pairing request already in progress')
    this.polling = true
    try { return await this.pollOnce(signal) } finally { this.polling = false }
  }
  private async pollOnce(signal?: AbortSignal): Promise<PairingStatus> {
    const channel = await this.channel()
    const result = this.pending.requestId
      ? await channel.unary('EnrollmentService/GetPairingStatus', { environment: { host_id: this.invite.host_id }, request_id: this.pending.requestId, proof: this.invite.one_time_token }, signal)
      : await channel.unary('EnrollmentService/RequestPairing', { environment: { host_id: this.invite.host_id }, one_time_token: this.invite.one_time_token, certificate_signing_request: fromBase64(this.pending.csr), device_name: this.pending.deviceName }, signal)
    if (!result.request_id || this.pending.requestId && result.request_id !== this.pending.requestId) throw Error('Pairing request identity mismatch')
    if (!this.pending.requestId) {
      this.pending = { ...this.pending, requestId: result.request_id }
      await SecureStore.setItemAsync(pendingKey(this.ref) + '.request', JSON.stringify(this.pending), options)
    }
    return result
  }
  async complete(status: PairingStatus): Promise<HostConnection> {
    if (status.state !== 2 || status.request_id !== this.pending.requestId || !status.device || status.device.revoked || !status.certificate || status.certificate_chain.length > 3) throw Error('Pairing is not approved')
    const key = pendingKey(this.ref)
    const certificateChain = [status.certificate, ...status.certificate_chain].map(certificatePEM).join('')
    const credentialRef = this.ref + '-device'
    await this.native.installIdentity(this.ref + '-pending', credentialRef, certificateChain)
    const connection: HostConnection = {
      hostId: this.invite.host_id, deviceId: status.device.device_id, name: this.invite.host_id,
      serverName: this.invite.server_name, fingerprint: Array.from(this.invite.certificate_sha256, b => b.toString(16).padStart(2, '0')).join(''),
      hostCertificate: certificatePEM(this.invite.host_certificate), credentialRef, routes: [this.pending.route], route: 0,
    }
    await new HostRegistry(this.native).save(connection)
    // Caller clears its durable pending navigation reference before cancel()
    // removes enrollment material. This keeps completion recoverable after a crash.
    await this.dispose()
    return connection
  }
  async cancel(): Promise<void> {
    await this.dispose()
    const key = pendingKey(this.ref)
    await SecureStore.deleteItemAsync(key + '.request', options)
    await this.native.removeCredential(this.ref + '-pending')
  }
  async dispose(): Promise<void> { const transport = this.transport; this.transport = undefined; await transport?.dispose() }
}
