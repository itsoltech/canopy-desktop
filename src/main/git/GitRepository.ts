import { ok, err, okAsync, type Result, type ResultAsync } from 'neverthrow'
import simpleGit from 'simple-git'
import { readFile } from 'fs/promises'
import { join } from 'path'
import type { GitError } from './errors'
import type { ParsedDiff, DiffFile } from './types'
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

function buildUntrackedDiffFile(
  repoRoot: string,
  filePath: string,
): ResultAsync<DiffFile, GitError> {
  return fromExternalCall(readFile(join(repoRoot, filePath), 'utf-8'), (e) =>
    gitErr('readFile', e),
  ).map((content) => {
    const lines = content.split('\n')
    if (lines[lines.length - 1] === '') lines.pop()
    const changes = lines.map((line, i) => ({
      type: 'add' as const,
      content: line,
      newLine: i + 1,
    }))
    return {
      path: filePath,
      status: 'added' as const,
      hunks:
        changes.length > 0
          ? [
              {
                oldStart: 0,
                oldLines: 0,
                newStart: 1,
                newLines: changes.length,
                header: `@@ -0,0 +1,${changes.length} @@`,
                changes,
              },
            ]
          : [],
      additions: changes.length,
      deletions: 0,
    }
  })
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
      // Sub-commands use orElse so failures (empty repo, no upstream, etc.)
      // don't cause detect() to report isGitRepo: false
      return GitRepository.getBranch(repoRoot)
        .orElse((e) => {
          console.warn(`[git] getBranch failed for "${repoRoot}":`, e)
          return okAsync<string | null, GitError>(null)
        })
        .andThen((branch) =>
          GitRepository.listWorktrees(repoRoot)
            .orElse((e) => {
              console.warn(`[git] listWorktrees failed for "${repoRoot}":`, e)
              return okAsync<GitWorktreeInfo[], GitError>([])
            })
            .andThen((worktrees) =>
              GitRepository.isDirty(repoRoot)
                .orElse((e) => {
                  console.warn(`[git] isDirty failed for "${repoRoot}":`, e)
                  return okAsync<boolean, GitError>(false)
                })
                .andThen((isDirty) =>
                  GitRepository.getAheadBehind(repoRoot)
                    // No warn — getAheadBehind fails routinely when there is no upstream
                    .orElse(() => okAsync<{ ahead: number; behind: number } | null, GitError>(null))
                    .map((aheadBehind) => ({
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
    return fromExternalCall(
      git.revparse(['--abbrev-ref', '@{u}']).then(
        () => git.push(),
        () => git.push(['-u', 'origin', 'HEAD']),
      ),
      (e) => gitErr('push', e),
    ).map((result) => ({
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

  static worktreeAddCheckout(
    repoRoot: string,
    path: string,
    branch: string,
    createLocalTracking: boolean,
  ): ResultAsync<void, GitError> {
    return validateRef(branch)
      .asyncAndThen(() => {
        const git = simpleGit(repoRoot)

        if (!createLocalTracking) {
          return gitCall('worktree add', git.raw(['worktree', 'add', path, branch]))
        }

        const slash = branch.indexOf('/')
        if (slash < 0) {
          return gitCall('worktree add', git.raw(['worktree', 'add', path, branch]))
        }
        const localName = branch.slice(slash + 1)

        return validateRef(localName).asyncAndThen(() =>
          gitCall(
            'show-ref',
            git.raw(['show-ref', '--verify', '--quiet', `refs/heads/${localName}`]),
          )
            .map(() => 'local-exists' as const)
            .orElse(() => okAsync('remote-only' as const))
            .andThen((kind) =>
              kind === 'local-exists'
                ? gitCall('worktree add', git.raw(['worktree', 'add', path, localName]))
                : gitCall(
                    'worktree add',
                    git.raw(['worktree', 'add', '-b', localName, path, branch]),
                  ),
            ),
        )
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

    const trackedDiff = gitCall('diff', git.diff(['HEAD']))
      .orElse((e) => {
        if (e._tag === 'GitCommandFailed') return gitCall('diff', git.diff())
        return okAsync<string, GitError>('')
      })
      .map((raw) => parseDiff(raw))

    const untrackedFiles = gitCall(
      'ls-files',
      git.raw(['ls-files', '--others', '--exclude-standard']),
    )
      .map((raw) => raw.trim().split('\n').filter(Boolean))
      .orElse(() => okAsync<string[], GitError>([]))

    return trackedDiff.andThen((parsed) =>
      untrackedFiles.andThen((files) => {
        if (files.length === 0) return okAsync<ParsedDiff, GitError>(parsed)

        return fromExternalCall(
          Promise.all(
            files.map((file) =>
              buildUntrackedDiffFile(repoRoot, file).match(
                (f) => f as DiffFile | null,
                () => null,
              ),
            ),
          ).then((results) => results.filter((f): f is DiffFile => f !== null)),
          (e) => gitErr('ls-files', e),
        ).map((untrackedDiffFiles) => ({
          files: [...parsed.files, ...untrackedDiffFiles],
        }))
      }),
    )
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
      .andThen((raw) => {
        if (raw.trim()) return okAsync<ParsedDiff, GitError>(parseDiff(raw))

        // No tracked diff — file may be untracked, read its content directly
        return buildUntrackedDiffFile(repoRoot, filePath).map((f) => ({ files: [f] }))
      })
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
