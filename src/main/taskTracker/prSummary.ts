import { execFile } from 'child_process'
import { err, ok, type ResultAsync } from 'neverthrow'
import { errorMessage, fromExternalCall } from '../errors'
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
    execFile(command, args, options, (error, stdout) => {
      if (error) reject(error)
      else resolve({ stdout })
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

export function loadPullRequestSummary(
  repoRoot: string,
  branch: string,
  run: SummaryCommandRunner = runSummaryCommand,
): ResultAsync<PullRequestSummary | null, TaskTrackerError> {
  return fromExternalCall(
    run(
      'gh',
      [
        'pr',
        'list',
        '--head',
        branch,
        '--state',
        'all',
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
    (e) => lookupError(errorMessage(e)),
  ).andThen(({ stdout }) => {
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
  })
}
