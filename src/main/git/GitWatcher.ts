import { join, sep } from 'path'
import * as watcher from '@parcel/watcher'
import { ResultAsync, okAsync } from 'neverthrow'
import { GitRepository } from './GitRepository'
import type { GitInfo } from './GitRepository'
import { fromExternalCall, errorMessage } from '../errors'
import { gitErrorMessage } from './errors'
import type { GitError } from './errors'

export interface GitRefreshFlags {
  branch: boolean
  worktrees: boolean
  dirty: boolean
  aheadBehind: boolean
}

/**
 * Watches `.git/` for metadata changes (HEAD, index, refs, worktrees) and
 * refreshes the cached `GitInfo`. Uses `@parcel/watcher` (FSEvents / inotify
 * / ReadDirectoryChangesW) — same stack as `FileTreeWatcher`.
 *
 * Subscribes to the entire `.git/` directory and filters events by path in
 * the callback. The high-noise subdirs (`objects`, `logs`, `hooks`) are
 * ignored natively to keep the event volume low.
 *
 * Refreshes are debounced 300ms; multiple flag-bumping events in the same
 * window collapse into a single `GitRepository` query batch.
 */
export class GitWatcher {
  private subscription: watcher.AsyncSubscription | null = null
  private debounceTimer: ReturnType<typeof setTimeout> | null = null
  private readonly debounceMs = 300
  /**
   * Grace window after a completed refresh during which `.git/index` events
   * are ignored. Our own `isDirty()` / `git diff` calls stat the index and
   * trigger native events; without this guard the watcher ends up in a
   * tight loop (refresh → stat → event → refresh…).
   */
  private readonly selfTriggeredGraceMs = 500
  private lastRefreshCompletedAt = 0
  private pendingRefresh: GitRefreshFlags = {
    branch: false,
    worktrees: false,
    dirty: false,
    aheadBehind: false,
  }
  private lastInfo: GitInfo | null

  constructor(
    private repoRoot: string,
    private onChange: (info: GitInfo, changes: GitRefreshFlags) => void,
    initialInfo?: GitInfo,
  ) {
    this.lastInfo = initialInfo ?? null
  }

  start(): ResultAsync<void, GitError> {
    if (this.subscription) return okAsync(undefined)

    const gitDir = join(this.repoRoot, '.git')
    return fromExternalCall(
      watcher.subscribe(gitDir, this.handleEvents, {
        // Skip write-heavy / irrelevant subdirs to keep event volume low.
        ignore: ['objects', 'logs', 'hooks', 'lfs', 'modules'],
      }),
      (e): GitError => ({
        _tag: 'WatcherStartFailed',
        path: gitDir,
        message: errorMessage(e),
      }),
    ).map((sub) => {
      this.subscription = sub
    })
  }

