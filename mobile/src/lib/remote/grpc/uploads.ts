import type { HostSession } from './hosts'
import type { ArtifactRef, ResourceRef, CommandMeta } from './schema'
import { command, checkReceipt, uniqueId } from './commands'
import { fromBase64, toBase64 } from './protobuf'
/** Upload journal is durable before Begin; retries retain both command IDs and bytes. */
export type UploadJournal = { id: string; begin: CommandMeta; finish: CommandMeta; target: ResourceRef; purpose: string; size: string; hash: Uint8Array; mediaType: string; localId: string; uploadId?: string; artifact?: ArtifactRef }
export async function prepareUpload(host: HostSession, target: ResourceRef, purpose: string, localId: string, size: number, hash: Uint8Array, mediaType: string): Promise<UploadJournal> {
  if (size < 0 || size > 64 * 1024 * 1024 || hash.length !== 32) throw Error('Invalid upload source')
  const journal = { id: uniqueId(), begin: command(host.store), finish: command(host.store), target, purpose, size: String(size), hash, mediaType, localId }
  await host.store.update(state => { (state.uploads ??= []).push(journal); return null })
  return journal
}
export async function upload(host: HostSession, id: string, signal?: AbortSignal): Promise<ArtifactRef> {
  let journal = host.store.snapshot().uploads?.find(u => u.id === id)
  if (!journal) throw Error('Upload journal missing')
  if (journal.artifact) return journal.artifact
  const save = async () => { const value = journal!; await host.store.update(state => { state.uploads = state.uploads?.map(u => u.id === id ? value : u); return null }) }
  let status = journal.uploadId ? await host.transport.unary('ArtifactsService/GetUpload', { host_id: journal.begin.host_id, upload_id: journal.uploadId }, signal) : await host.transport.unary('ArtifactsService/BeginUpload', { command: journal.begin, target: journal.target, purpose: journal.purpose, size: journal.size, sha256: journal.hash, media_type: journal.mediaType }, signal)
  if (status.host_id !== journal.begin.host_id || !status.upload_id) throw Error('Upload identity mismatch')
  if (journal.uploadId && journal.uploadId !== status.upload_id) throw Error('Upload identity changed')
  journal = { ...journal, uploadId: status.upload_id }; await save()
  if (!status.artifact) {
    for (let offset = 0; offset < Number(journal.size); offset += 65536) {
      const length = Math.min(65536, Number(journal.size) - offset)
      if (status.received.some(r => BigInt(r.offset) <= BigInt(offset) && BigInt(r.offset) + BigInt(r.length) >= BigInt(offset + length))) continue
      const bytes = fromBase64(await host.transport.native.readAttachment(journal.localId, offset, length))
      if (bytes.length !== length) throw Error('Staged source changed')
      status = await host.transport.unary('ArtifactsService/PutChunk', { host_id: journal.begin.host_id, upload_id: journal.uploadId!, offset: String(offset), data: bytes }, signal)
      if (status.host_id !== journal.begin.host_id || status.upload_id !== journal.uploadId) throw Error('Chunk acknowledgement identity mismatch')
    }
    checkReceipt(await host.transport.unary('ArtifactsService/FinishUpload', { command: journal.finish, upload_id: journal.uploadId! }, signal))
    status = await host.transport.unary('ArtifactsService/GetUpload', { host_id: journal.begin.host_id, upload_id: journal.uploadId! }, signal)
  }
  const artifact = status.artifact
  if (!artifact || artifact.host_id !== journal.begin.host_id || artifact.size !== journal.size || toBase64(artifact.sha256) !== toBase64(journal.hash)) throw Error('Upload is not ready or digest changed')
  journal = { ...journal, artifact }; await save(); return artifact
}
export async function uploadText(host: HostSession, target: ResourceRef, text: string): Promise<ArtifactRef> {
  const bytes = new TextEncoder().encode(text), id = uniqueId(), native = host.transport.native
  // Native staging enforces total attachment budget and atomically publishes the source.
  await native.beginAttachment(id)
  try {
    for (let offset = 0; offset < bytes.length; offset += 65536) await native.appendAttachment(id, toBase64(bytes.subarray(offset, offset + 65536)))
    const source = await native.finishAttachment(id, 'text/plain')
    const journal = await prepareUpload(host, target, 'file-content', id, source.size, fromBase64(source.sha256), source.mediaType)
    return await upload(host, journal.id)
  } catch (error) { throw error }
}
