import { execFile } from 'child_process'
import { promisify } from 'util'
import { errAsync, okAsync, type ResultAsync } from 'neverthrow'
import type { TrackerTask, PRTemplateConfig, PRTargetRule } from './types'
import type { TaskTrackerError } from './errors'
import { renderPRTitle, renderPRBody, resolveTargetBranch } from './prTemplate'
import { GitRepository } from '../git/GitRepository'
import { fromExternalCall } from '../errors'
import { gitHubCliFailureReason, isMissingGitHubCli } from '../github/redactFailureReason'
import { isSafeGitRefName } from '../../renderer-shared/gitRef'

const execFileAsync = promisify(execFile)
export const PR_COMMAND_TIMEOUT_MS = 30_000

interface PRCommandOptions {
  cwd: string
  encoding: 'utf8'
  timeout: number
  windowsHide: true
}

const commandOptions = (repoRoot: string): PRCommandOptions => ({
  cwd: repoRoot,
  encoding: 'utf8',
  timeout: PR_COMMAND_TIMEOUT_MS,
  windowsHide: true,
})

export type PRCommandRunner = (
  repoRoot: string,
  args: string[],
  extraOptions?: { maxBuffer?: number },
) => Promise<{ stdout: string; stderr: string }>

const runGhCommand: PRCommandRunner = (repoRoot, args, extraOptions = {}) => {
  return execFileAsync('gh', args, {
    ...commandOptions(repoRoot),
    ...extraOptions,
  }) as Promise<{ stdout: string; stderr: string }>
}

export interface CreatePRParams {
  repoRoot: string
  task: TrackerTask
  sourceBranch: string
  prConfig: PRTemplateConfig
  existingBranches?: string[]
  /** User-edited values from the create-PR form; template rendering is skipped where provided. */
  overrides?: {
    title?: string
    body?: string
    targetBranch?: string
    reviewers?: string[]
    /** undefined → default `@me`; empty array → no assignee; otherwise these logins. */
    assignees?: string[]
  }
}

export interface PreparedPR {
  title: string
  body: string
  targetBranch: string
}

/** Render the PR pieces from the template WITHOUT creating anything — feeds the edit form. */
export function preparePullRequest(
  task: TrackerTask,
  prConfig: PRTemplateConfig,
  existingBranches?: string[],
): PreparedPR {
  return {
    title: renderPRTitle(prConfig.titleTemplate, task),
    body: renderPRBody(prConfig.bodyTemplate, task),
    targetBranch: resolveTargetBranch(
      task,
      prConfig.defaultTargetBranch,
      prConfig.targetRules,
      existingBranches,
    ),
  }
}

export interface CreatePRResult {
  url: string
  title: string
  targetBranch: string
}

function prErr(reason: string): TaskTrackerError {
  return { _tag: 'PRCreationFailed', reason }
}

export function prCliFailure(error: unknown): TaskTrackerError {
  return prErr(gitHubCliFailureReason(error, PR_COMMAND_TIMEOUT_MS))
}

function detectGhCli(
  repoRoot: string,
  commandRunner: PRCommandRunner,
): ResultAsync<true, TaskTrackerError> {
  return fromExternalCall(commandRunner(repoRoot, ['--version']), (error) =>
    isMissingGitHubCli(error)
      ? prErr('GitHub CLI (gh) is not installed. Install it to create PRs automatically.')
      : prCliFailure(error),
  ).map(() => true as const)
}

function findExistingPR(
  repoRoot: string,
  sourceBranch: string,
  commandRunner: PRCommandRunner,
): ResultAsync<string | null, TaskTrackerError> {
  if (!isSafeGitRefName(sourceBranch)) return okAsync(null)
  // Only an OPEN PR blocks creating another one — a branch may accumulate merged/closed PRs
  // (gh pr view would happily return those), and a new PR is legitimate then.
  return fromExternalCall(
    commandRunner(repoRoot, [
      'pr',
      'list',
      '--state',
      'open',
      '--json',
      'url',
      '--jq',
      '.[0].url // empty',
      '--head',
      sourceBranch,
    ]),
    () => prErr('Failed to check existing PR'),
  )
    .map((result) => result.stdout.trim() || null)
    .orElse(() => okAsync(null))
}