  async stop(): Promise<void> {
    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer)
      this.debounceTimer = null
    }
    const sub = this.subscription
    if (!sub) return
    this.subscription = null
    try {
      await sub.unsubscribe()
    } catch {
      // Ignore — best-effort teardown
    }
  }

  private handleEvents = (err: Error | null, events: watcher.Event[]): void => {
    if (err) {
      console.warn(`[GitWatcher] native error for ${this.repoRoot}:`, err.message)
      return
    }

    const indexPath = join(this.repoRoot, '.git', 'index')
    const now = Date.now()
    const inGraceWindow = now - this.lastRefreshCompletedAt < this.selfTriggeredGraceMs

    for (const ev of events) {
      // `ev.path` and the comparison targets in `isRelevantGitPath` /
      // `markPendingRefresh` are all built with `path.join()`, so they share
      // the platform's native separator. No normalization needed.
      const changedPath = ev.path
      if (!this.isRelevantGitPath(changedPath)) continue
      // Skip index events triggered by our own refresh (stat refresh loop)
      if (inGraceWindow && changedPath === indexPath) continue
      this.scheduleRefresh(changedPath)
    }
  }

  /**
   * Returns true only for paths we actually care about: `HEAD`, `index`,
   * anything under `refs/` or `worktrees/`. Everything else in `.git/` —
   * `*.lock`, `COMMIT_EDITMSG`, `FETCH_HEAD`, `ORIG_HEAD`, `packed-refs`,
   * `config`, `info/`, … — is transient or irrelevant to the cached
   * `GitInfo` and would otherwise trigger refresh loops (each refresh
   * touches `.git/index` stat which triggers another event, ad infinitum).
   */
  private isRelevantGitPath(changedPath: string): boolean {
    const gitDir = join(this.repoRoot, '.git')
    const headPath = join(gitDir, 'HEAD')
    const indexPath = join(gitDir, 'index')
    const refsPrefix = join(gitDir, 'refs') + sep
    const worktreesPrefix = join(gitDir, 'worktrees') + sep

    // Skip lock files and other transient intermediates
    if (changedPath.endsWith('.lock')) return false

    if (changedPath === headPath) return true
    if (changedPath === indexPath) return true
    if (changedPath.startsWith(refsPrefix)) return true
    if (changedPath.startsWith(worktreesPrefix)) return true
    return false
  }

  private scheduleRefresh(changedPath: string): void {
    this.markPendingRefresh(changedPath)

    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer)
    }
    this.debounceTimer = setTimeout(async () => {
      this.debounceTimer = null
      try {
        const changes = this.consumePendingRefresh()
        const info = await this.refreshInfo(changes)
        this.onChange(info, changes)
      } catch {
        // Ignore errors from git commands during refresh
      } finally {
        this.lastRefreshCompletedAt = Date.now()
      }
    }, this.debounceMs)
  }

  private consumePendingRefresh(): GitRefreshFlags {
    const changes = { ...this.pendingRefresh }
    this.pendingRefresh = {
      branch: false,
      worktrees: false,
      dirty: false,
      aheadBehind: false,
    }
    return changes
  }

  private markPendingRefresh(changedPath: string): void {
    // `handleEvents` already filtered via `isRelevantGitPath`, so we know
    // the path is one of HEAD, index, refs/*, or worktrees/*.
    const gitDir = join(this.repoRoot, '.git')
    const headPath = join(gitDir, 'HEAD')
    const indexPath = join(gitDir, 'index')
    const refsPrefix = join(gitDir, 'refs') + sep
    const worktreesPrefix = join(gitDir, 'worktrees') + sep

    if (changedPath === indexPath) {
      this.pendingRefresh.dirty = true
      return
    }

    if (changedPath === headPath) {
      this.pendingRefresh.branch = true
      this.pendingRefresh.worktrees = true
      this.pendingRefresh.aheadBehind = true
      return
    }

    if (changedPath.startsWith(refsPrefix)) {
      this.pendingRefresh.branch = true
      this.pendingRefresh.worktrees = true
      this.pendingRefresh.aheadBehind = true
      return
    }

    if (changedPath.startsWith(worktreesPrefix)) {
      this.pendingRefresh.branch = true
      this.pendingRefresh.worktrees = true
      this.pendingRefresh.dirty = true
      this.pendingRefresh.aheadBehind = true
    }
  }

  private async refreshInfo(changes: GitRefreshFlags): Promise<GitInfo> {
    if (!this.lastInfo) {
      const result = await GitRepository.detect(this.repoRoot)
      if (result.isErr()) throw new Error(gitErrorMessage(result.error))
      this.lastInfo = result.value
      return result.value
    }

    const [branch, worktrees, isDirty, aheadBehind] = await Promise.all([
      changes.branch
        ? GitRepository.getBranch(this.repoRoot).unwrapOr(this.lastInfo.branch)
        : Promise.resolve(this.lastInfo.branch),
      changes.worktrees
        ? GitRepository.listWorktrees(this.repoRoot).unwrapOr(this.lastInfo.worktrees)
        : Promise.resolve(this.lastInfo.worktrees),
      changes.dirty
        ? GitRepository.isDirty(this.repoRoot).unwrapOr(this.lastInfo.isDirty)
        : Promise.resolve(this.lastInfo.isDirty),
      changes.aheadBehind
        ? GitRepository.getAheadBehind(this.repoRoot).unwrapOr(this.lastInfo.aheadBehind)
        : Promise.resolve(this.lastInfo.aheadBehind),
    ])

    this.lastInfo = {
      ...this.lastInfo,
      isGitRepo: true,
      repoRoot: this.repoRoot,
      branch,
      worktrees,
      isDirty,
      aheadBehind,
    }

    return this.lastInfo
  }
}
