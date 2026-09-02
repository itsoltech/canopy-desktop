import { execFile } from 'child_process'

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

const inFlightSummaryRequests = new Map<string, Promise<PullRequestSummary | null>>()
const summaryRequestTails = new Map<string, Promise<PullRequestSummary | null>>()

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

export function loadPullRequestSummary(
  repoRoot: string,
  branch: string,
  generation: number,
  run: SummaryCommandRunner = runSummaryCommand,
): Promise<PullRequestSummary | null> {
  const targetKey = JSON.stringify([repoRoot.replace(/\\/g, '/'), branch])
  const requestKey = JSON.stringify([targetKey, generation])
  const existing = inFlightSummaryRequests.get(requestKey)
  if (existing) return existing

  const execute = async (): Promise<PullRequestSummary | null> => {
    try {
      const { stdout } = await run('gh', ['pr', 'view', branch, '--json', PR_SUMMARY_FIELDS], {
        cwd: repoRoot,
        encoding: 'utf8',
        maxBuffer: 64 * 1024,
        timeout: PR_SUMMARY_TIMEOUT_MS,
        windowsHide: true,
      })
      const parsed: unknown = JSON.parse(stdout)
      return isPullRequestSummary(parsed) ? parsed : null
    } catch {
      return null
    }
  }
  const tail = summaryRequestTails.get(targetKey)
  const request = (tail ? tail.then(execute, execute) : execute()).finally(() => {
    if (inFlightSummaryRequests.get(requestKey) === request) {
      inFlightSummaryRequests.delete(requestKey)
    }
    if (summaryRequestTails.get(targetKey) === request) summaryRequestTails.delete(targetKey)
  })

  inFlightSummaryRequests.set(requestKey, request)
  summaryRequestTails.set(targetKey, request)
  return request
}
