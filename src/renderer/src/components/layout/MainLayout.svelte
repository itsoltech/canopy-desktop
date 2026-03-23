<script lang="ts">
  import TerminalInstance from '../../lib/terminal/TerminalInstance.svelte'
  import Sidebar from '../sidebar/Sidebar.svelte'
  import {
    workspaceState,
    openWorkspace,
    updateGitInfo,
    toggleSidebar
  } from '../../lib/stores/workspace.svelte'
  import { sessions, ensureSession } from '../../lib/stores/sessions.svelte'

  const isMac = navigator.userAgent.includes('Mac')

  // Auto-create session when selected worktree changes
  $effect(() => {
    const path = workspaceState.selectedWorktreePath
    if (path) {
      ensureSession(path)
    }
  })

  // Subscribe to git:changed push events
  $effect(() => {
    if (!workspaceState.repoRoot) return
    const unsubscribe = window.api.onGitChanged((info) => {
      updateGitInfo(info as Parameters<typeof updateGitInfo>[0])
    })
    return unsubscribe
  })

  // Global keyboard shortcuts
  function handleKeydown(e: KeyboardEvent): void {
    const mod = isMac ? e.metaKey : e.ctrlKey
    if (!mod) return

    if (e.key === 'b') {
      e.preventDefault()
      toggleSidebar()
    }

    if (e.key === 'o') {
      e.preventDefault()
      window.api.openFolder().then((path) => {
        if (path) openWorkspace(path)
      })
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="main-layout">
  {#if workspaceState.sidebarOpen}
    <Sidebar />
  {/if}

  <div class="center-area">
    {#each Object.entries(sessions) as [path, session] (path)}
      <div class="terminal-panel" class:hidden={path !== workspaceState.selectedWorktreePath}>
        <TerminalInstance sessionId={session.sessionId} wsUrl={session.wsUrl} />
      </div>
    {/each}

    {#if Object.keys(sessions).length === 0}
      <div class="empty-state">
        <p class="hint">Press {isMac ? 'Cmd' : 'Ctrl'}+O to open a folder</p>
      </div>
    {/if}
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
    position: relative;
  }

  .terminal-panel {
    position: absolute;
    inset: 0;
  }

  .terminal-panel.hidden {
    visibility: hidden;
    pointer-events: none;
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
