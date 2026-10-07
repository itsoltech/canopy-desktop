import { describe, expect, it, vi } from 'vitest'
import { okAsync } from 'neverthrow'

const detect = vi.fn()
vi.mock('../git/GitRepository', () => ({
  GitRepository: { detect: (path: string) => detect(path) },
}))
vi.mock('../git/GitWatcher', () => ({ GitWatcher: class {} }))

import { WorkspaceCommandService } from './workspaceCommands'

describe('WorkspaceCommandService.selectWorktree', () => {
  it('never runs git in a path that is not attached to the window', async () => {
    detect.mockImplementation((path: string) =>
      okAsync({
        isGitRepo: true,
        repoRoot: path,
        branch: 'main',
        worktrees: [{ path, branch: 'main', isMain: true }],
        isDirty: false,
        aheadBehind: null,
      }),
    )
    const service = new WorkspaceCommandService({
      workspaceStore: {} as never,
      layoutStore: {} as never,
      windowManager: {} as never,
      persistWindowConfigs: vi.fn(),
      validatePathAccess: vi.fn(),
      clearWorkspaceFileCache: vi.fn(),
      emitAppStateChanged: vi.fn(),
    })
    const sender = { id: 7, once: vi.fn(), isDestroyed: () => false }

    await expect(service.selectWorktree(sender as never, '/tmp/untrusted-repo')).rejects.toThrow(
      'not attached',
    )
    expect(detect).not.toHaveBeenCalled()
  })
})
