import { addToast } from './toast.svelte'

let branchPRs: GitHubBranchPRMap = $state({})
let repoInfo: GitHubRepoInfo | null = $state(null)
let loading = $state(false)
let lastFetch = 0

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

export async function loadBranchPRs(repoRoot: string): Promise<void> {
  const now = Date.now()
  if (now - lastFetch < DEBOUNCE_MS) return
  lastFetch = now
  loading = true
  try {
    branchPRs = await window.api.githubFetchBranchPRs(repoRoot)
  } catch (e) {
    branchPRs = {}
    const msg = e instanceof Error ? e.message : String(e)
    if (msg.includes('rate limit')) {
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
  lastFetch = 0
}
