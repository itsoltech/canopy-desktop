<script lang="ts">
  import SplitPaneContainer from '../terminal/SplitPaneContainer.svelte'
  import TabBar from '../terminal/TabBar.svelte'
  import Sidebar from '../sidebar/Sidebar.svelte'
  import CommandPalette from '../palette/CommandPalette.svelte'
  import ConfirmDialog from '../dialogs/ConfirmDialog.svelte'
  import InputDialog from '../dialogs/InputDialog.svelte'
  import CreateWorktreeModal from '../worktree/CreateWorktreeModal.svelte'
  import WelcomeDashboard from '../dashboard/WelcomeDashboard.svelte'
  import ClaudeInspector from '../claude/ClaudeInspector.svelte'
  import { dialogState, closeDialog } from '../../lib/stores/dialogs.svelte'
  import {
    workspaceState,
    openWorkspace,
    updateGitInfo,
    toggleSidebar,
    toggleInspector,
  } from '../../lib/stores/workspace.svelte'
  import {
    activeTabId,
    ensureShellTab,
    openTool,
    reopenClosedTab,
    switchTabByIndex,
    nextTab,
    prevTab,
    handlePtyExit,
    getAllTabs,
    getTabsForWorktree,
    splitFocusedPane,
    closeFocusedPane,
    navigatePaneFocus,
    focusPane,
    updateSplitRatio,
    focusSessionByPtyId,
  } from '../../lib/stores/tabs.svelte'
  import { allPanes } from '../../lib/stores/splitTree'
  import {
    claudeSessions,
    handleHookEvent,
    handleStatusUpdate,
    clearBadge,
    setBadge,
  } from '../../lib/claude/claudeState.svelte'

  const isMac = navigator.userAgent.includes('Mac')
  let paletteOpen = $state(false)

  let allTabs = $derived(getAllTabs())
  let currentActiveTabId = $derived(
    workspaceState.selectedWorktreePath
      ? (activeTabId[workspaceState.selectedWorktreePath] ?? null)
      : null,
  )
  let currentWorktreeTabs = $derived(
    workspaceState.selectedWorktreePath
      ? getTabsForWorktree(workspaceState.selectedWorktreePath)
      : [],
  )

  // Auto-create shell tab when selected worktree changes
  $effect(() => {
    const path = workspaceState.selectedWorktreePath
    if (path) {
      ensureShellTab(path)
    }
  })

  // Subscribe to git:changed push events
  $effect(() => {
    if (!workspaceState.repoRoot) return undefined
    const unsubscribe = window.api.onGitChanged((info) => {
      updateGitInfo(info as Parameters<typeof updateGitInfo>[0])
    })
    return unsubscribe
  })

  // Subscribe to pty:exit push events
  $effect(() => {
    const unsubscribe = window.api.onPtyExit((data) => {
      handlePtyExit(data.sessionId, data.exitCode)
    })
    return unsubscribe
  })

  // Subscribe to Claude hook events
  $effect(() => {
    const unsubscribe = window.api.onClaudeHookEvent((data) => {
      handleHookEvent(data.ptySessionId, data.event as Parameters<typeof handleHookEvent>[1])
      // Only set badge if this session is NOT the active tab
      if (data.ptySessionId !== activeClaudePtySessionId) {
        const name = (data.event as { hook_event_name?: string }).hook_event_name
        if (name === 'PermissionRequest') {
          setBadge(data.ptySessionId, 'permission')
        } else if (name === 'Stop' || name === 'PostToolUse') {
          setBadge(data.ptySessionId, 'unread')
        }
      }
    })
    return unsubscribe
  })

  // Subscribe to Claude status line updates
  $effect(() => {
    const unsubscribe = window.api.onClaudeStatusUpdate((data) => {
      handleStatusUpdate(data.ptySessionId, data.status as Parameters<typeof handleStatusUpdate>[1])
    })
    return unsubscribe
  })

  // Subscribe to Claude focus-session requests (notification clicks)
  $effect(() => {
    const unsubscribe = window.api.onClaudeFocusSession((data) => {
      focusSessionByPtyId(data.ptySessionId)
    })
    return unsubscribe
  })

  // Derive active Claude session from current tab
  let activeClaudePtySessionId = $derived.by(() => {
    if (!currentActiveTabId) return null
    const tab = allTabs.find((t) => t.id === currentActiveTabId)
    if (!tab || tab.toolId !== 'claude') return null
    const panes = allPanes(tab.rootSplit)
    const claudePane = panes.find((p) => p.toolId === 'claude')
    return claudePane?.sessionId ?? null
  })

  let activeClaudeState = $derived(
    activeClaudePtySessionId ? (claudeSessions[activeClaudePtySessionId] ?? null) : null,
  )

  // Clear badge when Claude tab is focused
  $effect(() => {
    if (activeClaudePtySessionId) {
      clearBadge(activeClaudePtySessionId)
    }
  })

  function handleLaunchTool(toolId: string): void {
    const path = workspaceState.selectedWorktreePath
    if (path) {
      openTool(toolId, path)
    }
  }

  // Global keyboard shortcuts
  function handleKeydown(e: KeyboardEvent): void {
    const mod = isMac ? e.metaKey : e.ctrlKey
    if (!mod) return

    // Cmd+K: toggle command palette
    if (e.key === 'k') {
      e.preventDefault()
      paletteOpen = !paletteOpen
      return
    }

    // Don't process other shortcuts while palette is open
    if (paletteOpen) return

    const path = workspaceState.selectedWorktreePath

    if (e.key === 'b') {
      e.preventDefault()
      toggleSidebar()
    }

    // Cmd+Shift+I: toggle Claude Inspector
    if ((e.key === 'I' || e.key === 'i') && e.shiftKey) {
      e.preventDefault()
      toggleInspector()
    }

    if (e.key === 'o') {
      e.preventDefault()
      window.api.openFolder().then((p) => {
        if (p) openWorkspace(p)
      })
    }

    if ((e.key === 't' || e.key === 'T') && path) {
      e.preventDefault()
      if (e.shiftKey) {
        reopenClosedTab(path)
      } else {
        openTool('shell', path)
      }
    }

    // Cmd+W: close focused pane (or tab if last pane)
    if (e.key === 'w' && path) {
      e.preventDefault()
      closeFocusedPane(path)
    }

    // Cmd+D: split vertical / Cmd+Shift+D: split horizontal
    if ((e.key === 'd' || e.key === 'D') && path) {
      e.preventDefault()
      splitFocusedPane(path, e.shiftKey ? 'hsplit' : 'vsplit')
    }

    // Cmd+Option+Arrow: navigate between panes
    if (e.altKey && path) {
      const dirMap: Record<string, 'left' | 'right' | 'up' | 'down'> = {
        ArrowLeft: 'left',
        ArrowRight: 'right',
        ArrowUp: 'up',
        ArrowDown: 'down',
      }
      const dir = dirMap[e.key]
      if (dir) {
        e.preventDefault()
        navigatePaneFocus(path, dir)
      }
    }

    // Cmd+1-9: switch to tab by index
    if (path && e.key >= '1' && e.key <= '9') {
      e.preventDefault()
      switchTabByIndex(path, parseInt(e.key) - 1)
    }

    // Cmd+Shift+[ and Cmd+Shift+]
    if (e.key === '[' && e.shiftKey && path) {
      e.preventDefault()
      prevTab(path)
    }

    if (e.key === ']' && e.shiftKey && path) {
      e.preventDefault()
      nextTab(path)
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if paletteOpen}
  <CommandPalette onClose={() => (paletteOpen = false)} />
{/if}

{#if dialogState.current.type === 'confirm'}
  <ConfirmDialog {...dialogState.current.props} />
{:else if dialogState.current.type === 'input'}
  <InputDialog {...dialogState.current.props} />
{:else if dialogState.current.type === 'createWorktree'}
  <CreateWorktreeModal onClose={closeDialog} />
{/if}

<div class="main-layout">
  {#if workspaceState.sidebarOpen && workspaceState.workspace}
    <Sidebar onLaunchTool={handleLaunchTool} />
  {/if}

  <div class="center-area">
    {#if workspaceState.selectedWorktreePath}
      <TabBar worktreePath={workspaceState.selectedWorktreePath} />
    {/if}

    <div class="terminal-area">
      {#each allTabs as tab (tab.id)}
        <div class="terminal-panel" class:hidden={tab.id !== currentActiveTabId}>
          <SplitPaneContainer
            node={tab.rootSplit}
            tabId={tab.id}
            worktreePath={tab.worktreePath}
            focusedPaneId={tab.focusedPaneId}
            active={tab.id === currentActiveTabId}
            onFocusPane={(paneId) => focusPane(tab.worktreePath, tab.id, paneId)}
            onUpdateRatio={(paneId, ratio) =>
              updateSplitRatio(tab.worktreePath, tab.id, paneId, ratio)}
          />
        </div>
      {/each}

      {#if !workspaceState.selectedWorktreePath && allTabs.length === 0}
        <WelcomeDashboard />
      {:else if workspaceState.selectedWorktreePath && currentWorktreeTabs.length === 0}
        <div class="empty-state">
          <p class="hint">
            Press {isMac ? 'Cmd' : 'Ctrl'}+T to open a shell
          </p>
          <p class="hint-sub">
            {isMac ? 'Cmd' : 'Ctrl'}+K to open command palette
          </p>
        </div>
      {/if}
    </div>
  </div>

  {#if workspaceState.inspectorOpen}
    {#if activeClaudeState && activeClaudePtySessionId}
      <ClaudeInspector state={activeClaudeState} />
    {:else}
      <aside class="inspector-empty">
        <p class="inspector-hint">No Claude session active</p>
        <p class="inspector-hint-sub">Open a Claude tab to see session details</p>
      </aside>
    {/if}
  {/if}
</div>

<style>
  .main-layout {
    display: flex;
    flex-direction: row;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .center-area {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .terminal-area {
    flex: 1;
    min-height: 0;
    position: relative;
  }

  .terminal-panel {
    position: absolute;
    inset: 0;
    background: #1e1e1e;
  }

  .terminal-panel.hidden {
    display: none;
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: rgba(255, 255, 255, 0.3);
  }

  .hint {
    font-size: 14px;
    font-weight: 400;
    margin: 0;
  }

  .hint-sub {
    font-size: 12px;
    font-weight: 400;
    margin: 6px 0 0;
    color: rgba(255, 255, 255, 0.2);
  }

  .inspector-empty {
    width: 280px;
    min-width: 280px;
    height: 100%;
    background: rgba(30, 30, 30, 0.75);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border-left: 1px solid rgba(255, 255, 255, 0.06);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
  }

  .inspector-hint {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.3);
    margin: 0;
  }

  .inspector-hint-sub {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.15);
    margin: 0;
  }
</style>
