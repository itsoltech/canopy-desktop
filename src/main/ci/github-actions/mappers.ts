import type { CiRun, CiRunConclusion, CiRunState } from '../types'
import type { GitHubWorkflowRun } from './client'

function timestamp(value: string | null | undefined): number | undefined {
  if (!value) return undefined
  const parsed = Date.parse(value)
  return Number.isNaN(parsed) ? undefined : parsed
}

function runState(status: string | undefined): CiRunState {
  if (status === 'requested' || status === 'queued' || status === 'pending') return 'queued'
  if (status === 'waiting') return 'waiting'
  if (status === 'in_progress') return 'running'
  if (status === 'completed') return 'finished'
  return 'unknown'
}

function runConclusion(conclusion: string | null | undefined): CiRunConclusion {
  if (conclusion === 'success') return 'success'
  if (
    conclusion === 'failure' ||
    conclusion === 'timed_out' ||
    conclusion === 'startup_failure' ||
    conclusion === 'action_required'
  ) {
    return 'failure'
  }
  if (conclusion === 'cancelled') return 'cancelled'
  if (conclusion === 'neutral' || conclusion === 'skipped' || conclusion === 'stale') {
    return 'neutral'
  }
  return 'unknown'
}

export function mapGitHubRun(
  raw: GitHubWorkflowRun,
  workflowPath: string,
  workflowLabel: string,
): CiRun {
  return {
    provider: 'github-actions',
    runId: String(raw.id),
    number: raw.run_number == null ? undefined : String(raw.run_number),
    jobId: workflowPath,
    jobLabel: workflowLabel,
    state: runState(raw.status),
    conclusion: runConclusion(raw.conclusion),
    statusText: raw.display_title || raw.name,
    webUrl: raw.html_url ?? '',
    ref: raw.head_branch
      ? { name: raw.head_branch, kind: 'branch', commitSha: raw.head_sha }
      : undefined,
    queuedAt: timestamp(raw.created_at),
    startedAt: timestamp(raw.run_started_at),
    finishedAt: raw.status === 'completed' ? timestamp(raw.updated_at) : undefined,
  }
}
