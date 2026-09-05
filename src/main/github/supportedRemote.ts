import { GitRepository } from '../git/GitRepository'
import { parseGitHubRemote } from './remoteUrl'

export const SUPPORTED_GITHUB_REMOTE_TTL_MS = 30_000
const SUPPORTED_REMOTE_CACHE_MAX = 100

type SupportedRemoteProbe = (repoRoot: string) => Promise<boolean>

interface SupportedRemoteCacheEntry {
  expiresAt: number
  request: Promise<boolean>
}

const supportedRemoteCache = new Map<string, SupportedRemoteCacheEntry>()

function cacheKey(repoRoot: string): string {
  const normalized = repoRoot.replace(/\\/g, '/')
  return process.platform === 'win32' ? normalized.toLowerCase() : normalized
}

const probeSupportedGitHubRemote: SupportedRemoteProbe = async (repoRoot) => {
  const hasRemote = await GitRepository.hasRemote(repoRoot)
  if (hasRemote.isErr() || !hasRemote.value) return false
  const remote = await GitRepository.getRemoteUrl(repoRoot)
  return remote.isOk() && parseGitHubRemote(remote.value).isOk()
}

/** Coalesce branch lookups and briefly cache the repository-scoped origin check. */
export function hasSupportedGitHubRemote(
  repoRoot: string,
  probe: SupportedRemoteProbe = probeSupportedGitHubRemote,
  force = false,
): Promise<boolean> {
  const key = cacheKey(repoRoot)
  const now = Date.now()
  if (force) supportedRemoteCache.delete(key)
  const existing = supportedRemoteCache.get(key)
  if (existing && existing.expiresAt > now) return existing.request

  for (const [cachedKey, entry] of supportedRemoteCache) {
    if (entry.expiresAt <= now) supportedRemoteCache.delete(cachedKey)
  }
  if (supportedRemoteCache.size >= SUPPORTED_REMOTE_CACHE_MAX) {
    const oldestKey = supportedRemoteCache.keys().next().value
    if (oldestKey !== undefined) supportedRemoteCache.delete(oldestKey)
  }

  const request = Promise.resolve().then(() => probe(repoRoot))
  const entry = { request, expiresAt: Number.POSITIVE_INFINITY }
  supportedRemoteCache.set(key, entry)
  void request.then(
    () => {
      if (supportedRemoteCache.get(key) === entry) {
        entry.expiresAt = Date.now() + SUPPORTED_GITHUB_REMOTE_TTL_MS
      }
    },
    () => {
      if (supportedRemoteCache.get(key) === entry) supportedRemoteCache.delete(key)
    },
  )
  return request
}

export function resetSupportedGitHubRemoteCache(): void {
  supportedRemoteCache.clear()
}
