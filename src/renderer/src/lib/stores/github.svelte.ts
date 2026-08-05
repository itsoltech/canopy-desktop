import { match } from 'ts-pattern'
import { addToast } from './toast.svelte'

let branchPRs: GitHubBranchPRMap = $state({})
let repoInfo: GitHubRepoInfo | null = $state(null)
let loading = $state(false)
const lastFetchByRepo: Record<string, number> = {}
let fallbackGenerationByKey: Record<string, number> = $state({})
// This is deliberately not reactive: GitSection calls the loader from an effect, so tracking
// cache get/set operations would make the effect depend on and then mutate the same collection.
type PRFallbackSummary = { number: number; state: string; isDraft: boolean } | null
interface PRFallbackCacheEntry {
  request: Promise<PRFallbackSummary>
  expiresAt: number
}
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const fallbackSummaryRequests = new Map<string, PRFallbackCacheEntry>()
// Invalidating the renderer cache also forces the next main-process origin probe. This makes the
// visible Retry action recover immediately after an origin is added or changed.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const forceRemoteProbeKeys = new Set<string>()

const DEBOUNCE_MS = 30_000
export const PR_FALLBACK_TTL_MS = 30_000

export function getBranchPRMap(): GitHubBranchPRMap {
  return branchPRs
}

/** Cache key: the same branch name can carry different PRs in different repositories. */
export function prKey(repoRoot: string | null | undefined, branch: string): string {
  return `${(repoRoot ?? '').replace(/\\/g, '/')}::${branch}`
}

export function getPRForBranch(
  repoRoot: string | null | undefined,
  branch: string,
): GitHubPRInfo | undefined {
  return branchPRs[prKey(repoRoot, branch)]
}

export function getGitHubRepoInfo(): GitHubRepoInfo | null {
  return repoInfo
}

export function isGitHubLoading(): boolean {
  return loading
}

export function getPRFallbackGeneration(repoRoot: string, branch: string): number {
  return fallbackGenerationByKey[prKey(repoRoot, branch)] ?? 0
}

export function invalidatePRFallback(repoRoot: string, branch: string): void {
  const key = prKey(repoRoot, branch)
  for (const requestKey of fallbackSummaryRequests.keys()) {
    if (requestKey.startsWith(`${key}::`)) fallbackSummaryRequests.delete(requestKey)
  }
  fallbackGenerationByKey[key] = (fallbackGenerationByKey[key] ?? 0) + 1
  forceRemoteProbeKeys.add(key)
}

export function loadPRFallbackSummary(
  repoRoot: string,
  branch: string,
): Promise<{ number: number; state: string; isDraft: boolean } | null> {
  const generation = getPRFallbackGeneration(repoRoot, branch)
  const requestKey = `${prKey(repoRoot, branch)}::${generation}`
  const existing = fallbackSummaryRequests.get(requestKey)
  if (existing && existing.expiresAt > Date.now()) return existing.request
  if (existing) fallbackSummaryRequests.delete(requestKey)

  const forceRemoteProbe = forceRemoteProbeKeys.delete(prKey(repoRoot, branch))
  const request = window.api.taskTrackerPRSummary(repoRoot, branch, forceRemoteProbe)
  const entry: PRFallbackCacheEntry = { request, expiresAt: Number.POSITIVE_INFINITY }
  fallbackSummaryRequests.set(requestKey, entry)
  void request.then(
    () => {
      if (fallbackSummaryRequests.get(requestKey) === entry) {
        entry.expiresAt = Date.now() + PR_FALLBACK_TTL_MS
      }
    },
    () => {
      if (fallbackSummaryRequests.get(requestKey) === entry) {
        fallbackSummaryRequests.delete(requestKey)
      }
    },
  )
  return request
}

export async function loadBranchPRs(repoRoot: string, force = false): Promise<void> {
  const now = Date.now()
  const lastFetch = lastFetchByRepo[repoRoot] ?? 0
  if (!force && now - lastFetch < DEBOUNCE_MS) return
  loading = true
  try {
    const result = await window.api.githubFetchBranchPRs(repoRoot)
    lastFetchByRepo[repoRoot] = Date.now()
    // Merge with existing PRs from other repos — scoped by repo so same-name branches don't collide.
    const scoped = Object.fromEntries(
      Object.entries(result).map(([branch, pr]) => [prKey(repoRoot, branch), pr]),
    )
    branchPRs = { ...branchPRs, ...scoped }
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
  fallbackGenerationByKey = {}
  fallbackSummaryRequests.clear()
  forceRemoteProbeKeys.clear()
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
