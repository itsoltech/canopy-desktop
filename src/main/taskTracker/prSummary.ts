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

export async function loadPullRequestSummary(
  repoRoot: string,
  branch: string,
  run: SummaryCommandRunner = runSummaryCommand,
): Promise<PullRequestSummary | null> {
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
