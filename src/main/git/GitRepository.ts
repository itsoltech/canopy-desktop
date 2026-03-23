import simpleGit from 'simple-git'

export interface GitInfo {
  isGitRepo: boolean
  repoRoot: string | null
  branch: string | null
  worktrees: GitWorktreeInfo[]
  isDirty: boolean
  aheadBehind: { ahead: number; behind: number } | null
}

export interface GitWorktreeInfo {
  path: string
  head: string
  branch: string
  isMain: boolean
  isBare: boolean
}

export class GitRepository {
  static async detect(dirPath: string): Promise<GitInfo> {
    const git = simpleGit(dirPath)

    try {
      const repoRoot = (await git.revparse(['--show-toplevel'])).trim()
      const [branch, worktrees, isDirty, aheadBehind] = await Promise.all([
        GitRepository.getBranch(repoRoot),
        GitRepository.listWorktrees(repoRoot),
        GitRepository.isDirty(repoRoot),
        GitRepository.getAheadBehind(repoRoot)
      ])

      return { isGitRepo: true, repoRoot, branch, worktrees, isDirty, aheadBehind }
    } catch {
      return {
        isGitRepo: false,
        repoRoot: null,
        branch: null,
        worktrees: [],
        isDirty: false,
        aheadBehind: null
      }
    }
  }

  static async getBranch(repoRoot: string): Promise<string | null> {
    const git = simpleGit(repoRoot)
    try {
      const branch = (await git.revparse(['--abbrev-ref', 'HEAD'])).trim()
      return branch === 'HEAD' ? null : branch
    } catch {
      return null
    }
  }

  static async listWorktrees(repoRoot: string): Promise<GitWorktreeInfo[]> {
    const git = simpleGit(repoRoot)
    try {
      const raw = await git.raw(['worktree', 'list', '--porcelain'])
      return parseWorktreeOutput(raw)
    } catch {
      return []
    }
  }

  static async isDirty(repoRoot: string): Promise<boolean> {
    const git = simpleGit(repoRoot)
    try {
      const status = await git.status()
      return status.files.length > 0
    } catch {
      return false
    }
  }

  static async getAheadBehind(repoRoot: string): Promise<{ ahead: number; behind: number } | null> {
    const git = simpleGit(repoRoot)
    try {
      const raw = await git.raw(['rev-list', '--left-right', '--count', 'HEAD...@{upstream}'])
      const parts = raw.trim().split(/\s+/)
      if (parts.length === 2) {
        return { ahead: parseInt(parts[0], 10), behind: parseInt(parts[1], 10) }
      }
      return null
    } catch {
      return null
    }
  }
}

function parseWorktreeOutput(raw: string): GitWorktreeInfo[] {
  const worktrees: GitWorktreeInfo[] = []
  const stanzas = raw.trim().split('\n\n')

  for (let i = 0; i < stanzas.length; i++) {
    const stanza = stanzas[i]
    if (!stanza.trim()) continue

    const lines = stanza.split('\n')
    let path = ''
    let head = ''
    let branch = ''
    let isBare = false

    for (const line of lines) {
      if (line.startsWith('worktree ')) {
        path = line.slice('worktree '.length)
      } else if (line.startsWith('HEAD ')) {
        head = line.slice('HEAD '.length)
      } else if (line.startsWith('branch ')) {
        const ref = line.slice('branch '.length)
        branch = ref.replace('refs/heads/', '')
      } else if (line === 'detached') {
        branch = '(detached)'
      } else if (line === 'bare') {
        isBare = true
      }
    }

    if (path) {
      worktrees.push({
        path,
        head,
        branch: branch || '(unknown)',
        isMain: i === 0,
        isBare
      })
    }
  }

  return worktrees
}
