import { match } from 'ts-pattern'
import { addToast } from './toast.svelte'

let branchPRs: GitHubBranchPRMap = $state({})
let repoInfo: GitHubRepoInfo | null = $state(null)
let loading = $state(false)
const lastFetchByRepo: Record<string, number> = {}

const DEBOUNCE_MS = 30_000

export function getBranchPRMap(): GitHubBranchPRMap {
  return branchPRs
}

export function getPRForBranch(branch: string): GitHubPRInfo | undefined {
  return branchPRs[branch]
}

export function getGitHubRepoInfo(): GitHubRepoInfo | null {
  return repoInfo
}

export function isGitHubLoading(): boolean {
  return loading
}

export async function loadBranchPRs(repoRoot: string, force = false): Promise<void> {
  const now = Date.now()
  const lastFetch = lastFetchByRepo[repoRoot] ?? 0
  if (!force && now - lastFetch < DEBOUNCE_MS) return
  loading = true
  try {
    const result = await window.api.githubFetchBranchPRs(repoRoot)
    lastFetchByRepo[repoRoot] = Date.now()
    // Merge with existing PRs from other repos
    branchPRs = { ...branchPRs, ...result }
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    if (msg.includes('rate limit') || msg.includes('401') || msg.includes('403')) {
      addToast(msg)
    }
  } finally {
    loading = false
  }
}

export async function loadRepoInfo(repoRoot: string): Promise<void> {
  try {
    repoInfo = await window.api.githubGetRepoInfo(repoRoot)
  } catch (e) {
    repoInfo = null
    const msg = e instanceof Error ? e.message : String(e)
    if (msg.includes('rate limit') || msg.includes('401') || msg.includes('403')) {
      addToast(msg)
    }
  }
}

export function resetGitHubState(): void {
  branchPRs = {}
  repoInfo = null
  for (const key of Object.keys(lastFetchByRepo)) delete lastFetchByRepo[key]
}

export function formatPrBadge(pr: GitHubPRInfo): { className: string; label: string } {
  const base =
    'text-2xs font-medium px-1 rounded-sm border-0 cursor-pointer flex-shrink-0 ml-auto font-inherit leading-4 hover:opacity-80'
  return match(pr)
    .with({ isDraft: true }, () => ({
      className: `${base} bg-hover-strong text-text-muted`,
      label: 'Draft',
    }))
    .with({ reviewDecision: 'APPROVED' }, () => ({
      className: `${base} bg-success-bg text-success`,
      label: 'Approved',
    }))
    .with({ reviewDecision: 'CHANGES_REQUESTED' }, () => ({
      className: `${base} bg-warning-bg text-warning-text`,
      label: 'Changes',
    }))
    .otherwise(() => ({
      className: `${base} bg-accent-bg text-accent-text`,
      label: `PR #${pr.number}`,
    }))
}