export function createPullRequest(
  params: CreatePRParams,
  commandRunner: PRCommandRunner = runGhCommand,
): ResultAsync<CreatePRResult, TaskTrackerError> {
  const { repoRoot, task, sourceBranch, prConfig, existingBranches, overrides } = params

  // Reject branch names that could be consumed as a gh CLI flag. sourceBranch is
  // passed as the value to `--head` below; `--` separators don't help in that
  // position. findExistingPR already performs the same check.
  if (!isSafeGitRefName(sourceBranch)) {
    return errAsync(prErr('Invalid source branch name'))
  }

  const rendered = preparePullRequest(task, prConfig, existingBranches)
  const title = overrides?.title?.trim() || rendered.title
  const body = overrides?.body ?? rendered.body
  const targetBranch = overrides?.targetBranch?.trim() || rendered.targetBranch
  // Reviewer/assignee logins become `--reviewer`/`--assignee` values — same leading-`-` defense
  // as branches.
  const safeLogins = (logins: string[]): string[] =>
    logins.filter((r) => typeof r === 'string' && r.trim() !== '' && !r.trim().startsWith('-'))
  const reviewers = safeLogins(overrides?.reviewers ?? [])
  const assignees = overrides?.assignees === undefined ? ['@me'] : safeLogins(overrides.assignees)

  // Same defense for the resolved target branch — it comes from the PR config
  // (defaultTargetBranch / targetRules) which a renderer or repo file can set.
  if (!isSafeGitRefName(targetBranch)) {
    return errAsync(prErr('Invalid target branch name'))
  }

  return (
    GitRepository.push(repoRoot)
      .orElse(() => okAsync({ branch: '', remote: '' }))
      // Verify gh CLI is available
      .andThen(() => detectGhCli(repoRoot, commandRunner))
      // Check if PR already exists
      .andThen(() => findExistingPR(repoRoot, sourceBranch, commandRunner))
      .andThen((existingUrl) => {
        if (existingUrl) {
          return okAsync<CreatePRResult, TaskTrackerError>({
            url: existingUrl,
            title,
            targetBranch,
          })
        }
        // Create new PR — assigned to the creating user unless the form said otherwise.
        const args = [
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
        ]
        for (const assignee of assignees) {
          args.push('--assignee', assignee.trim())
        }
        for (const reviewer of reviewers) {
          args.push('--reviewer', reviewer.trim())
        }
        return fromExternalCall(commandRunner(repoRoot, args), prCliFailure).map(
          (result): CreatePRResult => ({
            url: result.stdout.trim(),
            title,
            targetBranch,
          }),
        )
      })
  )
}

export type PRMergeStrategy = 'merge' | 'squash' | 'rebase'

const MERGE_STRATEGIES: readonly PRMergeStrategy[] = ['merge', 'squash', 'rebase']

function validPRNumber(prNumber: number): boolean {
  return Number.isInteger(prNumber) && prNumber > 0
}

/** Merge an open PR via the gh CLI. Strategy is whitelisted; gh enforces branch protections. */
export function mergePullRequest(
  repoRoot: string,
  prNumber: number,
  strategy: PRMergeStrategy,
  deleteBranch: boolean,
  commandRunner: PRCommandRunner = runGhCommand,
): ResultAsync<void, TaskTrackerError> {
  if (!validPRNumber(prNumber)) return errAsync(prErr('Invalid PR number'))
  if (!MERGE_STRATEGIES.includes(strategy)) return errAsync(prErr('Invalid merge strategy'))
  const args = ['pr', 'merge', String(prNumber), `--${strategy}`]
  if (deleteBranch) args.push('--delete-branch')
  return fromExternalCall(commandRunner(repoRoot, args), prCliFailure).map(() => undefined)
}

/** Close an open PR without merging. */
export function closePullRequest(
  repoRoot: string,
  prNumber: number,
  deleteBranch: boolean,
  commandRunner: PRCommandRunner = runGhCommand,
): ResultAsync<void, TaskTrackerError> {
  if (!validPRNumber(prNumber)) return errAsync(prErr('Invalid PR number'))
  const args = ['pr', 'close', String(prNumber)]
  if (deleteBranch) args.push('--delete-branch')
  return fromExternalCall(commandRunner(repoRoot, args), prCliFailure).map(() => undefined)
}

/**
 * Does the branch still exist on the remote? 404 → false; any other failure (network, auth) →
 * true, so the delete action stays available and its own error surfaces the real problem.
 */
export function remoteBranchExists(
  repoRoot: string,
  branch: string,
  commandRunner: PRCommandRunner = runGhCommand,
): ResultAsync<boolean, TaskTrackerError> {
  if (!isSafeGitRefName(branch)) {
    return okAsync(false)
  }
  return fromExternalCall(
    commandRunner(
      repoRoot,
      ['api', `repos/{owner}/{repo}/branches/${encodeURIComponent(branch)}`],
      {
        maxBuffer: 1024 * 1024,
      },
    ),
    prCliFailure,
  )
    .map(() => true)
    .orElse((e) => okAsync(!/HTTP 404|Not Found/i.test('reason' in e ? String(e.reason) : '')))
}

/** Delete the remote head branch of a merged/closed PR ({owner}/{repo} resolved by gh from cwd). */
export function deleteRemoteBranch(
  repoRoot: string,
  branch: string,
  commandRunner: PRCommandRunner = runGhCommand,
): ResultAsync<void, TaskTrackerError> {
  if (!isSafeGitRefName(branch)) {
    return errAsync(prErr('Invalid branch name'))
  }
  return fromExternalCall(
    commandRunner(repoRoot, [
      'api',
      '-X',
      'DELETE',
      `repos/{owner}/{repo}/git/refs/heads/${encodeURIComponent(branch)}`,
    ]),
    prCliFailure,
  ).map(() => undefined)
}

export function buildPRConfig(
  titleTemplate: string,
  bodyTemplate: string,
  defaultTargetBranch: string,
  targetRules: PRTargetRule[] = [],
): PRTemplateConfig {
  return { titleTemplate, bodyTemplate, defaultTargetBranch, targetRules }
}
