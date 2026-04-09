<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { match } from 'ts-pattern'
  import { REMOTE_PROTOCOL_VERSION } from '../../../renderer-shared/rpc/protocol'
  import { PeerController, type PeerPhase } from './lib/peer/PeerController'
  import type { RemoteApi } from './lib/api'
  import { mirrorState } from './lib/state/mirrorState.svelte'
  import { createViewportTracker, type ViewportTracker } from './lib/viewportTracker.svelte'
  import RemoteTerminalView from './components/RemoteTerminalView.svelte'

  // ===== controller lifecycle =====

  let controller: PeerController | null = null
  let phase: PeerPhase = $state({ kind: 'init' })
  let remoteApi: RemoteApi | null = $state(null)

  // Diagnostic ping loop — proves the typed RPC pipeline is alive end-to-end.
  let pingIntervalId: ReturnType<typeof setInterval> | null = null
  let lastLatencyMs: number | null = $state(null)
  let pingCount = $state(0)
  let pingError: string | null = $state(null)

  // ===== viewport detection (mobile vs desktop layout) =====

  // Initialise from `matchMedia` synchronously so the first paint already
  // picks the right layout — otherwise a desktop user sees a phone-shaped
  // mobile view flicker into the wide layout on mount.
  let isDesktop = $state(
    typeof window !== 'undefined' && window.matchMedia('(min-width: 768px)').matches,
  )

  // ===== soft keyboard tracking (mobile only) =====

  // Tracks the on-screen keyboard height via `window.visualViewport` so we
  // can shrink the shell to avoid the xterm preview hiding underneath the
  // keyboard when the user taps into the terminal. The helper has an
  // internal SSR / feature-detection guard (returns a no-op tracker when
  // `window.visualViewport` is missing) so it's safe to call at component
  // init time instead of deferring to `onMount`.
  const viewportTracker: ViewportTracker = createViewportTracker()
  // Reading the getter inside `$derived` establishes a reactive dependency
  // on the tracker's internal `$state`, so `keyboardHeight` updates whenever
  // the keyboard opens/closes without us having to wire a manual effect.
  let keyboardHeight = $derived(viewportTracker.keyboardHeight)
  /** Visual viewport height in CSS pixels. Used to size the shell so it
   *  matches exactly the visible area above the on-screen keyboard. */
  let visualHeight = $derived(viewportTracker.visualHeight)
  let isKeyboardOpen = $derived(viewportTracker.isKeyboardOpen)

  // ===== drill-down + selection state =====

  let selectedProjectId = $state<string | null>(null)
  let selectedWorktreePath = $state<string | null>(null)
  /**
   * Session id of the terminal currently shown in fullscreen mode (modal
   * overlay covering the whole viewport). When null, the active tab's
   * terminal preview is rendered *inline* inside the workspace pane.
   *
   * Mutex with the inline preview: at most one `RemoteTerminalView` is
   * mounted for a given sessionId at a time, so we don't double-subscribe
   * to `pty.data.<sessionId>` and risk one component disposing the shared
   * forwarder out from under the other.
   */
  let fullscreenSessionId: string | null = $state(null)

  // Spawn / close action feedback. Agent input is gone — the inline xterm
  // preview already accepts keyboard input (it pipes term.onData → pty.write),
  // so a separate "Send prompt" footer would just duplicate that channel.
  let actionError: string | null = $state(null)
  let lastAction: string | null = $state(null)

  // Bottom sheet visibility for the mobile "New tool" picker. On mobile
  // viewports the spawn-strip can overflow to two rows when there are more
  // than 2-3 available tools, so we collapse them behind a single "New tool"
  // button that opens this sheet.
  let toolPickerOpen = $state(false)

  // One-shot auto-select after first hydration so mobile users land on the
  // host's currently active worktree. Without the guard, tapping "← Projects"
  // would clear the selection and the effect would immediately re-pin it.
  let autoSelectDone = $state(false)

  onMount(() => {
    // viewport listener — keeps `isDesktop` in sync if the user resizes the
    // browser window or rotates a tablet.
    const mq = window.matchMedia('(min-width: 768px)')
    isDesktop = mq.matches
    const onMqChange = (e: MediaQueryListEvent): void => {
      isDesktop = e.matches
    }
    mq.addEventListener('change', onMqChange)

    try {
      controller = new PeerController()
      controller.onPhaseChange = (p) => {
        phase = p
      }
      controller.onApiReady = (api) => {
        remoteApi = api
        startPingLoop(api)
      }
      controller.start(window.location)
    } catch (e) {
      // Catch any synchronous throw from `PeerController.start` (e.g.
      // WebSocket constructor failing due to CSP or invalid URL) so the
      // error surfaces as a visible error phase instead of a blank
      // "Connecting…" screen stuck forever.
      console.error('[peer] controller.start threw:', e)
      phase = {
        kind: 'error',
        message: `Failed to start: ${e instanceof Error ? e.message : String(e)}`,
      }
    }

    return () => {
      mq.removeEventListener('change', onMqChange)
    }
  })

  onDestroy(() => {
    stopPingLoop()
    controller?.dispose()
    controller = null
    viewportTracker.dispose()
  })

  function startPingLoop(api: RemoteApi): void {
    stopPingLoop()
    void runPing(api)
    pingIntervalId = setInterval(() => void runPing(api), 3000)
  }

  function stopPingLoop(): void {
    if (pingIntervalId !== null) {
      clearInterval(pingIntervalId)
      pingIntervalId = null
    }
  }

  async function runPing(api: RemoteApi): Promise<void> {
    const start = performance.now()
    try {
      const result = await api.diag.ping(pingCount + 1)
      lastLatencyMs = Math.round(performance.now() - start)
      pingCount = result.n
      pingError = null
    } catch (e) {
      pingError = e instanceof Error ? e.message : String(e)
    }
  }

  // ===== status derivations =====

  let statusHeadline = $derived.by(() =>
    match(phase)
      .with({ kind: 'init' }, () => 'Starting…')
      .with({ kind: 'connecting-signaling' }, () => 'Connecting to host…')
      .with({ kind: 'awaiting-paired' }, () => 'Sending pairing token…')
      .with({ kind: 'awaiting-accept' }, () => 'Waiting for host approval')
      .with({ kind: 'negotiating' }, () => 'Negotiating connection…')
      .with({ kind: 'connected' }, () => 'Connected')
      .with({ kind: 'rejected' }, () => 'Pairing rejected')
      .with({ kind: 'disconnected' }, () => 'Disconnected')
      .with({ kind: 'error' }, () => 'Connection error')
      .exhaustive(),
  )

  let statusDetail = $derived.by(() =>
    match(phase)
      .with({ kind: 'init' }, () => 'Initializing client…')
      .with({ kind: 'connecting-signaling' }, () => 'Opening WebSocket to host…')
      .with({ kind: 'awaiting-paired' }, () => 'Validating pairing token…')
      .with(
        { kind: 'awaiting-accept' },
        () => 'Click Accept on the host dialog to grant this device access.',
      )
      .with({ kind: 'negotiating' }, () => 'Exchanging SDP and ICE candidates…')
      .with(
        { kind: 'connected' },
        () => 'Data channels are open. The mirror updates live as the host state changes.',
      )
      .with({ kind: 'rejected' }, (p) => p.reason)
      .with(
        { kind: 'disconnected' },
        () => 'The connection was closed. Scan the QR code again to retry.',
      )
      .with({ kind: 'error' }, (p) => p.message)
      .exhaustive(),
  )

  let dotKind = $derived.by(() =>
    match(phase.kind)
      .with('init', 'connecting-signaling', 'awaiting-paired', 'negotiating', () => 'progress')
      .with('awaiting-accept', () => 'progress')
      .with('connected', () => 'ok')
      .with('rejected', () => 'error')
      .with('disconnected', () => 'muted')
      .with('error', () => 'error')
      .exhaustive(),
  )

  // ===== auto-select active worktree on first hydration =====

  $effect(() => {
    if (!mirrorState.hydrated) return
    if (autoSelectDone) return
    autoSelectDone = true
    const active = mirrorState.activeWorktreePath
    if (!active) return
    const owning = mirrorState.projects.find((p) =>
      p.isGitRepo ? p.worktrees.some((wt) => wt.path === active) : p.path === active,
    )
    if (owning) {
      selectedProjectId = owning.id
      selectedWorktreePath = active
    }
  })

  // ===== layout visibility (drill-down on mobile, side-by-side on desktop) =====

  let showSidebar = $derived(isDesktop || selectedProjectId === null)
  let showMainPane = $derived(isDesktop || selectedProjectId !== null)

  // ===== derived selection =====

  let selectedProject = $derived(
    selectedProjectId !== null
      ? (mirrorState.projects.find((p) => p.id === selectedProjectId) ?? null)
      : null,
  )
  let selectedTabs = $derived(
    selectedWorktreePath !== null ? (mirrorState.tabsByWorktree[selectedWorktreePath] ?? []) : [],
  )
  let selectedActiveTabId = $derived(
    selectedWorktreePath !== null ? mirrorState.activeTabByWorktree[selectedWorktreePath] : null,
  )
  let selectedActiveTab = $derived(
    selectedActiveTabId ? selectedTabs.find((t) => t.id === selectedActiveTabId) : undefined,
  )

  let workspaceLabel = $derived.by(() => {
    if (!selectedProject) return ''
    if (!selectedWorktreePath || !selectedProject.isGitRepo) return selectedProject.name
    const wt = selectedProject.worktrees.find((w) => w.path === selectedWorktreePath)
    return `${selectedProject.name} · ${wt?.branch ?? '—'}`
  })

  // ===== actions =====

  function selectProject(projectId: string): void {
    selectedProjectId = projectId
    const project = mirrorState.projects.find((p) => p.id === projectId)
    if (!project) return
    if (project.isGitRepo) {
      const main = project.worktrees.find((wt) => wt.isMain) ?? project.worktrees[0]
      selectedWorktreePath = main?.path ?? null
    } else {
      selectedWorktreePath = project.path
    }
    // Switching projects always closes any fullscreen terminal — its
    // sessionId belongs to a different worktree.
    fullscreenSessionId = null
  }

  function selectWorktreeRow(path: string): void {
    selectedWorktreePath = path
    fullscreenSessionId = null
  }

  function backToProjects(): void {
    selectedProjectId = null
    selectedWorktreePath = null
    fullscreenSessionId = null
  }

  async function runAction(label: string, action: () => Promise<void>): Promise<void> {
    if (!remoteApi) return
    lastAction = null
    actionError = null
    try {
      await action()
      lastAction = label
    } catch (e) {
      actionError = e instanceof Error ? e.message : String(e)
    }
  }

  async function spawnTool(toolId: string): Promise<void> {
    if (!selectedWorktreePath || !remoteApi) return
    const api = remoteApi
    const worktreePath = selectedWorktreePath
    await runAction(`Spawned ${toolId}`, async () => {
      await api.tools.spawn(toolId, worktreePath)
    })
  }

  async function closeRemoteTab(tabId: string): Promise<void> {
    if (!remoteApi) return
    const api = remoteApi
    await runAction(`Closed tab ${tabId}`, async () => {
      await api.tabs.close(tabId)
    })
  }

  async function activateTab(tabId: string): Promise<void> {
    if (!remoteApi) return
    const api = remoteApi
    await runAction(`Activated tab ${tabId}`, async () => {
      await api.tabs.activate(tabId)
    })
  }

  function enterFullscreenForTab(tab: { focusedSessionId?: string }): void {
    if (tab.focusedSessionId) fullscreenSessionId = tab.focusedSessionId
  }

  function exitFullscreen(): void {
    fullscreenSessionId = null
  }

  function openToolPicker(): void {
    toolPickerOpen = true
  }

  function closeToolPicker(): void {
    toolPickerOpen = false
  }

  async function pickAndSpawnTool(toolId: string): Promise<void> {
    closeToolPicker()
    await spawnTool(toolId)
  }

  // Drive the shell / fullscreen overlay height directly from the visual
  // viewport so the keyboard can never cover the layout — `100dvh` on iOS
  // doesn't reliably exclude the keyboard, but `visualViewport.height`
  // always does. Also expose `--kb-offset` for any descendant that wants
  // to distinguish keyboard-open from keyboard-closed state.
  let shellStyle = $derived(`--shell-height: ${visualHeight}px; --kb-offset: ${keyboardHeight}px`)
