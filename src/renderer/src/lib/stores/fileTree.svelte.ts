import { SvelteMap } from 'svelte/reactivity'

interface DirEntry {
  name: string
  isDirectory: boolean
  size: number
}

export const fileTree = createFileTreeStore()

// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
function createFileTreeStore() {
  let expandedDirs: Record<string, DirEntry[]> = $state({})
  let selectedFilePath: string | null = $state(null)
  let rootPath: string | null = $state(null)
  const gitFileStatus = new SvelteMap<string, string>()
  let refreshTimer: ReturnType<typeof setTimeout> | null = null

  async function expandDir(dirPath: string): Promise<void> {
    try {
      const entries = await window.api.readDir(dirPath)
      expandedDirs[dirPath] = entries
    } catch (e) {
      // Show empty rather than leaving in collapsed state
      expandedDirs[dirPath] = []
      console.warn('readDir failed:', dirPath, e)
    }
  }

  function collapseDir(dirPath: string): void {
    const prefix = dirPath + '/'
    for (const key of Object.keys(expandedDirs)) {
      if (key === dirPath || key.startsWith(prefix)) {
        delete expandedDirs[key]
      }
    }
  }

  async function toggleDir(dirPath: string): Promise<void> {
    if (expandedDirs[dirPath]) {
      collapseDir(dirPath)
    } else {
      await expandDir(dirPath)
    }
  }

  function selectFile(filePath: string): void {
    selectedFilePath = filePath
  }

  async function refreshGitStatus(repoRoot: string): Promise<void> {
    if (!rootPath) return
    try {
      const previousPaths = [...gitFileStatus.keys()]
      const porcelain = await window.api.gitStatusPorcelain(repoRoot, rootPath)
      const nextStatuses: Record<string, string> = {}
      // eslint-disable-next-line svelte/prefer-svelte-reactivity
      const affectedPaths = new Set<string>()

      const collectPaths = (rawPath: string): void => {
        for (const part of rawPath.split(' -> ')) {
          const normalized = part.trim()
          if (normalized) affectedPaths.add(normalized)
        }
      }

      gitFileStatus.clear()
      for (const line of porcelain.split('\n')) {
        if (line.length < 4) continue
        const xy = line.substring(0, 2)
        const fp = line.substring(3)
        const status = xy[0] !== ' ' && xy[0] !== '?' ? xy[0] : xy[1]
        const normalizedStatus = status === '?' ? '?' : status
        nextStatuses[fp] = normalizedStatus
        gitFileStatus.set(fp, normalizedStatus)
        collectPaths(fp)
      }

      for (const prevPath of previousPaths) {
        if (!(prevPath in nextStatuses)) {
          affectedPaths.add(prevPath)
        }
      }

      // eslint-disable-next-line svelte/prefer-svelte-reactivity
      const dirsToRefresh = new Set<string>()
      for (const relPath of affectedPaths) {
        let currentDir = rootPath
        if (expandedDirs[currentDir]) dirsToRefresh.add(currentDir)

        const segments = relPath.split('/').slice(0, -1)
        for (const segment of segments) {
          currentDir = `${currentDir}/${segment}`
          if (expandedDirs[currentDir]) {
            dirsToRefresh.add(currentDir)
          }
        }
      }

      if (dirsToRefresh.size > 0) {
        await Promise.all([...dirsToRefresh].map((dirPath) => expandDir(dirPath)))
      }
    } catch {
      // Git status unavailable
    }
  }

  function refreshAll(repoRoot: string): void {
    if (refreshTimer) clearTimeout(refreshTimer)
    refreshTimer = setTimeout(async () => {
      refreshTimer = null
      // Re-read all expanded directories (branch/worktree switches can
      // add or remove files that don't show up in git status porcelain)
      const dirs = Object.keys(expandedDirs)
      if (dirs.length > 0) {
        await Promise.all(dirs.map((dirPath) => expandDir(dirPath)))
      }
      await refreshGitStatus(repoRoot)
    }, 300)
  }

  function reset(newRoot: string | null): void {
    if (refreshTimer) clearTimeout(refreshTimer)
    refreshTimer = null
    expandedDirs = {}
    selectedFilePath = null
    gitFileStatus.clear()
    rootPath = newRoot
  }

  return {
    get expandedDirs() {
      return expandedDirs
    },
    get selectedFilePath() {
      return selectedFilePath
    },
    get rootPath() {
      return rootPath
    },
    get gitFileStatus() {
      return gitFileStatus
    },
    expandDir,
    collapseDir,
    toggleDir,
    selectFile,
    refreshAll,
    refreshGitStatus,
    reset,
  }
}
