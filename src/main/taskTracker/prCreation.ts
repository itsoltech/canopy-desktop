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

function runGhCommand(
  repoRoot: string,
  args: string[],
  extraOptions: { maxBuffer?: number } = {},
): Promise<{ stdout: string; stderr: string }> {
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

function detectGhCli(repoRoot: string): ResultAsync<true, TaskTrackerError> {
  return fromExternalCall(runGhCommand(repoRoot, ['--version']), (error) =>
    isMissingGitHubCli(error)
      ? prErr('GitHub CLI (gh) is not installed. Install it to create PRs automatically.')
      : prCliFailure(error),
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
  // Only an OPEN PR blocks creating another one — a branch may accumulate merged/closed PRs
  // (gh pr view would happily return those), and a new PR is legitimate then.
  return fromExternalCall(
    runGhCommand(repoRoot, [
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
): ResultAsync<CreatePRResult, TaskTrackerError> {
  const { repoRoot, task, sourceBranch, prConfig, existingBranches, overrides } = params

  // Reject branch names that could be consumed as a gh CLI flag. sourceBranch is
  // passed as the value to `--head` below; `--` separators don't help in that
  // position. findExistingPR already performs the same check.
  if (typeof sourceBranch !== 'string' || sourceBranch.startsWith('-')) {
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
  if (targetBranch.startsWith('-')) {
    return errAsync(prErr('Invalid target branch name'))
  }

  return (
    GitRepository.push(repoRoot)
      .orElse(() => okAsync({ branch: '', remote: '' }))
      // Verify gh CLI is available
      .andThen(() => detectGhCli(repoRoot))
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
        return fromExternalCall(runGhCommand(repoRoot, args), prCliFailure).map(
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
): ResultAsync<void, TaskTrackerError> {
  if (!validPRNumber(prNumber)) return errAsync(prErr('Invalid PR number'))
  if (!MERGE_STRATEGIES.includes(strategy)) return errAsync(prErr('Invalid merge strategy'))
  const args = ['pr', 'merge', String(prNumber), `--${strategy}`]
  if (deleteBranch) args.push('--delete-branch')
  return fromExternalCall(runGhCommand(repoRoot, args), prCliFailure).map(() => undefined)
}

/** Close an open PR without merging. */
export function closePullRequest(
  repoRoot: string,
  prNumber: number,
  deleteBranch: boolean,
): ResultAsync<void, TaskTrackerError> {
  if (!validPRNumber(prNumber)) return errAsync(prErr('Invalid PR number'))
  const args = ['pr', 'close', String(prNumber)]
  if (deleteBranch) args.push('--delete-branch')
  return fromExternalCall(runGhCommand(repoRoot, args), prCliFailure).map(() => undefined)
}

/**
 * Renderer-supplied branch names end up interpolated into the PATH of authenticated `gh api`
 * calls (including a DELETE). Validate against git ref-name rules plus URL metacharacters so a
 * crafted value cannot retarget the request — legitimate branch names keep their `/` segments,
 * so validation is preferred over encoding here.
 */
/**
 * Does the branch still exist on the remote? 404 → false; any other failure (network, auth) →
 * true, so the delete action stays available and its own error surfaces the real problem.
 */
export function remoteBranchExists(
  repoRoot: string,
  branch: string,
): ResultAsync<boolean, TaskTrackerError> {
  if (!isSafeGitRefName(branch)) {
    return okAsync(false)
  }
  return fromExternalCall(
    runGhCommand(repoRoot, ['api', `repos/{owner}/{repo}/branches/${branch.trim()}`], {
      maxBuffer: 1024 * 1024,
    }),
    prCliFailure,
  )
    .map(() => true)
    .orElse((e) => okAsync(!/HTTP 404|Not Found/i.test('reason' in e ? String(e.reason) : '')))
}

/** Delete the remote head branch of a merged/closed PR ({owner}/{repo} resolved by gh from cwd). */
export function deleteRemoteBranch(
  repoRoot: string,
  branch: string,
): ResultAsync<void, TaskTrackerError> {
  if (!isSafeGitRefName(branch)) {
    return errAsync(prErr('Invalid branch name'))
  }
  return fromExternalCall(
    runGhCommand(repoRoot, [
      'api',
      '-X',
      'DELETE',
      `repos/{owner}/{repo}/git/refs/heads/${branch.trim()}`,
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
