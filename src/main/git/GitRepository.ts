import { ok, err, okAsync, type Result, type ResultAsync } from 'neverthrow'
import simpleGit from 'simple-git'
import type { GitError } from './errors'
import type { ParsedDiff } from './types'
import { parseDiff } from './diffParser'
import { fromExternalCall, errorMessage } from '../errors'

function validateRef(name: string): Result<string, GitError> {
  if (name.startsWith('-')) return err({ _tag: 'InvalidRef', ref: name })
  return ok(name)
}

function gitErr(command: string, e: unknown): GitError {
  return { _tag: 'GitCommandFailed', command, message: errorMessage(e) }
}

function gitCall<T>(command: string, promise: Promise<T>): ResultAsync<T, GitError> {
  return fromExternalCall(promise, (e) => gitErr(command, e))
}

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
  static detect(dirPath: string): ResultAsync<GitInfo, GitError> {
    const git = simpleGit(dirPath)
    return fromExternalCall(git.revparse(['--show-toplevel']), () => ({
      _tag: 'NotAGitRepo' as const,
      path: dirPath,
    })).andThen((raw) => {
      const repoRoot = raw.trim()
      return GitRepository.getBranch(repoRoot).andThen((branch) =>
        GitRepository.listWorktrees(repoRoot).andThen((worktrees) =>
          GitRepository.isDirty(repoRoot).andThen((isDirty) =>
            GitRepository.getAheadBehind(repoRoot).map((aheadBehind) => ({
              isGitRepo: true as const,
              repoRoot,
              branch,
              worktrees,
              isDirty,
              aheadBehind,
            })),
          ),
        ),
      )
    })
  }

  static getBranch(repoRoot: string): ResultAsync<string | null, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('rev-parse', git.revparse(['--abbrev-ref', 'HEAD'])).map((raw) => {
      const branch = raw.trim()
      return branch === 'HEAD' ? null : branch
    })
  }

  static getRemoteUrl(repoRoot: string, remote = 'origin'): ResultAsync<string, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('remote get-url', git.raw(['remote', 'get-url', remote])).map((raw) =>
      raw.trim(),
    )
  }

  static listWorktrees(repoRoot: string): ResultAsync<GitWorktreeInfo[], GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('worktree list', git.raw(['worktree', 'list', '--porcelain'])).map(
      parseWorktreeOutput,
    )
  }

  static isDirty(repoRoot: string): ResultAsync<boolean, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('status', git.status()).map((status) => status.files.length > 0)
  }

  static getAheadBehind(
    repoRoot: string,
  ): ResultAsync<{ ahead: number; behind: number } | null, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall(
      'rev-list',
      git.raw(['rev-list', '--left-right', '--count', 'HEAD...@{upstream}']),
    ).map((raw) => {
      const parts = raw.trim().split(/\s+/)
      if (parts.length === 2) {
        return { ahead: parseInt(parts[0], 10), behind: parseInt(parts[1], 10) }
      }
      return null
    })
  }

  // --- Write operations ---

  static commit(
    repoRoot: string,
    message: string,
    stageAll?: boolean,
  ): ResultAsync<GitCommitResult, GitError> {
    const git = simpleGit(repoRoot)
    const doStage = stageAll ? gitCall('add', git.add('-A')) : okAsync<unknown, GitError>(undefined)
    return doStage.andThen(() =>
      gitCall('commit', git.commit(message)).map((result) => ({
        hash: result.commit || '',
        summary: result.summary
          ? `${result.summary.changes} changed, ${result.summary.insertions} insertions, ${result.summary.deletions} deletions`
          : 'Committed',
      })),
    )
  }

  static push(repoRoot: string): ResultAsync<{ branch: string; remote: string }, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('push', git.push()).map((result) => ({
      branch: result.pushed?.[0]?.local || '',
      remote: result.remoteMessages?.all?.[0] || '',
    }))
  }

  static pull(repoRoot: string, rebase: boolean): ResultAsync<{ summary: string }, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('pull', git.pull(undefined, undefined, rebase ? { '--rebase': null } : {})).map(
      (result) => {
        const files = result.files?.length ?? 0
        return { summary: `${files} file(s) updated` }
      },
    )
  }

  static fetch(repoRoot: string): ResultAsync<void, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('fetch', git.fetch()).map(() => undefined)
  }

  static fetchAll(repoRoot: string): ResultAsync<void, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('fetch', git.fetch(['--all'])).map(() => undefined)
  }

  static stash(repoRoot: string): ResultAsync<void, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('stash', git.stash()).map(() => undefined)
  }

  static stashPop(repoRoot: string): ResultAsync<void, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('stash pop', git.stash(['pop'])).map(() => undefined)
  }

  static listBranches(repoRoot: string): ResultAsync<GitBranchList, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('branch', git.branch(['-a'])).map((result) => {
      const local: string[] = []
      const remote: string[] = []

      for (const [name, info] of Object.entries(result.branches)) {
        if (name.startsWith('remotes/')) {
          if (!name.endsWith('/HEAD')) {
            remote.push(name.replace('remotes/', ''))
          }
        } else {
          local.push(info.name)
        }
      }

      return { local, remote, current: result.current || null }
    })
  }

  static createBranch(
    repoRoot: string,
    name: string,
    baseBranch: string,
  ): ResultAsync<void, GitError> {
    return validateRef(name)
      .andThen(() => validateRef(baseBranch))
      .asyncAndThen(() => {
        const git = simpleGit(repoRoot)
        return gitCall('branch', git.raw(['branch', name, baseBranch]))
      })
      .map(() => undefined)
  }

  static checkout(repoRoot: string, branch: string): ResultAsync<void, GitError> {
    return validateRef(branch)
      .asyncAndThen(() => {
        const git = simpleGit(repoRoot)
        return gitCall('checkout', git.checkout(branch))
      })
      .map(() => undefined)
  }

  static deleteBranch(repoRoot: string, name: string, force: boolean): ResultAsync<void, GitError> {
    return validateRef(name)
      .asyncAndThen(() => {
        const git = simpleGit(repoRoot)
        return gitCall('branch -d', git.branch([force ? '-D' : '-d', name]))
      })
      .map(() => undefined)
  }

  static deleteRemoteBranch(
    repoRoot: string,
    remote: string,
    name: string,
  ): ResultAsync<void, GitError> {
    return validateRef(remote)
      .andThen(() => validateRef(name))
      .asyncAndThen(() => {
        const git = simpleGit(repoRoot)
        return gitCall('push --delete', git.push(remote, name, { '--delete': null }))
      })
      .map(() => undefined)
  }

  static getPushInfo(repoRoot: string): ResultAsync<GitPushInfo | null, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('rev-parse', git.revparse(['--abbrev-ref', 'HEAD'])).andThen((raw) => {
      const branch = raw.trim()
      if (branch === 'HEAD') return okAsync<GitPushInfo | null, GitError>(null)

      return gitCall('config', git.raw(['config', `branch.${branch}.remote`])).andThen(
        (remoteRaw) => {
          const remote = remoteRaw.trim()
          if (!remote) return okAsync<GitPushInfo | null, GitError>(null)

          return gitCall(
            'rev-list',
            git.raw(['rev-list', '--count', `${remote}/${branch}..HEAD`]),
          ).map((countRaw) => {
            const commitCount = parseInt(countRaw.trim(), 10) || 0
            return { branch, remote, commitCount }
          })
        },
      )
    })
  }

  static isBranchMerged(repoRoot: string, branch: string): ResultAsync<boolean, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('branch --merged', git.raw(['branch', '--merged'])).map((raw) => {
      const merged = raw
        .split('\n')
        .map((line) => line.replace(/^\*?\s+/, '').trim())
        .filter(Boolean)
      return merged.includes(branch)
    })
  }

  static worktreeAdd(
    repoRoot: string,
    path: string,
    branch: string,
    baseBranch: string,
  ): ResultAsync<void, GitError> {
    return validateRef(branch)
      .andThen(() => validateRef(baseBranch))
      .asyncAndThen(() => {
        const git = simpleGit(repoRoot)
        return gitCall('worktree add', git.raw(['worktree', 'add', '-b', branch, path, baseBranch]))
      })
      .map(() => undefined)
  }

  static worktreeRemove(
    repoRoot: string,
    path: string,
    force: boolean,
  ): ResultAsync<void, GitError> {
    const git = simpleGit(repoRoot)
    const args = ['worktree', 'remove', path]
    if (force) args.push('--force')
    return gitCall('worktree remove', git.raw(args)).map(() => undefined)
  }

  static getUnmergedCommits(repoRoot: string, branch: string): ResultAsync<string[], GitError> {
    return validateRef(branch).asyncAndThen(() => {
      const git = simpleGit(repoRoot)
      return gitCall('log', git.raw(['log', branch, '--not', '--remotes', '--oneline'])).map(
        (raw) => raw.trim().split('\n').filter(Boolean),
      )
    })
  }

  static getStatusPorcelain(
    repoRoot: string,
    worktreePath?: string,
  ): ResultAsync<string, GitError> {
    const git = simpleGit(worktreePath ?? repoRoot)
    return gitCall('status', git.raw(['status', '--porcelain']))
  }

  static getDiff(repoRoot: string): ResultAsync<string, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('diff', git.diff(['--cached'])).andThen((staged) => {
      if (staged.trim()) return okAsync<string, GitError>(staged)
      return gitCall('diff', git.diff())
    })
  }

  static getDiffParsed(repoRoot: string): ResultAsync<ParsedDiff, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('diff', git.diff(['HEAD']))
      .orElse((e) => {
        if (e._tag === 'GitCommandFailed') {
          return gitCall('diff', git.diff())
        }
        return okAsync<string, GitError>('')
      })
      .map((raw) => parseDiff(raw))
  }

  static getFileDiff(repoRoot: string, filePath: string): ResultAsync<ParsedDiff, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('diff', git.diff(['HEAD', '--', filePath]))
      .orElse((e) => {
        if (e._tag === 'GitCommandFailed') {
          return gitCall('diff', git.diff(['--', filePath]))
        }
        return okAsync<string, GitError>('')
      })
      .map((raw) => parseDiff(raw))
  }

  static stageFile(repoRoot: string, filePath: string): ResultAsync<void, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('add', git.add(filePath)).map(() => undefined)
  }

  static revertFile(repoRoot: string, filePath: string): ResultAsync<void, GitError> {
    const git = simpleGit(repoRoot)
    return gitCall('checkout', git.checkout(['--', filePath])).map(() => undefined)
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
