import { match } from 'ts-pattern'

export type FileSystemError =
  | { _tag: 'NotFound'; path: string }
  | { _tag: 'PermissionDenied'; path: string }
  | { _tag: 'StaleWrite'; actualMtimeMs: number }
  | { _tag: 'WriteFailed'; message: string }
  | { _tag: 'StatFailed'; message: string }

export function fileSystemErrorMessage(error: FileSystemError): string {
  return match(error)
    .with({ _tag: 'NotFound' }, (e) => `File not found: ${e.path}`)
    .with({ _tag: 'PermissionDenied' }, (e) => `Permission denied: ${e.path}`)
    .with(
      { _tag: 'StaleWrite' },
      (e) => `File changed on disk (mtime ${e.actualMtimeMs}) — reload before saving`,
    )
    .with({ _tag: 'WriteFailed' }, (e) => `Failed to write file: ${e.message}`)
    .with({ _tag: 'StatFailed' }, (e) => `Failed to stat file: ${e.message}`)
    .exhaustive()
}

export function isStaleWriteMessage(message: string): boolean {
  return message.startsWith('File changed on disk')
}
