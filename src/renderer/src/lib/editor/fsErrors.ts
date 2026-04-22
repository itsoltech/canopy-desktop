// Mirrors the StaleWrite discriminant produced by main/ipc/fsErrors.ts.
// Keep the prefix in sync with `fileSystemErrorMessage` over there —
// the main process flattens typed FileSystemError via unwrapOrThrow
// before the error crosses IPC, so the renderer can only identify it
// by message prefix.
const STALE_WRITE_PREFIX = 'File changed on disk'

export function isStaleWriteError(err: unknown): boolean {
  const message = err instanceof Error ? err.message : String(err)
  return message.startsWith(STALE_WRITE_PREFIX)
}
