<script lang="ts">
  import TerminalInstance from '../../lib/terminal/TerminalInstance.svelte'
  import TabBar from '../terminal/TabBar.svelte'
  import ExitBanner from '../terminal/ExitBanner.svelte'
  import Sidebar from '../sidebar/Sidebar.svelte'
  import {
    workspaceState,
    openWorkspace,
    updateGitInfo,
    toggleSidebar
  } from '../../lib/stores/workspace.svelte'
  import {
    activeTabId,
    ensureShellTab,
    openTool,
    closeTab,
    reopenClosedTab,
    switchTabByIndex,
    nextTab,
    prevTab,
    handlePtyExit,
    restartTab,
    getAllTabs
  } from '../../lib/stores/tabs.svelte'

  const isMac = navigator.userAgent.includes('Mac')

  let allTabs = $derived(getAllTabs())
  let currentActiveTabId = $derived(
    workspaceState.selectedWorktreePath
      ? (activeTabId[workspaceState.selectedWorktreePath] ?? null)
      : null
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

    const path = workspaceState.selectedWorktreePath

    if (e.key === 'b') {
      e.preventDefault()
      toggleSidebar()
    }

    if (e.key === 'o') {
      e.preventDefault()
      window.api.openFolder().then((p) => {
        if (p) openWorkspace(p)
      })
    }

    if (e.key === 't' && !e.shiftKey && path) {
      e.preventDefault()
      openTool('shell', path)
    }

    if (e.key === 'T' && e.shiftKey && path) {
      e.preventDefault()
      reopenClosedTab(path)
    }

    if (e.key === 'w' && path) {
      e.preventDefault()
      const id = activeTabId[path]
      if (id) closeTab(id)
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

<div class="main-layout">
  {#if workspaceState.sidebarOpen}
    <Sidebar onLaunchTool={handleLaunchTool} />
  {/if}

  <div class="center-area">
    {#if workspaceState.selectedWorktreePath}
      <TabBar worktreePath={workspaceState.selectedWorktreePath} />
    {/if}

    <div class="terminal-area">
      {#each allTabs as tab (tab.id)}
        <div class="terminal-panel" class:hidden={tab.id !== currentActiveTabId}>
          <TerminalInstance
            sessionId={tab.sessionId}
            wsUrl={tab.wsUrl}
            active={tab.id === currentActiveTabId}
          />
          {#if !tab.isRunning}
            <ExitBanner exitCode={tab.exitCode} onRestart={() => restartTab(tab.id)} />
          {/if}
        </div>
      {/each}

      {#if allTabs.length === 0 && !workspaceState.selectedWorktreePath}
        <div class="empty-state">
          <p class="hint">Press {isMac ? 'Cmd' : 'Ctrl'}+O to open a folder</p>
        </div>
      {/if}
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
  }
</style>
