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

  async function expandDir(dirPath: string): Promise<void> {
    try {
      const entries = await window.api.readDir(dirPath)
      expandedDirs[dirPath] = entries
    } catch {
      // Directory unreadable
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
      const porcelain = await window.api.gitStatusPorcelain(repoRoot, rootPath)
      gitFileStatus.clear()
      for (const line of porcelain.split('\n')) {
        if (line.length < 4) continue
        const xy = line.substring(0, 2)
        const fp = line.substring(3)
        const status = xy[0] !== ' ' && xy[0] !== '?' ? xy[0] : xy[1]
        gitFileStatus.set(fp, status === '?' ? '?' : status)
      }
    } catch {
      // Git status unavailable
    }
  }

  async function refreshAll(repoRoot: string): Promise<void> {
    const dirs = Object.keys(expandedDirs)
    await Promise.all(dirs.map((d) => expandDir(d)))
    await refreshGitStatus(repoRoot)
  }

  function reset(newRoot: string | null): void {
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
