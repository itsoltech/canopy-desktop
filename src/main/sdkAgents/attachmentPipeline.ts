import { randomUUID } from 'crypto'
import fs from 'fs'
import path from 'path'
import { app } from 'electron'
import type { Attachment, AttachmentKind, ConversationId } from './types'

const MAX_IMAGE_BYTES = 5 * 1024 * 1024 // 5 MB
const MAX_TEXT_BYTES = 200 * 1024 // 200 KB

export interface UploadAttachmentInput {
  conversationId: ConversationId
  filename: string
  mimeType: string
  kind: AttachmentKind
  /** Base64-encoded bytes (renderer reads the file, we avoid streaming IPC here). */
  dataBase64: string
}

export type UploadAttachmentError =
  | { _tag: 'too_large'; limitBytes: number }
  | { _tag: 'io_error'; message: string }
  | { _tag: 'decode_error' }

/**
 * Write an uploaded attachment to disk and return a staged Attachment record.
 * No DB row is inserted here — persistence happens when the sending message is
 * committed (see SdkAgentManager.sendMessage). The file stays on disk; the
 * conversation's hardDelete cascade removes the folder via
 * `sweepConversationAttachments`.
 */
export function uploadAttachment(input: UploadAttachmentInput): Attachment | UploadAttachmentError {
  let buffer: Buffer
  try {
    buffer = Buffer.from(input.dataBase64, 'base64')
  } catch {
    return { _tag: 'decode_error' }
  }

  const limit = input.kind === 'image' ? MAX_IMAGE_BYTES : MAX_TEXT_BYTES
  if (buffer.length > limit) {
    return { _tag: 'too_large', limitBytes: limit }
  }

  const id = randomUUID()
  const ext = path.extname(input.filename) || extFromMime(input.mimeType)
  const dir = path.join(app.getPath('userData'), 'attachments', input.conversationId)
  const target = path.join(dir, `${id}${ext}`)

  try {
    fs.mkdirSync(dir, { recursive: true })
    fs.writeFileSync(target, buffer)
  } catch (e) {
    return { _tag: 'io_error', message: e instanceof Error ? e.message : String(e) }
  }

  return {
    id,
    kind: input.kind,
    filename: input.filename,
    path: target,
    mimeType: input.mimeType,
    sizeBytes: buffer.length,
  }
}

/**
 * Best-effort cleanup for a conversation's attachment folder. Called after
 * ConversationStore.hardDelete so cascaded DB rows aren't left with orphan
 * files on disk.
 */
export function sweepConversationAttachments(conversationId: ConversationId): void {
  const dir = path.join(app.getPath('userData'), 'attachments', conversationId)
  try {
    fs.rmSync(dir, { recursive: true, force: true })
  } catch {
    // Already gone or permission issue — nothing to do.
  }
}

function extFromMime(mime: string): string {
  switch (mime) {
    case 'image/png':
      return '.png'
    case 'image/jpeg':
      return '.jpg'
    case 'image/gif':
      return '.gif'
    case 'image/webp':
      return '.webp'
    case 'text/plain':
      return '.txt'
    case 'text/markdown':
      return '.md'
    default:
      return ''
  }
}
