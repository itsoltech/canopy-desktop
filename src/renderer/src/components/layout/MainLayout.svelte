<script lang="ts">
  import SplitPaneContainer from '../terminal/SplitPaneContainer.svelte'
  import TabBar from '../terminal/TabBar.svelte'
  import Sidebar from '../sidebar/Sidebar.svelte'
  import CommandPalette from '../palette/CommandPalette.svelte'
  import ConfirmDialog from '../dialogs/ConfirmDialog.svelte'
  import InputDialog from '../dialogs/InputDialog.svelte'
  import CreateWorktreeModal from '../worktree/CreateWorktreeModal.svelte'
  import PreferencesModal from '../preferences/PreferencesModal.svelte'
  import AboutModal from '../dialogs/AboutModal.svelte'
  import WelcomeDashboard from '../dashboard/WelcomeDashboard.svelte'
  import Toast from '../shared/Toast.svelte'
  import {
    dialogState,
    closeDialog,
    showPreferences,
    showAbout,
  } from '../../lib/stores/dialogs.svelte'
  import {
    workspaceState,
    projects,
    attachProject,
    updateGitInfoForProject,
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
    saveAllLayouts,
  } from '../../lib/stores/tabs.svelte'
  import { findLeaf } from '../../lib/stores/splitTree'
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

  // Subscribe to git:changed push events (all projects)
  $effect(() => {
    const unsubscribe = window.api.onGitChanged((info) => {
      if (info.repoRoot) {
        updateGitInfoForProject(info.repoRoot, info)
      }
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
      const session = claudeSessions[data.ptySessionId]
      const prevSessionId = session?.claudeSessionId
      handleHookEvent(data.ptySessionId, data.event as Parameters<typeof handleHookEvent>[1])
      // Persist layout when Claude session ID changes (e.g. UUID -> slug)
      if (session && session.claudeSessionId !== prevSessionId && session.claudeSessionId) {
        saveAllLayouts()
      }
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

  // Subscribe to URL action events
  $effect(() => {
    const unsubscribe = window.api.onUrlAction(async (data) => {
      if (data.path) {
        await attachProject(data.path)
        if (data.tool && workspaceState.selectedWorktreePath) {
          openTool(data.tool, workspaceState.selectedWorktreePath).catch((err) => {
            console.error(`Failed to launch tool '${data.tool}':`, err)
          })
        }
      }
    })
    return unsubscribe
  })

  // Subscribe to menu:showAbout from native menu
  $effect(() => {
    return window.api.onMenuShowAbout(() => showAbout())
  })

  // Subscribe to menu:showPreferences from native menu (Windows File menu)
  $effect(() => {
    return window.api.onMenuShowPreferences(() => showPreferences())
  })

  // Save layouts on window close
  $effect(() => {
    const handler = (): void => saveAllLayouts()
    window.addEventListener('beforeunload', handler)
    return () => window.removeEventListener('beforeunload', handler)
  })

  // Freeze/unfreeze browser views when modals/palette are open
  $effect(() => {
    const anyOverlayOpen = dialogState.current.type !== 'none' || paletteOpen
    window.dispatchEvent(
      new CustomEvent(anyOverlayOpen ? 'canopy:freeze-browsers' : 'canopy:unfreeze-browsers'),
    )
  })

  // Derive active tab and focused pane info
  let activeTab = $derived(allTabs.find((t) => t.id === currentActiveTabId) ?? null)
  let focusedPane = $derived(
    activeTab ? findLeaf(activeTab.rootSplit, activeTab.focusedPaneId) : null,
  )

  // Derive active Claude session from focused pane
  let activeClaudePtySessionId = $derived(
    focusedPane?.toolId === 'claude' ? focusedPane.sessionId : null,
  )

  // Clear badge when Claude pane is focused
  $effect(() => {
    if (activeClaudePtySessionId) {
      clearBadge(activeClaudePtySessionId)
    }
  })

  function handleLaunchTool(toolId: string): void {
    const path = workspaceState.selectedWorktreePath
    if (path) {
      openTool(toolId, path).catch((err) => {
        console.error(`Failed to launch tool '${toolId}':`, err)
      })
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

    // Cmd+Shift+N: new window
    if ((e.key === 'N' || e.key === 'n') && e.shiftKey) {
      e.preventDefault()
      window.api.newWindow()
      return
    }

    // Don't process other shortcuts while palette is open
    if (paletteOpen) return

    const path = workspaceState.selectedWorktreePath

    if (e.key === 'b') {
      e.preventDefault()
      toggleSidebar()
    }

    // Cmd+,: open preferences
    if (e.key === ',') {
      e.preventDefault()
      showPreferences()
    }

    // Cmd+Shift+I: toggle Claude Inspector on focused pane
    if ((e.key === 'I' || e.key === 'i') && e.shiftKey) {
      e.preventDefault()
      toggleInspector()
    }

    // Cmd+L: focus browser URL bar (when active tab is browser)
    if (e.key === 'l' && activeTab?.toolId === 'browser') {
      e.preventDefault()
      window.dispatchEvent(new CustomEvent('canopy:focus-url-bar'))
      return
    }

    if (e.key === 'o') {
      e.preventDefault()
      window.api.openFolder().then((p) => {
        if (p) attachProject(p)
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
  <CreateWorktreeModal
    onClose={closeDialog}
    repoRoot={dialogState.current.repoRoot}
    workspaceId={dialogState.current.workspaceId}
  />
{:else if dialogState.current.type === 'preferences'}
  <PreferencesModal />
{:else if dialogState.current.type === 'about'}
  <AboutModal />
{/if}

<Toast />

<div class="main-layout">
  {#if workspaceState.sidebarOpen && projects.length > 0}
    <Sidebar onLaunchTool={handleLaunchTool} />
  {/if}

  <div class="center-area">
    {#if workspaceState.selectedWorktreePath}
      <TabBar worktreePath={workspaceState.selectedWorktreePath} />
    {/if}

    <div class="content-row">
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

        {#if projects.length === 0 && allTabs.length === 0}
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
  </div>
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

  .content-row {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: row;
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
</style>
