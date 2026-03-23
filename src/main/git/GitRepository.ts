import simpleGit from 'simple-git'

export interface GitCommitResult {
  hash: string
  summary: string
}

export interface GitPushInfo {
  branch: string
  remote: string
  commitCount: number
}

export interface GitBranchList {
  local: string[]
  remote: string[]
  current: string | null
}

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
        GitRepository.getAheadBehind(repoRoot),
      ])

      return { isGitRepo: true, repoRoot, branch, worktrees, isDirty, aheadBehind }
    } catch {
      return {
        isGitRepo: false,
        repoRoot: null,
        branch: null,
        worktrees: [],
        isDirty: false,
        aheadBehind: null,
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

  // --- Write operations ---

  static async commit(repoRoot: string, message: string): Promise<GitCommitResult> {
    const git = simpleGit(repoRoot)
    const result = await git.commit(message)
    return {
      hash: result.commit || '',
      summary: result.summary
        ? `${result.summary.changes} changed, ${result.summary.insertions} insertions, ${result.summary.deletions} deletions`
        : 'Committed',
    }
  }

  static async push(repoRoot: string): Promise<{ branch: string; remote: string }> {
    const git = simpleGit(repoRoot)
    const result = await git.push()
    return {
      branch: result.pushed?.[0]?.local || '',
      remote: result.remoteMessages?.all?.[0] || '',
    }
  }

  static async pull(repoRoot: string, rebase: boolean): Promise<{ summary: string }> {
    const git = simpleGit(repoRoot)
    const result = await git.pull(undefined, undefined, rebase ? { '--rebase': null } : {})
    const files = result.files?.length ?? 0
    return { summary: `${files} file(s) updated` }
  }

  static async fetch(repoRoot: string): Promise<void> {
    const git = simpleGit(repoRoot)
    await git.fetch()
  }

  static async fetchAll(repoRoot: string): Promise<void> {
    const git = simpleGit(repoRoot)
    await git.fetch(['--all'])
  }

  static async stash(repoRoot: string): Promise<void> {
    const git = simpleGit(repoRoot)
    await git.stash()
  }

  static async stashPop(repoRoot: string): Promise<void> {
    const git = simpleGit(repoRoot)
    await git.stash(['pop'])
  }

  static async listBranches(repoRoot: string): Promise<GitBranchList> {
    const git = simpleGit(repoRoot)
    const result = await git.branch(['-a'])
    const local: string[] = []
    const remote: string[] = []

    for (const [name, info] of Object.entries(result.branches)) {
      if (name.startsWith('remotes/')) {
        // Skip HEAD pointers like remotes/origin/HEAD
        if (!name.endsWith('/HEAD')) {
          remote.push(name.replace('remotes/', ''))
        }
      } else {
        local.push(info.name)
      }
    }

    return { local, remote, current: result.current || null }
  }

  static async createBranch(repoRoot: string, name: string, baseBranch: string): Promise<void> {
    const git = simpleGit(repoRoot)
    await git.raw(['branch', name, baseBranch])
  }

  static async deleteBranch(repoRoot: string, name: string, force: boolean): Promise<void> {
    const git = simpleGit(repoRoot)
    await git.branch([force ? '-D' : '-d', name])
  }

  static async deleteRemoteBranch(repoRoot: string, remote: string, name: string): Promise<void> {
    const git = simpleGit(repoRoot)
    await git.push(remote, name, { '--delete': null })
  }

  static async getPushInfo(repoRoot: string): Promise<GitPushInfo | null> {
    const git = simpleGit(repoRoot)
    try {
      const branch = (await git.revparse(['--abbrev-ref', 'HEAD'])).trim()
      if (branch === 'HEAD') return null

      const remote = (await git.raw(['config', `branch.${branch}.remote`])).trim()
      if (!remote) return null

      const countRaw = await git.raw(['rev-list', '--count', `${remote}/${branch}..HEAD`])
      const commitCount = parseInt(countRaw.trim(), 10) || 0

      return { branch, remote, commitCount }
    } catch {
      return null
    }
  }

  static async isBranchMerged(repoRoot: string, branch: string): Promise<boolean> {
    const git = simpleGit(repoRoot)
    try {
      const raw = await git.raw(['branch', '--merged'])
      const merged = raw
        .split('\n')
        .map((line) => line.replace(/^\*?\s+/, '').trim())
        .filter(Boolean)
      return merged.includes(branch)
    } catch {
      return false
    }
  }

  static async worktreeAdd(
    repoRoot: string,
    path: string,
    branch: string,
    baseBranch: string,
  ): Promise<void> {
    const git = simpleGit(repoRoot)
    await git.raw(['worktree', 'add', '-b', branch, path, baseBranch])
  }

  static async worktreeRemove(repoRoot: string, path: string, force: boolean): Promise<void> {
    const git = simpleGit(repoRoot)
    const args = ['worktree', 'remove', path]
    if (force) args.push('--force')
    await git.raw(args)
  }

  static async getUnmergedCommits(repoRoot: string, branch: string): Promise<string[]> {
    const git = simpleGit(repoRoot)
    try {
      const raw = await git.raw(['log', branch, '--not', '--remotes', '--oneline'])
      return raw.trim().split('\n').filter(Boolean)
    } catch {
      return []
    }
  }

  static async getStatusPorcelain(repoRoot: string, worktreePath?: string): Promise<string> {
    const git = simpleGit(worktreePath ?? repoRoot)
    try {
      return await git.raw(['status', '--porcelain'])
    } catch {
      return ''
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
        isBare,
      })
    }
  }

  return worktrees
}
