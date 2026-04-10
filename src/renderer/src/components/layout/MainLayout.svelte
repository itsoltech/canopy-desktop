<script lang="ts">
  import { match } from 'ts-pattern'
  import { onMount } from 'svelte'
  import SplitPaneContainer from '../terminal/SplitPaneContainer.svelte'
  import TabBar from '../terminal/TabBar.svelte'
  import Sidebar from '../sidebar/Sidebar.svelte'
  import CommandPalette from '../palette/CommandPalette.svelte'
  import ConfirmDialog from '../dialogs/ConfirmDialog.svelte'
  import InputDialog from '../dialogs/InputDialog.svelte'
  import CreateWorktreeModal from '../worktree/CreateWorktreeModal.svelte'
  import PreferencesModal from '../preferences/PreferencesModal.svelte'
  import AboutModal from '../dialogs/AboutModal.svelte'
  import ChangelogModal from '../dialogs/ChangelogModal.svelte'
  import TaskPickerModal from '../taskTracker/TaskPickerModal.svelte'
  import OnboardingWizard from '../onboarding/OnboardingWizard.svelte'
  import FeatureOnboarding from '../onboarding/FeatureOnboarding.svelte'
  import TmuxSessionBrowser from '../terminal/TmuxSessionBrowser.svelte'
  import CreatePRModal from '../github/CreatePRModal.svelte'
  import RemoteConnectionModal from '../dialogs/RemoteConnectionModal.svelte'
  import RemoteAcceptDeviceModal from '../dialogs/RemoteAcceptDeviceModal.svelte'
  import RunConfigEditorModal from '../runConfig/RunConfigEditorModal.svelte'
  import RunConfigManagerModal from '../runConfig/RunConfigManagerModal.svelte'
  import WelcomeDashboard from '../dashboard/WelcomeDashboard.svelte'
  import RightPanel from './RightPanel.svelte'
  import Toast from '../shared/Toast.svelte'
  import { getPref, setPref } from '../../lib/stores/preferences.svelte'
  import {
    dialogState,
    closeDialog,
    showPreferences,
    showAbout,
    showChangelog,
    showOnboardingWizard,
    showFeatureOnboarding,
  } from '../../lib/stores/dialogs.svelte'
  import {
    workspaceState,
    projects,
    attachProject,
    restoreProjects,
    updateGitInfoForProject,
    toggleSidebar,
    toggleRightPanel,
  } from '../../lib/stores/workspace.svelte'
  import {
    activeTabId,
    ensureDefaultTab,
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
  import { initRemoteSessionListeners } from '../../lib/stores/remoteSession.svelte'
  import {
    agentSessions,
    handleHookEvent,
    handleStatusUpdate,
    clearBadge,
    setBadge,
    setWorktreeBadge,
    clearWorktreeBadge,
  } from '../../lib/agents/agentState.svelte'
  import { findWorktreeForSession } from '../../lib/stores/tabs.svelte'
  import { initToolStore, destroyToolStore } from '../../lib/stores/tools.svelte'
  import { initSkillStore, destroySkillStore } from '../../lib/stores/skills.svelte'

  onMount(() => {
    initToolStore()
    initSkillStore()
    const stopRemoteListeners = initRemoteSessionListeners()
    return () => {
      destroySkillStore()
      stopRemoteListeners()
      destroyToolStore()
    }
  })

  const isMac = navigator.userAgent.includes('Mac')
  let paletteOpen = $state(false)

  // Sidebar resize state
  const SIDEBAR_MIN = 150
  const SIDEBAR_MAX = 600
  let sidebarWidth = $state(parseInt(getPref('sidebar.width', '220'), 10) || 220)
  let sidebarDragging = $state(false)
  let sidebarDragStart = 0

  function handleSidebarPointerDown(e: PointerEvent): void {
    e.preventDefault()
    const target = e.currentTarget as HTMLElement
    target.setPointerCapture(e.pointerId)
    sidebarDragging = true
    sidebarDragStart = e.clientX
  }

  function handleSidebarPointerMove(e: PointerEvent): void {
    if (!sidebarDragging) return
    const delta = e.clientX - sidebarDragStart
    if (delta !== 0) {
      sidebarWidth = Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, sidebarWidth + delta))
      sidebarDragStart = e.clientX
    }
  }

  function handleSidebarPointerUp(): void {
    if (sidebarDragging) {
      sidebarDragging = false
      setPref('sidebar.width', String(sidebarWidth))
    }
  }

  // Right panel resize state
  const RPANEL_MIN = 200
  const RPANEL_MAX = 500
  let rightPanelWidth = $state(parseInt(getPref('rightPanel.width', '280'), 10) || 280)
  let rpDragging = $state(false)
  let rpDragStart = 0

  function handleRpPointerDown(e: PointerEvent): void {
    e.preventDefault()
    const target = e.currentTarget as HTMLElement
    target.setPointerCapture(e.pointerId)
    rpDragging = true
    rpDragStart = e.clientX
  }

  function handleRpPointerMove(e: PointerEvent): void {
    if (!rpDragging) return
    const delta = rpDragStart - e.clientX
    if (delta !== 0) {
      rightPanelWidth = Math.min(RPANEL_MAX, Math.max(RPANEL_MIN, rightPanelWidth + delta))
      rpDragStart = e.clientX
    }
  }

  function handleRpPointerUp(): void {
    if (rpDragging) {
      rpDragging = false
      setPref('rightPanel.width', String(rightPanelWidth))
    }
  }

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

  // Auto-create default tab when selected worktree changes
  $effect(() => {
    const path = workspaceState.selectedWorktreePath
    if (path) {
      ensureDefaultTab(path)
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
      handlePtyExit(data.sessionId, data.exitCode, data.tmuxSessionName)
    })
    return unsubscribe
  })

  // Subscribe to agent hook events
  $effect(() => {
    const unsubscribe = window.api.onAgentHookEvent((data) => {
      const session = agentSessions[data.ptySessionId]
      const prevSessionId = session?.agentSessionId
      handleHookEvent(data.ptySessionId, data.event as Parameters<typeof handleHookEvent>[1])
      // Persist layout when agent session ID changes (e.g. UUID -> slug)
      if (session && session.agentSessionId !== prevSessionId && session.agentSessionId) {
        saveAllLayouts()
      }
      const normalized = data.event as { event?: string }
      const badge = match(normalized.event)
        .with('PermissionRequest', () => 'permission' as const)
        .with('Idle', 'AfterToolUse', () => 'unread' as const)
        .otherwise(() => null)
      // Only set badge if this session is NOT the active tab
      if (badge && data.ptySessionId !== activeAgentPtySessionId) {
        setBadge(data.ptySessionId, badge)
      }
      // Set worktree badge if session is in a non-selected worktree
      const sessionWorktreePath = findWorktreeForSession(data.ptySessionId)
      if (
        badge &&
        sessionWorktreePath &&
        sessionWorktreePath !== workspaceState.selectedWorktreePath
      ) {
        setWorktreeBadge(sessionWorktreePath, badge)
      }
    })
    return unsubscribe
  })

  // Subscribe to agent status line updates
  $effect(() => {
    const unsubscribe = window.api.onAgentStatusUpdate((data) => {
      handleStatusUpdate(data.ptySessionId, data.status as Parameters<typeof handleStatusUpdate>[1])
    })
    return unsubscribe
  })

  // Subscribe to agent focus-session requests (notification clicks)
  $effect(() => {
    const unsubscribe = window.api.onAgentFocusSession((data) => {
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

  // Notify browser panes when app-level overlays open/close so they can hide
  // DevTools WebContentsView (native layer that paints above DOM modals)
  $effect(() => {
    const anyOverlayOpen = dialogState.current.type !== 'none' || paletteOpen
    window.dispatchEvent(
      new CustomEvent('canopy:app-overlay', { detail: { open: anyOverlayOpen } }),
    )
  })

  // Restore a whole window's projects in parallel, then focus the saved worktree once.
  $effect(() => {
    const unsubscribe = window.api.onRestoreWindow(async (data) => {
      await restoreProjects(data.paths, data.activeWorktreePath, data.removedPaths)
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

  // Subscribe to onboarding push event
  $effect(() => {
    return window.api.onShowOnboarding(async (data) => {
      const { initOnboarding, onboardingState } = await import('../../lib/stores/onboarding.svelte')
      await initOnboarding(data.mode, data.fromVersion)
      if (onboardingState.mode === 'none' && data.fromVersion) {
        // No onboarding steps to show, fall back to changelog
        showChangelog(data.fromVersion)
      } else if (onboardingState.mode === 'first-launch') {
        showOnboardingWizard()
      } else if (onboardingState.mode === 'upgrade') {
        showFeatureOnboarding(data.fromVersion ?? '')
      }
    })
  })

  // Subscribe to post-update changelog push event
  $effect(() => {
    return window.api.onShowChangelog((data) => {
      showChangelog(data.fromVersion)
    })
  })

  // Save layouts on window close
  $effect(() => {
    const handler = (): void => saveAllLayouts()
    window.addEventListener('beforeunload', handler)
    return () => window.removeEventListener('beforeunload', handler)
  })

  // Derive active tab and focused pane info
  let activeTab = $derived(allTabs.find((t) => t.id === currentActiveTabId) ?? null)
  let focusedPane = $derived(
    activeTab ? findLeaf(activeTab.rootSplit, activeTab.focusedPaneId) : null,
  )

  // Derive active agent session from focused pane
  let activeAgentPtySessionId = $derived(
    focusedPane && agentSessions[focusedPane.sessionId] ? focusedPane.sessionId : null,
  )
  let activeAgentState = $derived(
    activeAgentPtySessionId ? agentSessions[activeAgentPtySessionId] : null,
  )

  // Clear badge when agent pane is focused
  $effect(() => {
    if (activeAgentPtySessionId) {
      clearBadge(activeAgentPtySessionId)
    }
  })

  // Clear worktree badge when worktree is selected
  $effect(() => {
    const path = workspaceState.selectedWorktreePath
    if (path) clearWorktreeBadge(path)
  })

  // Notify main process about focused agent session for notch peek suppression
  $effect(() => {
    window.api.setFocusedAgentSession(activeAgentPtySessionId)
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

    // Cmd+Shift+I: toggle Agent Inspector on focused pane
    if ((e.key === 'I' || e.key === 'i') && e.shiftKey) {
      e.preventDefault()
      toggleRightPanel()
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
        openTool(getPref('newTab.toolId', 'shell'), path).catch((err) => {
          console.error('Failed to open new tab:', err)
        })
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
      switchTabByIndex(path, parseInt(e.key) - 1).catch((err) =>
        console.error('switchTabByIndex failed:', err),
      )
    }

    // Cmd+Shift+[ and Cmd+Shift+]
    if (e.key === '[' && e.shiftKey && path) {
      e.preventDefault()
      prevTab(path).catch((err) => console.error('prevTab failed:', err))
    }

    if (e.key === ']' && e.shiftKey && path) {
      e.preventDefault()
      nextTab(path).catch((err) => console.error('nextTab failed:', err))
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
    baseBranch={dialogState.current.baseBranch}
  />
{:else if dialogState.current.type === 'preferences'}
  <PreferencesModal section={dialogState.current.section} />
{:else if dialogState.current.type === 'taskPicker'}
  <TaskPickerModal connectionId={dialogState.current.connectionId} />
{:else if dialogState.current.type === 'about'}
  <AboutModal />
{:else if dialogState.current.type === 'changelog'}
  <ChangelogModal fromVersion={dialogState.current.fromVersion} />
{:else if dialogState.current.type === 'onboardingWizard'}
  <OnboardingWizard />
{:else if dialogState.current.type === 'featureOnboarding'}
  <FeatureOnboarding fromVersion={dialogState.current.fromVersion} />
{:else if dialogState.current.type === 'tmuxBrowser'}
  <TmuxSessionBrowser />
{:else if dialogState.current.type === 'createGitHubPR'}
  <CreatePRModal />
{:else if dialogState.current.type === 'remoteConnection'}
  <RemoteConnectionModal />
{:else if dialogState.current.type === 'remoteAcceptDevice'}
  <RemoteAcceptDeviceModal
    deviceId={dialogState.current.deviceId}
    deviceName={dialogState.current.deviceName}
    fingerprint={dialogState.current.fingerprint}
  />
{:else if dialogState.current.type === 'runConfigEditor'}
  <RunConfigEditorModal
    configDir={dialogState.current.configDir}
    configName={dialogState.current.configName}
  />
{:else if dialogState.current.type === 'runConfigManager'}
  <RunConfigManagerModal
    initialConfigDir={dialogState.current.selectConfigDir}
    initialConfigName={dialogState.current.selectConfigName}
  />
{/if}

<Toast />

<main class="main-layout">
  {#if workspaceState.sidebarOpen && projects.length > 0}
    <Sidebar onLaunchTool={handleLaunchTool} width={sidebarWidth} />
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="sidebar-resize-handle"
      class:dragging={sidebarDragging}
      onpointerdown={handleSidebarPointerDown}
      onpointermove={handleSidebarPointerMove}
      onpointerup={handleSidebarPointerUp}
      onpointercancel={handleSidebarPointerUp}
    ></div>
  {/if}

  <div class="center-area">
    {#if workspaceState.selectedWorktreePath}
      <TabBar worktreePath={workspaceState.selectedWorktreePath} />
    {/if}

    <div class="content-row">
      <div class="terminal-area">
        {#each allTabs as tab (tab.id)}
          {#if !tab.suspended}
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
          {/if}
        {/each}

        {#if projects.length === 0 && allTabs.length === 0}
          <WelcomeDashboard />
        {:else if workspaceState.selectedWorktreePath && currentWorktreeTabs.length === 0}
          <div class="empty-state">
            <p class="hint">
              Press {isMac ? 'Cmd' : 'Ctrl'}+T to open a new tab
            </p>
            <p class="hint-sub">
              {isMac ? 'Cmd' : 'Ctrl'}+K to open command palette
            </p>
          </div>
        {/if}
      </div>

      {#if workspaceState.rightPanelOpen && workspaceState.selectedWorktreePath}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="right-resize-handle"
          class:dragging={rpDragging}
          onpointerdown={handleRpPointerDown}
          onpointermove={handleRpPointerMove}
          onpointerup={handleRpPointerUp}
          onpointercancel={handleRpPointerUp}
        ></div>
        <RightPanel
          agentState={activeAgentState}
          width={rightPanelWidth}
          worktreePath={workspaceState.selectedWorktreePath ?? ''}
        />
      {/if}
    </div>
  </div>
</main>

<style>
  .main-layout {
    display: flex;
    flex-direction: row;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .sidebar-resize-handle {
    width: 4px;
    cursor: col-resize;
    background: transparent;
    transition: background 0.15s;
    flex-shrink: 0;
  }

  .sidebar-resize-handle:hover,
  .sidebar-resize-handle.dragging {
    background: var(--c-accent-muted);
  }

  .right-resize-handle {
    width: 1px;
    cursor: col-resize;
    background: transparent;
    flex-shrink: 0;
    position: relative;
  }

  .right-resize-handle::after {
    content: '';
    position: absolute;
    inset: 0 -3px;
    cursor: col-resize;
  }

  .right-resize-handle:hover,
  .right-resize-handle.dragging {
    background: var(--c-accent-muted);
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
    background: var(--c-bg);
  }

  .terminal-panel.hidden {
    display: none;
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--c-text-faint);
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
    color: var(--c-text-faint);
  }
</style>
