import { relative, sep } from 'path'
import * as watcher from '@parcel/watcher'
import { ResultAsync, okAsync } from 'neverthrow'
import { fromExternalCall, errorMessage } from '../errors'
import type { FileWatcherError } from './errors'

/**
 * Granular change event emitted to renderers.
 *
 * `path` is relative to the watched root, using forward slashes for
 * consistency across platforms (matches the format the renderer uses to
 * build paths from `rootPath`).
 */
export interface FileChangeEvent {
  type: 'add' | 'change' | 'unlink'
  path: string
}

const DEBOUNCE_MS = 50

function mapEventType(type: watcher.Event['type']): FileChangeEvent['type'] {
  switch (type) {
    case 'create':
      return 'add'
    case 'update':
      return 'change'
    case 'delete':
      return 'unlink'
  }
}

function toRelative(root: string, absPath: string): string {
  const rel = relative(root, absPath)
  // Normalize Windows backslashes to forward slashes so the renderer can
  // split on '/' the same way as entries returned from fs:readDir.
  return sep === '\\' ? rel.split(sep).join('/') : rel
}

/**
 * Watches a workspace root recursively for filesystem changes.
 *
 * Uses `@parcel/watcher` which delegates to native OS APIs (FSEvents on
 * macOS, inotify on Linux, ReadDirectoryChangesW on Windows). Events are
 * coalesced in a {@link DEBOUNCE_MS}ms window and deduplicated by
 * `path+type` before being delivered to `onChange`.
 *
 * Lifecycle is driven by callers: construct, `start()`, optionally
 * `updateIgnorePatterns()`, then `stop()`. The class is safe to start/stop
 * multiple times but only one subscription is active at a time.
 */
export class FileTreeWatcher {
  private subscription: watcher.AsyncSubscription | null = null
  private debounceTimer: ReturnType<typeof setTimeout> | null = null
  private pendingEvents = new Map<string, FileChangeEvent>()
  private ignorePatterns: string[]

  constructor(
    private readonly repoRoot: string,
    ignorePatterns: readonly string[],
    private readonly onChange: (events: FileChangeEvent[]) => void,
  ) {
    this.ignorePatterns = [...ignorePatterns]
  }

  start(): ResultAsync<void, FileWatcherError> {
    if (this.subscription) return okAsync(undefined)

    return fromExternalCall(
      watcher.subscribe(this.repoRoot, this.handleEvents, {
        ignore: this.ignorePatterns,
      }),
      (e): FileWatcherError => ({
        _tag: 'WatchStartFailed',
        path: this.repoRoot,
        message: errorMessage(e),
      }),
    ).map((sub) => {
      this.subscription = sub
    })
  }

  stop(): ResultAsync<void, FileWatcherError> {
    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer)
      this.debounceTimer = null
    }
    this.pendingEvents.clear()

    const sub = this.subscription
    if (!sub) return okAsync(undefined)
    this.subscription = null

    return fromExternalCall(
      sub.unsubscribe(),
      (e): FileWatcherError => ({
        _tag: 'WatchStopFailed',
        path: this.repoRoot,
        message: errorMessage(e),
      }),
    )
  }

  /**
   * Restart the watcher with a new ignore list. Used when the user edits
   * patterns in Preferences — callers should await this before emitting
   * further events.
   */
  updateIgnorePatterns(patterns: readonly string[]): ResultAsync<void, FileWatcherError> {
    this.ignorePatterns = [...patterns]
    return this.stop().andThen(() => this.start())
  }

  private handleEvents = (err: Error | null, events: watcher.Event[]): void => {
    if (err) {
      // Native watcher can report transient errors (e.g. directory
      // temporarily unavailable, permission denied, inotify limit hit).
      // Log so the failure is visible in DevTools, then drop the batch —
      // next event will retry.
      console.warn(`[FileTreeWatcher] native error for ${this.repoRoot}:`, err.message)
      return
    }

    for (const ev of events) {
      const relPath = toRelative(this.repoRoot, ev.path)
      if (!relPath) continue
      const mapped: FileChangeEvent = { type: mapEventType(ev.type), path: relPath }
      // Dedup by path+type: if the same event fires multiple times in a
      // single debounce window we only keep the last one.
      this.pendingEvents.set(`${mapped.type}:${mapped.path}`, mapped)
    }

    if (this.debounceTimer) return
    this.debounceTimer = setTimeout(() => {
      this.debounceTimer = null
      const batch = [...this.pendingEvents.values()]
      this.pendingEvents.clear()
      if (batch.length > 0) this.onChange(batch)
    }, DEBOUNCE_MS)
  }
}
