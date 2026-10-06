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
  getPref: vi.fn(),
  setPref: vi.fn(),
}

vi.stubGlobal('window', { api })

import {
  getActiveTasks,
  getRepoConfig,
  getRepoConfigLoadError,
  loadActiveTask,
  loadRepoConfig,
  setActiveTask,
} from './taskTracker.svelte'

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

describe('setActiveTask', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.setPref.mockResolvedValue(undefined)
  })

  it('links a task to a new worktree without replacing the selected worktree tasks', async () => {
    const current = { taskKey: 'CUR-1', summary: 'Current work', connectionId: 'jira' }
    const created = { taskKey: 'NEW-2', summary: 'New work', connectionId: 'jira' }
    api.getPref.mockResolvedValueOnce(JSON.stringify([current]))
    await loadActiveTask('/repo/current')

    await setActiveTask('/repo/new', created)

    expect(getActiveTasks()).toEqual([current])
    expect(api.setPref).toHaveBeenCalledWith('activeTask./repo/new', JSON.stringify([created]))
  })
})
