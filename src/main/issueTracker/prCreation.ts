import { execFile } from 'child_process'
import { promisify } from 'util'
import type { TrackerIssue, PRTemplateConfig, PRTargetRule } from './types'
import { renderPRTitle, renderPRBody, resolveTargetBranch } from './prTemplate'
import { GitRepository } from '../git/GitRepository'

const execFileAsync = promisify(execFile)

export interface CreatePRParams {
  repoRoot: string
  issue: TrackerIssue
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
  const { repoRoot, issue, sourceBranch, prConfig, existingBranches } = params

  const title = renderPRTitle(prConfig.titleTemplate, issue)
  const body = renderPRBody(prConfig.bodyTemplate, issue)
  const targetBranch = resolveTargetBranch(
    issue,
    prConfig.defaultTargetBranch,
    prConfig.targetRules,
    existingBranches,
  )

  // Ensure branch is pushed
  try {
    await GitRepository.push(repoRoot)
  } catch {
    // May already be pushed or upstream set
  }

  const hasGh = await detectGhCli()
  if (!hasGh) {
    throw new Error('GitHub CLI (gh) is not installed. Install it to create PRs automatically.')
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
