import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { AppStateSnapshot } from '../../../../main/commands/types'

const storeMocks = vi.hoisted(() => ({
  applyTabsSnapshot: vi.fn(),
  loadActiveTask: vi.fn(async () => undefined),
  loadRepoConfig: vi.fn(async () => undefined),
  resolvePanelTask: vi.fn(async () => undefined),
}))

vi.mock('./tabs.svelte', () => ({
  restoreLayout: vi.fn(),
  closeAllTabsForWorktree: vi.fn(),
  killAllTabs: vi.fn(),
  tabsByWorktree: {},
  applyTabsSnapshot: storeMocks.applyTabsSnapshot,
}))

vi.mock('./taskTracker.svelte', () => ({
  loadRepoConfig: storeMocks.loadRepoConfig,
  getRepoConfig: vi.fn(() => null),
  hasAnyCredentials: vi.fn(() => false),
  loadActiveTask: storeMocks.loadActiveTask,
  resolvePanelTask: storeMocks.resolvePanelTask,
}))

vi.mock('./toast.svelte', () => ({ addToast: vi.fn() }))
vi.mock('./quickOpenStore.svelte', () => ({ clearQuickOpenCache: vi.fn() }))
vi.mock('./quickOpenMru.svelte', () => ({ clearMru: vi.fn() }))

const selectedPath = 'C:/source/GithubITSOL/gakko'
const selectedBranch = 's116/GAKKO-5912/usuniecie-odniesien-do-podanie-zalacznik'

const project = {
  workspace: {
    id: 'gakko',
    path: selectedPath,
    name: 'gakko',
    is_git_repo: 1,
    last_opened: null,
    cached_branch: selectedBranch,
    cached_dirty: 0,
    cached_ahead_behind: null,
    cached_worktree_count: 1,
  },
  isGitRepo: true,
  repoRoot: selectedPath,
  worktrees: [
    {
      path: selectedPath,
      head: '0123456789abcdef',
      branch: selectedBranch,
      isMain: true,
      isBare: false,
    },
  ],
}

const initialSnapshot: AppStateSnapshot = {
  workspace: {
    projects: [project],
    workspaceState: {
      project,
      selectedWorktreePath: selectedPath,
      branch: selectedBranch,
      isDirty: false,
      aheadBehind: null,
    },
  },
  tabs: {
    tabsByWorktree: {},
    activeTabIdByWorktree: {},
  },
}

const api = {
  getAppState: vi.fn(async () => initialSnapshot),
  onAppStateChanged: vi.fn(() => vi.fn()),
  watchFiles: vi.fn(async () => undefined),
}

vi.stubGlobal('window', { api })

import { initWorkspaceStateSubscription, workspaceState } from './workspace.svelte'

let cleanup: (() => void) | undefined

describe('initWorkspaceStateSubscription', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.getAppState.mockResolvedValue(initialSnapshot)
    api.onAppStateChanged.mockReturnValue(vi.fn())
    api.watchFiles.mockResolvedValue(undefined)
    storeMocks.loadActiveTask.mockResolvedValue(undefined)
    storeMocks.loadRepoConfig.mockResolvedValue(undefined)
    storeMocks.resolvePanelTask.mockResolvedValue(undefined)
    workspaceState.selectedWorktreePath = null
  })

  afterEach(() => {
    cleanup?.()
    cleanup = undefined
  })

  it('hydrates the selected worktree from the initial app-state snapshot', async () => {
    let finishActiveTask: (() => void) | undefined
    storeMocks.loadActiveTask.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          finishActiveTask = resolve
        }),
    )
    cleanup = initWorkspaceStateSubscription()

    await vi.waitFor(() => {
      expect(storeMocks.loadActiveTask).toHaveBeenCalledWith(selectedPath, {
        shouldApply: expect.any(Function),
      })
    })

    expect(api.watchFiles).not.toHaveBeenCalled()
    expect(storeMocks.resolvePanelTask).not.toHaveBeenCalled()

    finishActiveTask?.()

    await vi.waitFor(() => {
      expect(storeMocks.resolvePanelTask).toHaveBeenCalledWith(selectedPath, selectedBranch, {
        shouldApply: expect.any(Function),
      })
    })

    expect(api.watchFiles).toHaveBeenCalledWith(selectedPath)
    expect(storeMocks.loadRepoConfig).toHaveBeenCalledWith(selectedPath, expect.any(Function))
  })

  it('drops a stale initial hydration after the selected worktree changes', async () => {
    let finishActiveTask: (() => void) | undefined
    storeMocks.loadActiveTask.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          finishActiveTask = resolve
        }),
    )
    cleanup = initWorkspaceStateSubscription()

    await vi.waitFor(() => expect(storeMocks.loadActiveTask).toHaveBeenCalledOnce())
    workspaceState.selectedWorktreePath = 'C:/source/GithubITSOL/canopy-desktop'
    finishActiveTask?.()

    await vi.waitFor(() => {
      const options = storeMocks.loadActiveTask.mock.calls[0]?.[1]
      expect(options?.shouldApply()).toBe(false)
    })
    expect(api.watchFiles).not.toHaveBeenCalled()
    expect(storeMocks.loadRepoConfig).not.toHaveBeenCalled()
    expect(storeMocks.resolvePanelTask).not.toHaveBeenCalled()
  })

  it('ignores an initial snapshot that resolves after a newer selection', async () => {
    let finishGetAppState: ((snapshot: AppStateSnapshot) => void) | undefined
    const pendingSnapshot = new Promise<AppStateSnapshot>((resolve) => {
      finishGetAppState = resolve
    })
    api.getAppState.mockReturnValueOnce(pendingSnapshot)
    cleanup = initWorkspaceStateSubscription()

    workspaceState.selectedWorktreePath = 'C:/source/GithubITSOL/canopy-desktop'
    finishGetAppState?.(initialSnapshot)
    await pendingSnapshot
    await Promise.resolve()
    await Promise.resolve()

    expect(workspaceState.selectedWorktreePath).toBe('C:/source/GithubITSOL/canopy-desktop')
    expect(storeMocks.applyTabsSnapshot).not.toHaveBeenCalled()
    expect(storeMocks.loadActiveTask).not.toHaveBeenCalled()
  })

  it('stops an initial hydration when the subscription is disposed', async () => {
    let finishActiveTask: (() => void) | undefined
    storeMocks.loadActiveTask.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          finishActiveTask = resolve
        }),
    )
    cleanup = initWorkspaceStateSubscription()

    await vi.waitFor(() => expect(storeMocks.loadActiveTask).toHaveBeenCalledOnce())
    cleanup()
    cleanup = undefined
    finishActiveTask?.()

    await vi.waitFor(() => {
      const options = storeMocks.loadActiveTask.mock.calls[0]?.[1]
      expect(options?.shouldApply()).toBe(false)
    })
    expect(api.watchFiles).not.toHaveBeenCalled()
    expect(storeMocks.loadRepoConfig).not.toHaveBeenCalled()
    expect(storeMocks.resolvePanelTask).not.toHaveBeenCalled()
  })
})
