import { watch } from 'chokidar'
import { join } from 'path'
import type { FSWatcher } from 'chokidar'
import { GitRepository } from './GitRepository'
import type { GitInfo } from './GitRepository'

export class GitWatcher {
  private watcher: FSWatcher | null = null
  private debounceTimer: ReturnType<typeof setTimeout> | null = null
  private readonly debounceMs = 300

  constructor(
    private repoRoot: string,
    private onChange: (info: GitInfo) => void,
  ) {}

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

    this.watcher.on('all', () => this.scheduleRefresh())
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

  private scheduleRefresh(): void {
    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer)
    }
    this.debounceTimer = setTimeout(async () => {
      this.debounceTimer = null
      try {
        const info = await GitRepository.detect(this.repoRoot)
        this.onChange(info)
      } catch {
        // Ignore errors from git commands during refresh
      }
    }, this.debounceMs)
  }
}
