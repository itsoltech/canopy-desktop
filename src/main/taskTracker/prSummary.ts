import { execFile } from 'child_process'
import { err, ok, ResultAsync, type Result } from 'neverthrow'
import { errorMessage } from '../errors'
import type { TaskTrackerError } from './errors'

export const PR_SUMMARY_FIELDS = 'number,state,isDraft'
export const PR_SUMMARY_TIMEOUT_MS = 15_000

export interface PullRequestSummary {
  number: number
  state: string
  isDraft: boolean
}

interface SummaryCommandOptions {
  cwd: string
  encoding: 'utf8'
  maxBuffer: number
  timeout: number
  windowsHide: true
}

export type SummaryCommandRunner = (
  command: string,
  args: string[],
  options: SummaryCommandOptions,
) => Promise<{ stdout: string }>

const runSummaryCommand: SummaryCommandRunner = (command, args, options) =>
  new Promise((resolve, reject) => {
    execFile(command, args, options, (error, stdout, stderr) => {
      if (!error) {
        resolve({ stdout })
        return
      }
      const reason = error.killed
        ? `GitHub CLI request timed out after ${PR_SUMMARY_TIMEOUT_MS / 1000} seconds`
        : stderr.trim() || error.message
      reject(Object.assign(new Error(reason), { code: (error as NodeJS.ErrnoException).code }))
    })
  })

function isPullRequestSummary(value: unknown): value is PullRequestSummary {
  if (!value || typeof value !== 'object') return false
  const candidate = value as Partial<PullRequestSummary>
  return (
    Number.isInteger(candidate.number) &&
    (candidate.number ?? 0) > 0 &&
    typeof candidate.state === 'string' &&
    candidate.state.length > 0 &&
    typeof candidate.isDraft === 'boolean'
  )
}

function lookupError(reason: string): TaskTrackerError {
  return { _tag: 'PRLookupFailed', reason }
}

function isMissingGitHubCli(error: unknown): boolean {
  return (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    (error as { code?: unknown }).code === 'ENOENT'
  )
}

function parseSummaryList(stdout: string): Result<PullRequestSummary | null, TaskTrackerError> {
  let parsed: unknown
  try {
    parsed = JSON.parse(stdout)
  } catch (e) {
    return err(lookupError(`Invalid GitHub CLI response: ${errorMessage(e)}`))
  }
  if (!Array.isArray(parsed)) {
    return err(lookupError('GitHub CLI returned a non-list response'))
  }
  if (parsed.length === 0) return ok(null)
  return isPullRequestSummary(parsed[0])
    ? ok(parsed[0])
    : err(lookupError('GitHub CLI returned an invalid pull request summary'))
}

export function loadPullRequestSummary(
  repoRoot: string,
  branch: string,
  run: SummaryCommandRunner = runSummaryCommand,
): ResultAsync<PullRequestSummary | null, TaskTrackerError> {
  const findByState = (
    state: 'open' | 'closed',
  ): ResultAsync<PullRequestSummary | null, TaskTrackerError> =>
    ResultAsync.fromPromise(
      run(
        'gh',
        [
          'pr',
          'list',
          '--head',
          branch,
          '--state',
          state,
          '--limit',
          '1',
          '--json',
          PR_SUMMARY_FIELDS,
        ],
        {
          cwd: repoRoot,
          encoding: 'utf8',
          maxBuffer: 64 * 1024,
          timeout: PR_SUMMARY_TIMEOUT_MS,
          windowsHide: true,
        },
      ),
      (e) => e,
    )
      .andThen(({ stdout }) => parseSummaryList(stdout))
      .orElse((e) => (isMissingGitHubCli(e) ? ok(null) : err(lookupError(errorMessage(e)))))

  return findByState('open').andThen((openPR) => (openPR ? ok(openPR) : findByState('closed')))
}
