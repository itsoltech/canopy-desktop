import { execFile } from 'child_process'
import { promisify } from 'util'
import type { TrackerTask, PRTemplateConfig, PRTargetRule } from './types'
import { renderPRTitle, renderPRBody, resolveTargetBranch } from './prTemplate'
import { GitRepository } from '../git/GitRepository'

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

async function detectGhCli(): Promise<boolean> {
  try {
    await execFileAsync('gh', ['--version'])
    return true
  } catch {
    return false
  }
}

export async function createPullRequest(params: CreatePRParams): Promise<CreatePRResult> {
  const { repoRoot, task, sourceBranch, prConfig, existingBranches } = params

  const title = renderPRTitle(prConfig.titleTemplate, task)
  const body = renderPRBody(prConfig.bodyTemplate, task)
  const targetBranch = resolveTargetBranch(
    task,
    prConfig.defaultTargetBranch,
    prConfig.targetRules,
    existingBranches,
  )

  // Ensure branch is pushed (ignore errors -- may already be pushed or upstream set)
  await GitRepository.push(repoRoot).unwrapOr({ branch: '', remote: '' })

  const hasGh = await detectGhCli()
  if (!hasGh) {
    throw new Error('GitHub CLI (gh) is not installed. Install it to create PRs automatically.')
  }

  // Check if PR already exists for this branch
  try {
    const { stdout: existing } = await execFileAsync(
      'gh',
      ['pr', 'view', sourceBranch, '--json', 'url', '--jq', '.url'],
      { cwd: repoRoot },
    )
    if (existing.trim()) {
      return { url: existing.trim(), title, targetBranch }
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

  const url = stdout.trim()
  return { url, title, targetBranch }
}

export function buildPRConfig(
  titleTemplate: string,
  bodyTemplate: string,
  defaultTargetBranch: string,
  targetRules: PRTargetRule[] = [],
): PRTemplateConfig {
  return { titleTemplate, bodyTemplate, defaultTargetBranch, targetRules }
}
