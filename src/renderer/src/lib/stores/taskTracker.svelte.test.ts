import { beforeEach, describe, expect, it, vi } from 'vitest'

const config: RepoConfig = {
  version: 1,
  trackers: [],
  projectOverrides: {},
  filters: { assignedToMe: true, statuses: [] },
}

const api = {
  repoConfigLoad: vi.fn(),
  trackerResolvedConfig: vi.fn(),
}

vi.stubGlobal('window', { api })

import { getRepoConfig, getRepoConfigLoadError, loadRepoConfig } from './taskTracker.svelte'

describe('loadRepoConfig', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.repoConfigLoad.mockResolvedValue(config)
    api.trackerResolvedConfig.mockResolvedValue(null)
  })

  it('keeps a load failure distinct from a missing repository config', async () => {
    api.repoConfigLoad.mockRejectedValueOnce(new Error('Configuration file exceeds 1 MiB'))

    await loadRepoConfig('/repo')

    expect(getRepoConfig()).toBeNull()
    expect(getRepoConfigLoadError()).toBe('Configuration file exceeds 1 MiB')
    expect(api.trackerResolvedConfig).not.toHaveBeenCalled()
  })

  it('clears a previous load failure when the repository config is genuinely missing', async () => {
    api.repoConfigLoad.mockRejectedValueOnce(new Error('permission denied'))
    await loadRepoConfig('/repo')
    api.repoConfigLoad.mockResolvedValueOnce(null)

    await loadRepoConfig('/repo')

    expect(getRepoConfig()).toBeNull()
    expect(getRepoConfigLoadError()).toBeNull()
  })
})
