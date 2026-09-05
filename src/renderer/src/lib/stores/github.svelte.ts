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
  settled: boolean
}
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const fallbackSummaryRequests = new Map<string, PRFallbackCacheEntry>()
// Invalidating the renderer cache also forces the next main-process origin probe. This makes the
// visible Retry action recover immediately after an origin is added or changed.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const forceRemoteProbeKeys = new Map<string, number>()

const DEBOUNCE_MS = 30_000
export const PR_FALLBACK_TTL_MS = 30_000
export const PR_FALLBACK_CACHE_MAX = 100

function baseKeyFromRequestKey(requestKey: string): string {
  return requestKey.slice(0, requestKey.lastIndexOf('::'))
}

function hasFallbackRequest(baseKey: string): boolean {
  return [...fallbackSummaryRequests.keys()].some(
    (requestKey) => baseKeyFromRequestKey(requestKey) === baseKey,
  )
}

function trimFallbackMetadata(protectedKey?: string): void {
  for (const key of Object.keys(fallbackGenerationByKey)) {
    if (Object.keys(fallbackGenerationByKey).length <= PR_FALLBACK_CACHE_MAX) break
    if (key === protectedKey || hasFallbackRequest(key)) continue
    delete fallbackGenerationByKey[key]
  }
}

function trimForcedProbeKeys(protectedKey: string): void {
  while (forceRemoteProbeKeys.size > PR_FALLBACK_CACHE_MAX) {
    const oldest = [...forceRemoteProbeKeys.keys()].find((key) => key !== protectedKey)
    if (!oldest) break
    forceRemoteProbeKeys.delete(oldest)
    if (!hasFallbackRequest(oldest)) delete fallbackGenerationByKey[oldest]
  }
}

function trimFallbackRequests(now: number): void {
  for (const [key, entry] of fallbackSummaryRequests) {
    if (entry.expiresAt <= now) fallbackSummaryRequests.delete(key)
  }
  while (fallbackSummaryRequests.size > PR_FALLBACK_CACHE_MAX) {
    const oldest = [...fallbackSummaryRequests.entries()].find(([, entry]) => entry.settled)?.[0]
    if (!oldest) break
    fallbackSummaryRequests.delete(oldest)
    const baseKey = baseKeyFromRequestKey(oldest)
    if (
      ![...fallbackSummaryRequests.keys()].some((key) => baseKeyFromRequestKey(key) === baseKey)
    ) {
      delete fallbackGenerationByKey[baseKey]
      forceRemoteProbeKeys.delete(baseKey)
    }
  }
}

export function getPRFallbackCacheSizes(): {
  requests: number
  generations: number
  forcedProbes: number
} {
  return {
    requests: fallbackSummaryRequests.size,
    generations: Object.keys(fallbackGenerationByKey).length,
    forcedProbes: forceRemoteProbeKeys.size,
  }
}

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
  const generation = (fallbackGenerationByKey[key] ?? 0) + 1
  fallbackGenerationByKey[key] = generation
  forceRemoteProbeKeys.set(key, generation)
  trimForcedProbeKeys(key)
  trimFallbackMetadata(key)
}

export function loadPRFallbackSummary(
  repoRoot: string,
  branch: string,
): Promise<{ number: number; state: string; isDraft: boolean } | null> {
  const key = prKey(repoRoot, branch)
  // Forced-probe metadata outlives the bounded generation cache. Retaining its generation here
  // prevents a trimmed entry from colliding with an older generation still running in main.
  const forcedGeneration = forceRemoteProbeKeys.get(key)
  const generation = forcedGeneration ?? getPRFallbackGeneration(repoRoot, branch)
  const requestKey = `${prKey(repoRoot, branch)}::${generation}`
  const existing = fallbackSummaryRequests.get(requestKey)
  if (existing && existing.expiresAt > Date.now()) return existing.request
  if (existing) fallbackSummaryRequests.delete(requestKey)
  trimFallbackRequests(Date.now())

  const forceRemoteProbe = forceRemoteProbeKeys.delete(key)
  const request = window.api.taskTrackerPRSummary(repoRoot, branch, generation, forceRemoteProbe)
  const entry: PRFallbackCacheEntry = {
    request,
    expiresAt: Number.POSITIVE_INFINITY,
    settled: false,
  }
  fallbackSummaryRequests.set(requestKey, entry)
  trimFallbackMetadata(prKey(repoRoot, branch))
  void request.then(
    () => {
      if (fallbackSummaryRequests.get(requestKey) === entry) {
        entry.settled = true
        entry.expiresAt = Date.now() + PR_FALLBACK_TTL_MS
        trimFallbackRequests(Date.now())
        trimFallbackMetadata()
      }
    },
    () => {
      if (fallbackSummaryRequests.get(requestKey) === entry) {
        fallbackSummaryRequests.delete(requestKey)
        trimFallbackRequests(Date.now())
        trimFallbackMetadata()
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
