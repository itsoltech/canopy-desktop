import { afterEach, describe, expect, it, vi } from 'vitest'
import type { PaneSnapshot, TabSnapshot } from '../../../../main/commands/types'
import { applyTabsSnapshot, tabsByWorktree, updateSplitRatio } from './tabs.svelte'
import { findLeaf } from './splitTree'

const worktreePath = '/work/repo'
const filePath = '/work/repo/src/app.ts'

function editorTab(dirty: boolean, currentContent: string): TabSnapshot {
  return {
    id: 'tab-1',
    toolId: 'editor',
    toolName: 'Editor',
    name: 'app.ts',
    worktreePath,
    focusedPaneId: 'pane-1',
    rootSplit: {
      type: 'leaf',
      pane: {
        id: 'pane-1',
        sessionId: 'editor-1',
        wsUrl: '',
        toolId: 'editor',
        toolName: 'Editor',
        isRunning: false,
        exitCode: null,
        title: null,
        paneType: 'editor',
        editorFiles: [{ filePath, dirty, originalContent: 'one', currentContent }],
        editorActiveFile: filePath,
      },
    },
  }
}

describe('applyTabsSnapshot', () => {
  it('takes editor file state from each new snapshot instead of the first one seen', () => {
    applyTabsSnapshot({
      tabsByWorktree: { [worktreePath]: [editorTab(false, 'one')] },
      activeTabIdByWorktree: { [worktreePath]: 'tab-1' },
    })
    applyTabsSnapshot({
      tabsByWorktree: { [worktreePath]: [editorTab(true, 'two')] },
      activeTabIdByWorktree: { [worktreePath]: 'tab-1' },
    })

    const pane = findLeaf(tabsByWorktree[worktreePath][0].rootSplit, 'pane-1')
    expect(pane?.editorFiles?.[0]).toMatchObject({ dirty: true, currentContent: 'two' })
  })
})

describe('updateSplitRatio', () => {
  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })

  const shell = (id: string): PaneSnapshot => ({
    id,
    sessionId: `session-${id}`,
    wsUrl: '',
    toolId: 'shell',
    toolName: 'Shell',
    isRunning: true,
    exitCode: null,
    title: null,
  })

  function nestedTab(innerRatio: number): TabSnapshot {
    return {
      id: 'tab-split',
      toolId: 'shell',
      toolName: 'Shell',
      name: 'Shell',
      worktreePath,
      focusedPaneId: 'a',
      rootSplit: {
        type: 'split',
        id: 'outer',
        direction: 'vertical',
        ratio: 0.5,
        first: { type: 'leaf', pane: shell('a') },
        second: {
          type: 'split',
          id: 'inner',
          direction: 'vertical',
          ratio: innerRatio,
          first: { type: 'leaf', pane: shell('b') },
          second: { type: 'leaf', pane: shell('c') },
        },
      },
    }
  }

  it('persists a ratio change of a nested split', async () => {
    vi.useFakeTimers()
    const api = {
      tabUpdateSplitRatio: vi.fn(async () => ({
        worktreePath,
        tabs: [nestedTab(0.3)],
        activeTabId: 'tab-split',
      })),
      tabSaveCurrentLayout: vi.fn(async () => undefined),
    }
    vi.stubGlobal('window', { api })
    applyTabsSnapshot({
      tabsByWorktree: { [worktreePath]: [nestedTab(0.5)] },
      activeTabIdByWorktree: { [worktreePath]: 'tab-split' },
    })

    updateSplitRatio(worktreePath, 'tab-split', 'inner', 0.3)
    await vi.advanceTimersByTimeAsync(600)

    expect(api.tabSaveCurrentLayout).toHaveBeenCalledWith(worktreePath)
  })
})
