import type { HostSession } from './hosts'
import type { ArtifactRef } from './schema'
import { uniqueId } from './commands'
import { toBase64, fromBase64 } from './protobuf'
export async function downloadArtifact(host: HostSession, artifact: ArtifactRef, signal?: AbortSignal): Promise<{ id: string; uri: string }> {
  if (artifact.host_id !== host.snapshot().hostId || BigInt(artifact.size) > 64n * 1024n * 1024n) throw Error('Artifact cannot be previewed on this device (64 MiB limit)')
  const id = uniqueId(), native = host.transport.native
  await native.beginAttachment(id)
  try {
    let offset = 0n, complete = false
    const stream = await host.transport.subscribe('ArtifactsService/Download', { artifact, offset: '0' }, async chunk => {
      if (complete || chunk.artifact?.host_id !== artifact.host_id || chunk.artifact.artifact_id !== artifact.artifact_id || chunk.offset !== String(offset) || chunk.data.length > 65536 || offset + BigInt(chunk.data.length) > BigInt(artifact.size)) throw Error('Invalid artifact stream')
      await native.appendAttachment(id, toBase64(chunk.data)); offset += BigInt(chunk.data.length); complete = chunk.complete
    }, signal)
    await stream.done
    if (!complete || offset !== BigInt(artifact.size)) throw Error('Incomplete artifact')
    const source = await native.finishAttachment(id, artifact.media_type)
    if (source.sha256 !== toBase64(artifact.sha256)) throw Error('Artifact checksum mismatch')
    return { id, uri: await native.attachmentUri(id) }
  } catch (error) { await native.removeAttachment(id); throw error }
}

export async function readChatPage(host: HostSession, chat: import('./schema').ThreadRef, item: import('./schema').ItemKey, signal?: AbortSignal): Promise<{ id: string; uri: string; html: string }> {
  if (chat.host_id !== host.snapshot().hostId) throw Error('Page belongs to another host')
  const id = uniqueId(), native = host.transport.native
  await native.beginAttachment(id)
  try {
    let artifact: ArtifactRef | undefined, offset = 0n, complete = false
    const stream = await host.transport.subscribe('CatalogService/ReadThreadPage', { thread: chat, item }, async chunk => {
      const current = chunk.artifact
      if (!current || current.host_id !== chat.host_id || current.media_type !== 'text/html' || current.sha256.length !== 32 || BigInt(current.size) > 32n * 1024n * 1024n || complete || chunk.offset !== String(offset) || chunk.data.length > 65536 || (!chunk.data.length && !chunk.complete)) throw Error('Invalid page stream')
      if (artifact && (artifact.artifact_id !== current.artifact_id || artifact.size !== current.size || toBase64(artifact.sha256) !== toBase64(current.sha256))) throw Error('Page identity changed')
      artifact = current; offset += BigInt(chunk.data.length)
      if (offset > BigInt(current.size)) throw Error('Page exceeds declared size')
      await native.appendAttachment(id, toBase64(chunk.data)); complete = chunk.complete
    }, signal)
    await stream.done
    if (signal?.aborted || !artifact || !complete || offset !== BigInt(artifact.size)) throw Error('Incomplete page')
    const source = await native.finishAttachment(id, 'text/html')
    if (source.sha256 !== toBase64(artifact.sha256)) throw Error('Page checksum mismatch')
    const bytes = new Uint8Array(Number(offset))
    for (let position = 0; position < bytes.length; position += 65536) {
      const part = await native.readAttachment(id, position, Math.min(65536, bytes.length - position))
      bytes.set(fromBase64(part), position)
    }
    return { id, uri: await native.attachmentUri(id), html: new TextDecoder('utf-8', { fatal: true }).decode(bytes) }
  } catch (error) { await native.removeAttachment(id); throw error }
}

export async function attachmentText(host: HostSession, id: string, size: number): Promise<string> {
  if (size > 32 * 1024 * 1024) throw Error('Text preview exceeds 32 MiB')
  const bytes = new Uint8Array(size)
  for (let offset = 0; offset < size; offset += 65536) {
    const length = Math.min(65536, size - offset), part = fromBase64(await host.transport.native.readAttachment(id, offset, length))
    if (part.length !== length) throw Error('Incomplete local preview')
    bytes.set(part, offset)
  }
  return new TextDecoder('utf-8', { fatal: true }).decode(bytes)
}
