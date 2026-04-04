import { execFile } from 'child_process'
import { promisify } from 'util'
import { ok, err, type ResultAsync } from 'neverthrow'
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

function detectGhCli(): ResultAsync<true, TaskTrackerError> {
  return fromExternalCall(execFileAsync('gh', ['--version']), () => ({
    _tag: 'ProviderApiError' as const,
    status: 0,
    message: 'GitHub CLI (gh) is not installed. Install it to create PRs automatically.',
    provider: 'jira' as const,
  })).map(() => true as const)
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

  return fromExternalCall(
    (async () => {
      // Ensure branch is pushed
      await GitRepository.push(repoRoot).unwrapOr({ branch: '', remote: '' })

      const ghResult = await detectGhCli()
      if (ghResult.isErr()) return err(ghResult.error)

      // Check if PR already exists
      try {
        const { stdout: existing } = await execFileAsync(
          'gh',
          ['pr', 'view', sourceBranch, '--json', 'url', '--jq', '.url'],
          { cwd: repoRoot },
        )
        if (existing.trim()) {
          return ok({ url: existing.trim(), title, targetBranch })
        }
      } catch {
        // No existing PR — proceed to create
      }

      const { stdout } = await execFileAsync(
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
      )

      return ok({ url: stdout.trim(), title, targetBranch })
    })(),
    (e) => ({
      _tag: 'ProviderApiError' as const,
      status: 0,
      message: errorMessage(e),
      provider: 'jira' as const,
    }),
  ).andThen((result) => result)
}

export function buildPRConfig(
  titleTemplate: string,
  bodyTemplate: string,
  defaultTargetBranch: string,
  targetRules: PRTargetRule[] = [],
): PRTemplateConfig {
  return { titleTemplate, bodyTemplate, defaultTargetBranch, targetRules }
}
