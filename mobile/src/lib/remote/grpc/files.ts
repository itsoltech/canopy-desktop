import { attachmentText } from './artifact-preview'
import type { HostSession } from './hosts'
import type { FileRef } from './schema'
import { command, checkReceipt, uniqueId } from './commands'
import { uploadText } from './uploads'
import { encode, toBase64 } from './protobuf'
const sameFile = (a: FileRef | undefined, b: FileRef) => !!a && toBase64(encode('FileRef', a)) === toBase64(encode('FileRef', b))
export async function readTextFile(host: HostSession, file: FileRef, signal?: AbortSignal): Promise<{ text: string; hash: Uint8Array }> {
  let offset = 0, hash: Uint8Array | undefined, complete = false
  const chunks: Uint8Array[] = []
  const hashId = uniqueId(), native = host.transport.native
  await native.startHash(hashId)
  try {
  const stream = await host.transport.subscribe('FilesService/ReadFile', { file, offset: '0', max_bytes: String(16 * 1024 * 1024) }, async chunk => {
    if (complete || !sameFile(chunk.file, file) || BigInt(chunk.offset) !== BigInt(offset)) throw Error('File stream identity or offset changed')
    if (hash && toBase64(hash) !== toBase64(chunk.base_hash)) throw Error('File changed during read')
    hash = chunk.base_hash; offset += chunk.data.length
    if (offset > 16 * 1024 * 1024) throw Error('Editor file exceeds 16 MiB')
    await native.hashChunk(hashId, toBase64(chunk.data))
    chunks.push(chunk.data); complete = chunk.complete
  }, signal)
  await stream.done
  if (signal?.aborted) throw Error('Read cancelled')
  if (!complete || !hash || hash.length !== 32) throw Error('Incomplete file read')
  if (await native.finishHash(hashId) !== toBase64(hash)) throw Error('File checksum mismatch')
  const bytes = new Uint8Array(offset); let position = 0
  for (const chunk of chunks) { bytes.set(chunk, position); position += chunk.length }
  return { text: new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes), hash }
  } finally { await native.cancelHash(hashId) }
}
// Read and validate before a single durable replacement. Failure leaves the old draft.
export async function reloadTextFile(host: HostSession, file: FileRef, signal: AbortSignal, unchanged: () => boolean, viewKey?: string) {
  const result = await readTextFile(host, file, signal)
  await host.store.update(state => {
    if (signal.aborted || !unchanged()) throw Error('Reload cancelled or buffer changed; draft retained')
    const key = viewKey ?? `file:${toBase64(encode('ResourceTarget', file.target!))}:${file.relative_path}`
    ;(state.views ??= {})[key] = { text: result.text, baseHash: toBase64(result.hash), newline: result.text.includes('\r\n') ? 'crlf' : 'lf' }
    return null
  })
  return result
}
export async function saveTextFile(host: HostSession, file: FileRef, text: string, hash: Uint8Array) {
  const content = await uploadText(host, { file }, text)
  return checkReceipt(await host.mutate('FilesService/SaveFile', { command: command(host.store), file, expected_hash: hash, content }))
}
export function fileMediaType(path: string): string {
  const extension = path.split('.').pop()?.toLowerCase() ?? ''
  return ({ png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', svg: 'image/svg+xml', avif: 'image/avif', heic: 'image/heic', mp4: 'video/mp4', mov: 'video/quicktime', webm: 'video/webm', mp3: 'audio/mpeg', wav: 'audio/wav', woff: 'font/woff', woff2: 'font/woff2', ttf: 'font/ttf', otf: 'font/otf', html: 'text/html', htm: 'text/html', pdf: 'application/pdf' } as Record<string, string>)[extension] ?? 'application/octet-stream'
}
export async function previewFile(host: HostSession, file: FileRef, signal?: AbortSignal): Promise<{ id: string; uri: string; mediaType: string; html?: string }> {
  const id = uniqueId(), native = host.transport.native, mediaType = fileMediaType(file.relative_path)
  await native.beginAttachment(id)
  try {
    let offset = 0n, hash: Uint8Array | undefined, complete = false
    const stream = await host.transport.subscribe('FilesService/ReadFile', { file, offset: '0', max_bytes: String(64 * 1024 * 1024) }, async chunk => {
      if (complete || !sameFile(chunk.file, file) || chunk.offset !== String(offset) || chunk.data.length > 65536) throw Error('Invalid file preview stream')
      if (hash && toBase64(hash) !== toBase64(chunk.base_hash)) throw Error('File changed during preview')
      hash = chunk.base_hash; offset += BigInt(chunk.data.length)
      if (offset > 64n * 1024n * 1024n) throw Error('File exceeds preview limit')
      await native.appendAttachment(id, toBase64(chunk.data)); complete = chunk.complete
    }, signal)
    await stream.done
    if (!complete || !hash) throw Error('Incomplete file preview')
    const source = await native.finishAttachment(id, mediaType)
    if (source.sha256 !== toBase64(hash)) throw Error('File preview checksum mismatch')
    return { id, uri: await native.attachmentUri(id), mediaType, html: mediaType === 'text/html' ? await attachmentText(host, id, source.size) : undefined }
  } catch (error) { await native.removeAttachment(id); throw error }
}
