import { watch } from 'chokidar'
import { join } from 'path'
import type { FSWatcher } from 'chokidar'
import { GitRepository } from './GitRepository'
import type { GitInfo } from './GitRepository'
import { gitErrorMessage } from './errors'

export interface GitRefreshFlags {
  branch: boolean
  worktrees: boolean
  dirty: boolean
  aheadBehind: boolean
}

export class GitWatcher {
  private watcher: FSWatcher | null = null
  private debounceTimer: ReturnType<typeof setTimeout> | null = null
  private readonly debounceMs = 300
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

  start(): void {
    if (this.watcher) return

    const gitDir = join(this.repoRoot, '.git')
    const watchPaths = [
      join(gitDir, 'HEAD'),
      join(gitDir, 'index'),
      join(gitDir, 'refs'),
      join(gitDir, 'worktrees'),
    ]

    this.watcher = watch(watchPaths, {
      ignoreInitial: true,
      depth: 2,
      awaitWriteFinish: { stabilityThreshold: 100 },
    })

    this.watcher.on('all', (_event, changedPath) => this.scheduleRefresh(changedPath))
  }

  stop(): void {
    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer)
      this.debounceTimer = null
    }
    if (this.watcher) {
      this.watcher.close()
      this.watcher = null
    }
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
    const gitDir = join(this.repoRoot, '.git')
    const headPath = join(gitDir, 'HEAD')
    const indexPath = join(gitDir, 'index')
    const refsPath = join(gitDir, 'refs')
    const worktreesPath = join(gitDir, 'worktrees')

    if (changedPath === indexPath) {
      this.pendingRefresh.dirty = true
      return
    }

    if (changedPath === headPath) {
      this.pendingRefresh.branch = true
      this.pendingRefresh.aheadBehind = true
      return
    }

    if (changedPath.startsWith(refsPath)) {
      this.pendingRefresh.branch = true
      this.pendingRefresh.worktrees = true
      this.pendingRefresh.aheadBehind = true
      return
    }

    if (changedPath.startsWith(worktreesPath)) {
      this.pendingRefresh.branch = true
      this.pendingRefresh.worktrees = true
      this.pendingRefresh.dirty = true
      this.pendingRefresh.aheadBehind = true
      return
    }

    this.pendingRefresh.branch = true
    this.pendingRefresh.worktrees = true
    this.pendingRefresh.dirty = true
    this.pendingRefresh.aheadBehind = true
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
