import { match } from 'ts-pattern'

export type FileWatcherError =
  | { _tag: 'WatchStartFailed'; path: string; message: string }
  | { _tag: 'WatchStopFailed'; path: string; message: string }

export function fileWatcherErrorMessage(error: FileWatcherError): string {
  return match(error)
    .with({ _tag: 'WatchStartFailed' }, (e) => `Failed to start watcher at ${e.path}: ${e.message}`)
    .with({ _tag: 'WatchStopFailed' }, (e) => `Failed to stop watcher at ${e.path}: ${e.message}`)
    .exhaustive()
}
