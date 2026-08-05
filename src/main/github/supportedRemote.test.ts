import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  SUPPORTED_GITHUB_REMOTE_TTL_MS,
  hasSupportedGitHubRemote,
  resetSupportedGitHubRemoteCache,
} from './supportedRemote'

describe('hasSupportedGitHubRemote', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    resetSupportedGitHubRemoteCache()
  })

  it('coalesces and briefly caches the repository-scoped probe', async () => {
    let now = 10_000
    vi.spyOn(Date, 'now').mockImplementation(() => now)
    const probe = vi.fn().mockResolvedValue(true)

    await expect(
      Promise.all([
        hasSupportedGitHubRemote('C:/repo', probe),
        hasSupportedGitHubRemote('C:/repo', probe),
        hasSupportedGitHubRemote('C:/repo', probe),
      ]),
    ).resolves.toEqual([true, true, true])
    expect(probe).toHaveBeenCalledOnce()

    now += SUPPORTED_GITHUB_REMOTE_TTL_MS
    await expect(hasSupportedGitHubRemote('C:/repo', probe)).resolves.toBe(true)
    expect(probe).toHaveBeenCalledTimes(2)
  })

  it('forces a fresh probe for Retry and bounds distinct repository entries', async () => {
    const probe = vi.fn().mockResolvedValue(false)
    await hasSupportedGitHubRemote('C:/repo-0', probe)
    await hasSupportedGitHubRemote('C:/repo-0', probe, true)
    expect(probe).toHaveBeenCalledTimes(2)

    for (let index = 1; index <= 100; index += 1) {
      await hasSupportedGitHubRemote(`C:/repo-${index}`, probe)
    }
    await hasSupportedGitHubRemote('C:/repo-0', probe)
    expect(probe).toHaveBeenCalledTimes(103)
  })
})
