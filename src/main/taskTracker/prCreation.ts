import { execFile } from 'child_process'
import { promisify } from 'util'
import { okAsync, type ResultAsync } from 'neverthrow'
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
  return fromExternalCall(
    execFileAsync('gh', ['pr', 'view', sourceBranch, '--json', 'url', '--jq', '.url'], {
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

  const title = renderPRTitle(prConfig.titleTemplate, task)
  const body = renderPRBody(prConfig.bodyTemplate, task)
  const targetBranch = resolveTargetBranch(
    task,
    prConfig.defaultTargetBranch,
    prConfig.targetRules,
    existingBranches,
  )

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
