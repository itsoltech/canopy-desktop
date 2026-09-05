import { execFile } from 'child_process'
import { err, ok, ResultAsync, type Result } from 'neverthrow'
import { errorMessage } from '../errors'
import { gitHubCliFailureReason, isMissingGitHubCli } from '../github/redactFailureReason'
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
      reject(
        Object.assign(new Error(gitHubCliFailureReason(error, PR_SUMMARY_TIMEOUT_MS, stderr)), {
          code: (error as NodeJS.ErrnoException).code,
        }),
      )
    })
  })

type SummaryResult = Result<PullRequestSummary | null, TaskTrackerError>
const inFlightSummaryRequests = new Map<
  string,
  ResultAsync<PullRequestSummary | null, TaskTrackerError>
>()
const summaryRequestTails = new Map<string, Promise<SummaryResult>>()

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

function isLookupError(error: unknown): error is TaskTrackerError {
  return (
    typeof error === 'object' &&
    error !== null &&
    '_tag' in error &&
    (error as { _tag?: unknown })._tag === 'PRLookupFailed'
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
  run?: SummaryCommandRunner,
): ResultAsync<PullRequestSummary | null, TaskTrackerError>
export function loadPullRequestSummary(
  repoRoot: string,
  branch: string,
  generation: number,
  run?: SummaryCommandRunner,
): ResultAsync<PullRequestSummary | null, TaskTrackerError>
export function loadPullRequestSummary(
  repoRoot: string,
  branch: string,
  generationOrRun: number | SummaryCommandRunner = 0,
  explicitRun: SummaryCommandRunner = runSummaryCommand,
): ResultAsync<PullRequestSummary | null, TaskTrackerError> {
  const generation = typeof generationOrRun === 'number' ? generationOrRun : 0
  const run = typeof generationOrRun === 'function' ? generationOrRun : explicitRun
  const targetKey = JSON.stringify([repoRoot.replace(/\\/g, '/'), branch])
  const requestKey = JSON.stringify([targetKey, generation])
  const existing = inFlightSummaryRequests.get(requestKey)
  if (existing) return existing

  const execute = (): ResultAsync<PullRequestSummary | null, TaskTrackerError> => {
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
        .orElse((e) => {
          if (isMissingGitHubCli(e)) return ok(null)
          return isLookupError(e) ? err(e) : err(lookupError(errorMessage(e)))
        })

    return findByState('open').andThen((openPR) =>
      openPR
        ? ok(openPR)
        : // The closed lookup is decorative. The open query already proved Create PR is safe.
          findByState('closed').orElse(() => ok<PullRequestSummary | null, TaskTrackerError>(null)),
    )
  }

  const previous = summaryRequestTails.get(targetKey)
  const queued: Promise<SummaryResult> = previous
    ? previous.then(execute, execute)
    : Promise.resolve(execute())
  const settled = queued.finally(() => {
    inFlightSummaryRequests.delete(requestKey)
    if (summaryRequestTails.get(targetKey) === settled) summaryRequestTails.delete(targetKey)
  })
  const request = new ResultAsync(settled)
  inFlightSummaryRequests.set(requestKey, request)
  summaryRequestTails.set(targetKey, settled)
  return request
}