</script>

<main class="shell" style={shellStyle}>
  <!-- ============ TOPBAR (always visible) ============ -->
  <header class="topbar">
    <div class="brand">
      {#if !isDesktop && selectedProject !== null && phase.kind === 'connected'}
        <button type="button" class="iconbar-btn" onclick={backToProjects} aria-label="Back">
          ←
        </button>
      {/if}
      <h1>Canopy Remote</h1>
      <span class="beta-badge" title="This feature is in beta — expect rough edges">Beta</span>
      <span class="version">v{REMOTE_PROTOCOL_VERSION}</span>
    </div>

    <div class="status-pill" data-kind={dotKind}>
      <span class="status-dot"></span>
      <span class="status-label">{statusHeadline}</span>
      {#if mirrorState.hostInfo?.hostname && phase.kind === 'connected'}
        <span class="status-host">· {mirrorState.hostInfo.hostname}</span>
      {/if}
    </div>
  </header>

  <!-- ============ DISCONNECTED / LOADING SCREENS ============ -->
  {#if phase.kind !== 'connected'}
    <section class="status-screen">
      <div class="status-screen-card">
        <div class="big-dot" data-kind={dotKind}></div>
        <h2>{statusHeadline}</h2>
        <p>{statusDetail}</p>
      </div>
    </section>
  {:else if !mirrorState.hydrated}
    <section class="status-screen">
      <div class="status-screen-card">
        <div class="big-dot" data-kind="progress"></div>
        <h2>Loading host state…</h2>
      </div>
    </section>
  {:else}
    <!-- ============ MAIN LAYOUT ============ -->
    <div class="layout">
      <!-- ============ SIDEBAR ============ -->
      <aside class="sidebar" class:hidden={!showSidebar}>
        <section class="sidebar-section">
          <h3 class="section-title">Projects</h3>
          {#if mirrorState.projects.length === 0}
            <p class="muted">No projects open on host.</p>
          {:else}
            <div class="row-list">
              {#each mirrorState.projects as project (project.id)}
                <button
                  type="button"
                  class="row"
                  class:active={project.id === selectedProjectId}
                  onclick={() => selectProject(project.id)}
                >
                  <span class="row-main">
                    <span class="row-label">{project.name}</span>
                    <span class="row-sub">{project.path}</span>
                  </span>
                  {#if project.isGitRepo}
                    <span class="row-meta">{project.worktrees.length} wt</span>
                  {:else}
                    <span class="row-meta dim">no git</span>
                  {/if}
                </button>
              {/each}
            </div>
          {/if}
        </section>

        {#if selectedProject?.isGitRepo && selectedProject.worktrees.length > 0}
          <section class="sidebar-section">
            <h3 class="section-title">Worktrees</h3>
            <div class="row-list">
              {#each selectedProject.worktrees as wt (wt.path)}
                <button
                  type="button"
                  class="row row-compact"
                  class:active={wt.path === selectedWorktreePath}
                  onclick={() => selectWorktreeRow(wt.path)}
                >
                  <span class="row-main">
                    <span class="row-label">
                      {wt.branch}{wt.isMain ? ' (main)' : ''}
                    </span>
                  </span>
                </button>
              {/each}
            </div>
          </section>
        {/if}

        <section class="sidebar-section">
          <h3 class="section-title">Tools</h3>
          {#if mirrorState.tools.length === 0}
            <p class="muted">No tools registered.</p>
          {:else}
            <ul class="tool-grid">
              {#each mirrorState.tools as tool (tool.id)}
                <li class="tool-chip" class:unavailable={!tool.available}>{tool.name}</li>
              {/each}
            </ul>
          {/if}
        </section>

        <section class="sidebar-footer">
          <div class="diag-pill" title="Round-trip RPC latency / total calls">
            <span class="diag-label">RPC</span>
            <span class="diag-value">{lastLatencyMs === null ? '—' : `${lastLatencyMs}ms`}</span>
            <span class="diag-sep">·</span>
            <span class="diag-value">{pingCount}</span>
          </div>
          {#if pingError}
            <p class="diag-error">{pingError}</p>
          {/if}
        </section>
      </aside>

      <!-- ============ MAIN PANE ============ -->
      <section class="main-pane" class:hidden={!showMainPane}>
        {#if selectedProject !== null}
          <!-- ----- workspace view: tabs + inline preview + spawn + agent input ----- -->
          <header class="pane-header">
            <h2 class="pane-title">{workspaceLabel}</h2>
            {#if selectedWorktreePath}
              <code class="pane-subtitle">{selectedWorktreePath}</code>
            {/if}
          </header>

          <!-- Compact tab list, capped height so the inline preview gets the
               lion's share of the pane on desktop. -->
          <section class="tabs-strip">
            {#if selectedTabs.length === 0}
              <p class="muted">No tabs in this worktree. Spawn a tool below.</p>
            {:else}
              <div class="row-list">
                {#each selectedTabs as tab (tab.id)}
                  <div class="row tab-row" class:active={tab.id === selectedActiveTabId}>
                    <button
                      type="button"
                      class="tab-row-main"
                      onclick={() => activateTab(tab.id)}
                      aria-label={`Activate tab ${tab.name}`}
                    >
                      <span class="row-label">{tab.name}</span>
                      <span class="row-sub">
                        {tab.toolName}{tab.paneType ? ` · ${tab.paneType}` : ''}
                      </span>
                    </button>
                    <span class="row-actions">
                      {#if tab.id === selectedActiveTabId}
                        <span class="row-badge">active</span>
                      {/if}
                      {#if tab.focusedSessionId && !isDesktop}
                        <button
                          type="button"
                          class="icon-btn icon-btn-primary"
                          title="Fullscreen terminal"
                          onclick={() => {
                            // Activate first so the inline preview matches
                            // what the user is about to expand.
                            activateTab(tab.id)
                            enterFullscreenForTab(tab)
                          }}
                          aria-label="Fullscreen"
                        >
                          <svg
                            viewBox="0 0 16 16"
                            width="13"
                            height="13"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.6"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                          >
                            <path d="M2 6V2h4" />
                            <path d="M14 6V2h-4" />
                            <path d="M2 10v4h4" />
                            <path d="M14 10v4h-4" />
                          </svg>
                        </button>
                      {/if}
                      <button
                        type="button"
                        class="icon-btn icon-btn-danger"
                        title="Close tab"
                        onclick={() => closeRemoteTab(tab.id)}
                        aria-label="Close tab"
                      >
                        ×
                      </button>
                    </span>
                  </div>
                {/each}
              </div>
            {/if}
          </section>

          <!-- Inline live preview of the active tab's PTY session.
               Mounted only when (a) we have a sessionId and (b) the
               fullscreen overlay is NOT active — at most one mount per
               sessionId at a time so subscriber refcounts stay sane.
               `{#key sessionId}` forces a remount when the user switches
               tabs so xterm + subscription rebind to the new session. -->
          {#if selectedActiveTab?.focusedSessionId && !fullscreenSessionId && remoteApi}
            <section class="preview-frame">
              <header class="preview-header">
                <span class="preview-label">
                  Live · {selectedActiveTab.toolName}
                </span>
                <span class="preview-session">
                  <code>{selectedActiveTab.focusedSessionId}</code>
                </span>
                {#if !isDesktop}
                  <button
                    type="button"
                    class="icon-btn icon-btn-primary"
                    title="Fullscreen"
                    aria-label="Fullscreen"
                    onclick={() => enterFullscreenForTab(selectedActiveTab)}
                  >
                    <svg
                      viewBox="0 0 16 16"
                      width="13"
                      height="13"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="1.6"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                    >
                      <path d="M2 6V2h4" />
                      <path d="M14 6V2h-4" />
                      <path d="M2 10v4h4" />
                      <path d="M14 10v4h-4" />
                    </svg>
                  </button>
                {/if}
              </header>
              <div class="preview-body">
                {#key selectedActiveTab.focusedSessionId}
                  <RemoteTerminalView
                    sessionId={selectedActiveTab.focusedSessionId}
                    api={remoteApi}
                    keyboardOpen={isKeyboardOpen}
                  />
                {/key}
              </div>
            </section>
          {:else if !selectedActiveTab?.focusedSessionId && selectedTabs.length > 0}
            <section class="preview-frame preview-empty">
              <p class="muted">Select a tab with a PTY session to see its live output.</p>
            </section>
          {/if}

          <!-- Spawn tool buttons sit at the bottom of the workspace pane.
               On desktop (≥768px) we render the tools as an inline chip row
               because there's enough horizontal room. On mobile we collapse
               them behind a single "+ New tool" button that opens a bottom
               sheet — otherwise 4-5 tool chips wrap to two rows and eat
               vertical space that should belong to the terminal preview. -->
          <section class="spawn-strip">
            {#if isDesktop}
              <span class="spawn-label">Spawn:</span>
              <div class="action-row">
                {#each mirrorState.tools.filter((t) => t.available) as tool (tool.id)}
                  <button type="button" class="action-btn" onclick={() => spawnTool(tool.id)}>
                    + {tool.name}
                  </button>
                {/each}
              </div>
            {:else}
              <button
                type="button"
                class="action-btn primary new-tool-btn"
                onclick={openToolPicker}
              >
                + New tool
              </button>
            {/if}
          </section>

          {#if actionError}
            <p class="action-error inline">{actionError}</p>
          {:else if lastAction}
            <p class="action-ok inline">{lastAction}</p>
          {/if}
        {:else}
          <!-- ----- empty state (desktop, no selection) ----- -->
          <div class="empty-pane">
            <p>Select a project from the sidebar to begin.</p>
          </div>
        {/if}
      </section>
    </div>
  {/if}
</main>

<!-- ============ FULLSCREEN TERMINAL OVERLAY ============
     Used on BOTH mobile and desktop when the user expands a session to
     fullscreen — covers the whole viewport, mounts a fresh
     `RemoteTerminalView` (the inline preview is unmounted by the mutex
     above so we don't double-subscribe). -->
{#if fullscreenSessionId && remoteApi}
  <div class="terminal-overlay" style={shellStyle}>
    <header class="terminal-overlay-header">
      <button type="button" class="back-btn" onclick={exitFullscreen}> ← Close </button>
      <span class="terminal-overlay-title">
        Terminal · <code>{fullscreenSessionId}</code>
      </span>
    </header>
    {#key fullscreenSessionId}
      <RemoteTerminalView
        sessionId={fullscreenSessionId}
        api={remoteApi}
        onClose={exitFullscreen}
        keyboardOpen={isKeyboardOpen}
      />
    {/key}
  </div>
{/if}

<!-- ============ TOOL PICKER BOTTOM SHEET ============
     Mobile-only spawn-tool picker. Opened by the "+ New tool" button in
     the workspace pane's spawn-strip when the viewport is below 768px.
     Backdrop click and the X close the sheet without spawning anything. -->
{#if toolPickerOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="sheet-backdrop" onclick={closeToolPicker}>
    <div class="tool-sheet" onclick={(e) => e.stopPropagation()}>
      <header class="sheet-header">
        <span class="sheet-title">Spawn a tool</span>
        <button
          type="button"
          class="icon-btn sheet-close"
          onclick={closeToolPicker}
          aria-label="Close"
        >
          ×
        </button>
      </header>
      <div class="sheet-body">
        {#each mirrorState.tools.filter((t) => t.available) as tool (tool.id)}
          <button type="button" class="sheet-tool-row" onclick={() => pickAndSpawnTool(tool.id)}>
            <span class="sheet-tool-name">{tool.name}</span>
            <span class="sheet-tool-hint">Spawn in {workspaceLabel || 'current worktree'}</span>
          </button>
        {/each}
        {#if mirrorState.tools.filter((t) => t.available).length === 0}
          <p class="muted">No tools registered on host.</p>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  /* ============ ROOT SHELL ============ */

  .shell {
    flex: 1;
    display: flex;
    flex-direction: column;
    /* `--shell-height` is set from JS as `visualViewport.height` in CSS
       pixels, so it always equals the visible region *above* the on-screen
       keyboard. Hard-coding the pixel height (instead of `100dvh`) means
       iOS can't sneak the keyboard under our layout — we know exactly
       how much room we have. Fallbacks use `100dvh` for the first paint
       before JS runs and for environments without visualViewport. */
    height: 100vh;
    height: 100dvh;
    height: var(--shell-height, 100dvh);
    overflow: hidden;
    background: var(--c-bg);
    color: var(--c-text);
  }

  .hidden {
    display: none !important;
  }

  /* ============ TOPBAR ============ */

  .topbar {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--c-border);
    background: var(--c-bg-elevated);
  }

  /* Respect the hardware safe-area insets on phones/tablets with a notch,
     dynamic island, or rounded corners. Without these, in landscape mode
     on an iPhone the topbar content (back button, brand) slides underneath
     the notch and is partially clipped. `@supports` keeps the block inert
     on older browsers that don't understand `env()`. */
  @supports (padding: env(safe-area-inset-top)) {
    .topbar {
      padding-left: calc(16px + env(safe-area-inset-left));
      padding-right: calc(16px + env(safe-area-inset-right));
      padding-top: calc(10px + env(safe-area-inset-top));
    }
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .brand h1 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--c-text);
    white-space: nowrap;
  }

  .version {
    font-size: 10px;
    color: var(--c-text-faint);
    font-variant-numeric: tabular-nums;
  }

  .beta-badge {
    display: inline-block;
    padding: 1px 6px;
    font-size: 8px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--c-warning);
    background: color-mix(in srgb, var(--c-warning) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--c-warning) 40%, transparent);
    border-radius: 8px;
  }

  .iconbar-btn {
    all: unset;
    width: 30px;
    height: 30px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--c-accent-text);
    font-size: 18px;
    cursor: pointer;
    border-radius: 6px;
  }

  .iconbar-btn:hover {
    background: var(--c-hover);
  }

  .status-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--c-bg-input);
    border: 1px solid var(--c-border-subtle);
    font-size: 11px;
    color: var(--c-text-secondary);
    min-width: 0;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c-text-muted);
    flex-shrink: 0;
  }

  .status-pill[data-kind='progress'] .status-dot {
    background: var(--c-warning);
    box-shadow: 0 0 6px var(--c-warning);
    animation: pulse 1.4s ease-in-out infinite;
  }

  .status-pill[data-kind='ok'] .status-dot {
    background: var(--c-success);
    box-shadow: 0 0 6px var(--c-success);
  }

  .status-pill[data-kind='error'] .status-dot {
    background: var(--c-danger);
    box-shadow: 0 0 6px var(--c-danger);
  }

  .status-pill[data-kind='muted'] .status-dot {
    background: var(--c-text-muted);
  }

  .status-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-host {
    color: var(--c-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ============ STATUS / LOADING SCREEN ============ */

  .status-screen {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }

  .status-screen-card {
    background: var(--c-bg-elevated);
    border: 1px solid var(--c-border);
    border-radius: 12px;
    padding: 28px 32px;
    max-width: 420px;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
  }

  .status-screen-card h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    color: var(--c-text);
  }

  .status-screen-card p {
    margin: 0;
    font-size: 13px;
    color: var(--c-text-secondary);
    line-height: 1.55;
  }

  .big-dot {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--c-text-muted);
  }

  .big-dot[data-kind='progress'] {
    background: var(--c-warning);
    box-shadow: 0 0 14px var(--c-warning);
    animation: pulse 1.4s ease-in-out infinite;
  }

  .big-dot[data-kind='ok'] {
    background: var(--c-success);
    box-shadow: 0 0 14px var(--c-success);
  }

  .big-dot[data-kind='error'] {
    background: var(--c-danger);
    box-shadow: 0 0 14px var(--c-danger);
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }

  /* ============ LAYOUT ============ */

  .layout {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-height: 0;
  }

  /* ============ SIDEBAR ============ */

  .sidebar {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 16px;
    gap: 18px;
    min-height: 0;
  }

  .sidebar-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex-shrink: 0;
  }

  .sidebar-footer {
    margin-top: auto;
    padding-top: 12px;
    border-top: 1px solid var(--c-border-subtle);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .section-title {
    margin: 0;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--c-text-secondary);
  }

  .row-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row {
    all: unset;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    background: var(--c-bg-elevated);
    border: 1px solid var(--c-border);
    border-radius: 8px;
    gap: 10px;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .row:hover {
    background: var(--c-hover-strong);
  }

  .row.active {
    border-color: var(--c-accent-muted);
    background: var(--c-accent-bg);
  }

  .row-compact {
    padding: 7px 12px;
  }

  .row-main {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }

  .row-label {
    font-size: 13px;
    font-weight: 500;
    color: var(--c-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-sub {
    font-size: 10px;
    color: var(--c-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: ui-monospace, 'SF Mono', Menlo, monospace;
  }

  .row-meta {
    font-size: 10px;
    color: var(--c-text-muted);
    white-space: nowrap;
    flex-shrink: 0;
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .row-meta.dim {
    opacity: 0.5;
  }

  .tool-grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .tool-chip {
    font-size: 11px;
    padding: 3px 9px;
    border-radius: 999px;
    background: var(--c-bg-input);
    border: 1px solid var(--c-border-subtle);
    color: var(--c-text);
  }

  .tool-chip.unavailable {
    opacity: 0.4;
    text-decoration: line-through;
  }

  .diag-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--c-bg-input);
    border: 1px solid var(--c-border-subtle);
    font-size: 10px;
    align-self: flex-start;
  }

  .diag-label {
    color: var(--c-text-muted);
    text-transform: uppercase;
    font-weight: 600;
    letter-spacing: 0.5px;
  }

  .diag-value {
    color: var(--c-text);
    font-variant-numeric: tabular-nums;
  }

  .diag-sep {
    color: var(--c-text-muted);
  }

  .diag-error {
    margin: 0;
    font-size: 10px;
    color: var(--c-danger-text);
  }

  /* ============ MAIN PANE ============ */

  .main-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-height: 0;
    min-width: 0;
  }

  .pane-header {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 20px;
    border-bottom: 1px solid var(--c-border);
    background: var(--c-bg);
  }

  .pane-title {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pane-subtitle {
    font-size: 11px;
    color: var(--c-text-muted);
    font-family: ui-monospace, 'SF Mono', Menlo, monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex-shrink: 1;
    min-width: 0;
  }

  .back-btn {
    all: unset;
    color: var(--c-accent-text);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    padding: 4px 10px;
    border-radius: 6px;
    flex-shrink: 0;
  }

  .back-btn:hover {
    background: var(--c-hover);
  }

  /* ============ TABS STRIP (compact, capped height) ============
     Sits between the pane header and the inline preview. Scrolls
     internally if there are many tabs so the preview frame underneath
     keeps a predictable size. On mobile we cap it much lower (one tab
     row ~50px) so the preview gets the lion's share of the screen —
     otherwise two tabs push the live preview down to a single-line
     sliver that's useless for actually watching CLI output. */
  .tabs-strip {
    flex-shrink: 0;
    padding: 12px 20px 4px;
    max-height: 120px;
    overflow-y: auto;
  }

  /* ============ INLINE PREVIEW FRAME ============
     Live xterm view of the active tab's PTY session, mounted inside the
     workspace pane (NOT a separate route). Takes the remaining vertical
     space between the tabs strip and the spawn/agent input footer.
     The `min-height` guarantees a usable viewing area even on mobile
     with a cramped workspace pane — without it, a small viewport plus
     multiple tabs could squeeze the preview into a single-row sliver. */
  .preview-frame {
    flex: 1 1 auto;
    min-height: 260px;
    margin: 6px 20px 0;
    display: flex;
    flex-direction: column;
    background: var(--c-bg-elevated);
    border: 1px solid var(--c-border);
    border-radius: 8px;
    overflow: hidden;
  }

  .preview-frame.preview-empty {
    align-items: center;
    justify-content: center;
    background: var(--c-bg-input);
    border-style: dashed;
    min-height: 80px;
    flex: 0 0 auto;
    padding: 16px;
  }

  .preview-header {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--c-border);
    background: var(--c-bg);
  }

  .preview-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--c-text-secondary);
  }

  .preview-session {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .preview-session code {
    font-family: ui-monospace, 'SF Mono', Menlo, monospace;
    font-size: 10px;
    color: var(--c-text-muted);
  }

  .preview-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 6px;
  }

  /* ============ SPAWN STRIP ============
     Compact horizontal row of spawn-tool buttons sitting between the
     preview and the agent input footer. */
  .spawn-strip {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 20px 4px;
  }

  /* The spawn-strip is the last interactive row in the mobile workspace
     pane, so we need to clear the iOS home bar (safe-area-inset-bottom)
     otherwise the buttons are obscured by the system gesture indicator
     and every tap feels one row off. Only applied on mobile — desktop
     overrides this in the ≥768px media query below. */
  @supports (padding: env(safe-area-inset-bottom)) {
    .spawn-strip {
      padding-bottom: calc(4px + env(safe-area-inset-bottom));
    }
  }

  .spawn-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--c-text-secondary);
    flex-shrink: 0;
  }

  .action-error.inline,
  .action-ok.inline {
    margin: 4px 20px 0;
  }

  .empty-pane {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--c-text-muted);
    font-size: 13px;
    padding: 24px;
  }

  /* ============ TAB ROW SPECIFIC ============ */

  .tab-row {
    padding: 12px 14px;
  }

  /* The main clickable area of the tab row is a <button> so the row is
     keyboard-navigable (Enter/Space activate it) and screen-reader
     friendly. It's a flex sibling of `.row-actions` inside the tab-row
     container — NOT nested inside another button — so fullscreen/close
     buttons remain valid HTML and clickable on their own. */
  .tab-row-main {
    all: unset;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
    cursor: pointer;
    text-align: left;
  }

  .tab-row-main:focus-visible {
    outline: 2px solid var(--c-accent-muted);
    outline-offset: 2px;
    border-radius: 4px;
  }

  .row-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .row-badge {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    padding: 2px 7px;
    border-radius: 4px;
    background: var(--c-success);
    color: var(--c-bg);
  }

  .icon-btn {
    all: unset;
    width: 26px;
    height: 26px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 13px;
    border-radius: 5px;
    cursor: pointer;
    background: var(--c-bg-input);
    color: var(--c-text-secondary);
  }

  .icon-btn:hover {
    background: var(--c-hover-strong);
  }

  .icon-btn-primary {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .icon-btn-primary:hover {
    background: var(--c-accent-bg-hover);
  }

  .icon-btn-danger:hover {
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
  }

  /* ============ ACTION BUTTONS ============ */

  .action-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .action-btn {
    all: unset;
    font-family: inherit;
    font-size: 12px;
    padding: 7px 14px;
    border-radius: 6px;
    background: var(--c-bg-input);
    border: 1px solid var(--c-border-subtle);
    color: var(--c-text);
    cursor: pointer;
  }

  .action-btn:hover {
    background: var(--c-hover-strong);
  }

  .action-btn.primary {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
    border-color: var(--c-accent-muted);
  }

  .action-btn.primary:hover {
    background: var(--c-accent-bg-hover);
  }

  .action-error {
    margin: 0;
    font-size: 11px;
    color: var(--c-danger-text);
    background: var(--c-danger-bg);
    border-radius: 6px;
    padding: 6px 10px;
  }

  .action-ok {
    margin: 0;
    font-size: 11px;
    color: var(--c-success);
  }

  .pane-title code {
    font-family: ui-monospace, 'SF Mono', Menlo, monospace;
    font-size: 12px;
    color: var(--c-text-secondary);
  }

  /* ============ MOBILE "+ NEW TOOL" BUTTON + BOTTOM SHEET ============ */

  .new-tool-btn {
    /* Take the full row on mobile — the spawn-strip has no other content
       so a full-width CTA reads as the primary action. */
    flex: 1;
    text-align: center;
    font-size: 13px;
    padding: 10px 14px;
  }

  .sheet-backdrop {
    /* Dim backdrop that covers the whole viewport. z-index sits above the
       fullscreen terminal overlay (2000) so the sheet can be opened even
       when someone is inside fullscreen mode (though right now we only
       open it from the inline workspace pane). */
    position: fixed;
    inset: 0;
    z-index: 2500;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    background: rgba(0, 0, 0, 0.55);
    animation: sheet-backdrop-fade 180ms ease-out;
  }

  .tool-sheet {
    /* The sheet itself — slides up from the bottom. `max-height: 75vh`
       keeps it from eating the whole screen when there are many tools,
       while respecting the iOS home indicator via safe-area-inset-bottom
       (padded inside the body so the last row doesn't sit on the bar). */
    width: 100%;
    max-width: 520px;
    max-height: 75vh;
    max-height: 75dvh;
    background: var(--c-bg-elevated);
    border-top-left-radius: 14px;
    border-top-right-radius: 14px;
    border: 1px solid var(--c-border);
    border-bottom: none;
    box-shadow: 0 -8px 24px rgba(0, 0, 0, 0.35);
    display: flex;
    flex-direction: column;
    animation: sheet-slide-up 220ms cubic-bezier(0.25, 1, 0.5, 1);
  }

  .sheet-header {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px 10px;
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .sheet-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--c-text);
  }

  .sheet-close {
    width: 30px;
    height: 30px;
    font-size: 18px;
  }

  .sheet-body {
    flex: 1;
    overflow-y: auto;
    padding: 10px 14px 18px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  @supports (padding: env(safe-area-inset-bottom)) {
    .sheet-body {
      padding-bottom: calc(18px + env(safe-area-inset-bottom));
    }
  }

  .sheet-tool-row {
    all: unset;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 14px 14px;
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 10px;
    cursor: pointer;
  }

  .sheet-tool-row:active {
    background: var(--c-accent-bg);
    border-color: var(--c-accent-muted);
  }

  .sheet-tool-name {
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
  }

  .sheet-tool-hint {
    font-size: 11px;
    color: var(--c-text-muted);
  }

  @keyframes sheet-backdrop-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes sheet-slide-up {
    from {
      transform: translateY(100%);
    }
    to {
      transform: translateY(0);
    }
  }

  /* ============ TERMINAL OVERLAY (mobile fullscreen) ============ */

  .terminal-overlay {
    position: fixed;
    inset: 0;
    z-index: 2000;
    display: flex;
    flex-direction: column;
    background: var(--c-bg);
    padding: 12px 12px 16px;
    gap: 10px;
    /* Mirror the shell's keyboard-aware sizing so the fullscreen terminal
       view also shrinks when the soft keyboard opens. Uses the same
       `--shell-height` variable as `.shell` above — driven directly from
       `visualViewport.height` so it always excludes the keyboard even on
       iOS Safari where `100dvh` is ambiguous. */
    height: 100dvh;
    height: var(--shell-height, 100dvh);
  }

  /* Safe-area insets on all four sides: top for notch/status bar, bottom
     for the iOS home indicator, left/right for landscape rounded corners.
     The overlay is position:fixed covering the whole viewport, so without
     these insets content can slide under the home bar and the user can't
     tap the last row of the terminal. */
  @supports (padding: env(safe-area-inset-top)) {
    .terminal-overlay {
      padding: calc(12px + env(safe-area-inset-top)) calc(12px + env(safe-area-inset-right))
        calc(16px + env(safe-area-inset-bottom)) calc(12px + env(safe-area-inset-left));
    }
  }

  .terminal-overlay-header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--c-border);
  }

  .terminal-overlay-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--c-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .terminal-overlay-title code {
    font-family: ui-monospace, 'SF Mono', Menlo, monospace;
    font-size: 11px;
    color: var(--c-text-muted);
    text-transform: none;
    letter-spacing: 0;
  }

  .muted {
    margin: 0;
    font-size: 12px;
    color: var(--c-text-muted);
    line-height: 1.5;
  }

  /* ============ DESKTOP LAYOUT (≥ 768px) ============ */

  @media (min-width: 768px) {
    .layout {
      flex-direction: row;
    }

    .sidebar {
      width: 280px;
      flex: 0 0 280px;
      border-right: 1px solid var(--c-border);
      background: var(--c-bg-elevated);
    }

    /* On desktop the sidebar/main panes are always side-by-side, so the
       drill-down `hidden` class is a no-op. */
    .sidebar.hidden,
    .main-pane.hidden {
      display: flex !important;
    }

    .pane-header {
      padding: 16px 24px;
    }

    .tabs-strip {
      padding: 14px 24px 6px;
      max-height: 240px;
    }

    .preview-frame {
      margin: 8px 24px 0;
    }

    .spawn-strip {
      padding: 12px 24px 4px;
    }

    .action-error.inline,
    .action-ok.inline {
      margin: 6px 24px 0;
    }

    .topbar {
      padding: 12px 24px;
    }

    .brand h1 {
      font-size: 17px;
    }

    .status-pill {
      font-size: 12px;
      padding: 5px 12px;
    }
  }

  /* ============ WIDE DESKTOP (≥ 1280px) ============ */

  @media (min-width: 1280px) {
    .sidebar {
      width: 320px;
      flex: 0 0 320px;
    }
  }
</style>
