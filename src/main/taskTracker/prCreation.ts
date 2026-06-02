import { execFile } from 'child_process'
import { promisify } from 'util'
import { errAsync, okAsync, type ResultAsync } from 'neverthrow'
import type { TrackerTask, PRTemplateConfig, PRTargetRule } from './types'
import type { TaskTrackerError } from './errors'
import { renderPRTitle, renderPRBody, resolveTargetBranch } from './prTemplate'
import { GitRepository } from '../git/GitRepository'
import { fromExternalCall, errorMessage } from '../errors'

const execFileAsync = promisify(execFile)

export interface CreatePRParams {
  repoRoot: string
  task: TrackerTask
  sourceBranch: string
  prConfig: PRTemplateConfig
  existingBranches?: string[]
}

export interface CreatePRResult {
  url: string
  title: string
  targetBranch: string
}

function prErr(reason: string): TaskTrackerError {
  return { _tag: 'PRCreationFailed', reason }
}

function detectGhCli(): ResultAsync<true, TaskTrackerError> {
  return fromExternalCall(execFileAsync('gh', ['--version']), () =>
    prErr('GitHub CLI (gh) is not installed. Install it to create PRs automatically.'),
  ).map(() => true as const)
}

function findExistingPR(
  repoRoot: string,
  sourceBranch: string,
): ResultAsync<string | null, TaskTrackerError> {
  // Reject branch names starting with `-` so they can't be consumed as a gh
  // CLI flag. sanitizeBranchName already strips leading `-` from generated
  // names, but `sourceBranch` here can be any string passed over IPC.
  if (sourceBranch.startsWith('-')) return okAsync(null)
  return fromExternalCall(
    execFileAsync('gh', ['pr', 'view', '--json', 'url', '--jq', '.url', '--', sourceBranch], {
      cwd: repoRoot,
    }),
    () => prErr('Failed to check existing PR'),
  )
    .map((result) => result.stdout.trim() || null)
    .orElse(() => okAsync(null))
}

export function createPullRequest(
  params: CreatePRParams,
): ResultAsync<CreatePRResult, TaskTrackerError> {
  const { repoRoot, task, sourceBranch, prConfig, existingBranches } = params

  // Reject branch names that could be consumed as a gh CLI flag. sourceBranch is
  // passed as the value to `--head` below; `--` separators don't help in that
  // position. findExistingPR already performs the same check.
  if (typeof sourceBranch !== 'string' || sourceBranch.startsWith('-')) {
    return errAsync(prErr('Invalid source branch name'))
  }

  const title = renderPRTitle(prConfig.titleTemplate, task)
  const body = renderPRBody(prConfig.bodyTemplate, task)
  const targetBranch = resolveTargetBranch(
    task,
    prConfig.defaultTargetBranch,
    prConfig.targetRules,
    existingBranches,
  )

  // Same defense for the resolved target branch — it comes from the PR config
  // (defaultTargetBranch / targetRules) which a renderer or repo file can set.
  if (targetBranch.startsWith('-')) {
    return errAsync(prErr('Invalid target branch name'))
  }

  return (
    GitRepository.push(repoRoot)
      .orElse(() => okAsync({ branch: '', remote: '' }))
      // Verify gh CLI is available
      .andThen(() => detectGhCli())
      // Check if PR already exists
      .andThen(() => findExistingPR(repoRoot, sourceBranch))
      .andThen((existingUrl) => {
        if (existingUrl) {
          return okAsync<CreatePRResult, TaskTrackerError>({
            url: existingUrl,
            title,
            targetBranch,
          })
        }
        // Create new PR
        return fromExternalCall(
          execFileAsync(
            'gh',
            [
              'pr',
              'create',
              '--title',
              title,
              '--body',
              body,
              '--base',
              targetBranch,
              '--head',
              sourceBranch,
              '--assignee',
              '@me',
            ],
            { cwd: repoRoot },
          ),
          (e) => prErr(errorMessage(e)),
        ).map(
          (result): CreatePRResult => ({
            url: result.stdout.trim(),
            title,
            targetBranch,
          }),
        )
      })
  )
}

export function buildPRConfig(
  titleTemplate: string,
  bodyTemplate: string,
  defaultTargetBranch: string,
  targetRules: PRTargetRule[] = [],
): PRTemplateConfig {
  return { titleTemplate, bodyTemplate, defaultTargetBranch, targetRules }
}
