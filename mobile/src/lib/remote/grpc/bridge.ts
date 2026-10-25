export interface RpcEvent { id: string; kind: 'message' | 'written' | 'close'; data?: string; code?: number; message?: string }
export interface NativeRemote {
  cleanupAttachments(keep: string[]): Promise<number>
  beginAttachment(id: string): Promise<void>
  appendAttachment(id: string, base64: string): Promise<void>
  finishAttachment(id: string, mediaType: string): Promise<{ size: number; sha256: string; mediaType: string }>
  readAttachment(id: string, offset: number, length: number): Promise<string>
  attachmentUri(id: string): Promise<string>
  removeAttachment(id: string): Promise<void>
  pickFile(): Promise<{ id: string; size: number; sha256: string; mediaType: string }>
  pickImage(): Promise<{ id: string; size: number; sha256: string; mediaType: string }>
  createIdentity(ref: string): Promise<{ csr: string }>
  installIdentity(pendingRef: string, ref: string, certificateChain: string): Promise<void>
  saveSshCredential(ref: string, value: Record<string, string>): Promise<void>
  removeCredential(ref: string): Promise<void>
  connect(id: string, config: Record<string, string>): Promise<void>
  disconnect(id: string): Promise<void>
  start(host: string, id: string, path: string, deadlineMs: number): Promise<void>
  write(id: string, bytes: string): Promise<void>
  ack(id: string): Promise<void>
  finish(id: string): Promise<void>
  cancel(id: string): Promise<void>
  addListener(event: 'rpc', callback: (event: RpcEvent) => void): { remove(): void }
  startHash(id: string): Promise<void>
  hashChunk(id: string, data: string): Promise<void>
  finishHash(id: string): Promise<string>
  cancelHash(id: string): Promise<void>
  readState(key: string): Promise<string | null>
  writeState(key: string, json: string): Promise<void>
}
