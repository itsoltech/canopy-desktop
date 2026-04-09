<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  import { FitAddon } from '@xterm/addon-fit'
  import { ProgressAddon, type IProgressState } from '@xterm/addon-progress'
  import '@xterm/xterm/css/xterm.css'
  import { workspaceState, selectWorktree } from '../../lib/stores/workspace.svelte'
  import { getPref, prefs } from '../../lib/stores/preferences.svelte'
  import { openTool } from '../../lib/stores/tabs.svelte'
  import { getTheme } from '../../lib/terminal/themes'
  import { safeDirName } from '../../lib/sanitize'
  import BranchPicker from './BranchPicker.svelte'

  let {
    onClose,
    repoRoot: repoRootProp,
    workspaceId: workspaceIdProp,
    baseBranch: baseBranchProp,
  }: {
    onClose: () => void
    repoRoot?: string
    workspaceId?: string
    baseBranch?: string
  } = $props()

  type Step = 'loading' | 'pickBase' | 'creating' | 'setup' | 'done' | 'error'
  type Mode = 'new' | 'existing'

  let step = $state<Step>('loading')
  let mode = $state<Mode>('new')
  let branches = $state<{ local: string[]; remote: string[] }>({ local: [], remote: [] })
  let branchQuery = $state('')
  let selectedBase = $state('')
  let newBranchName = $state('')
  let errorMessage = $state('')
  let createdPath = $state('')
  let homedir = $state('')
  let refreshing = $state(false)
  let containerEl: HTMLDivElement | undefined = $state()

  // Setup progress state
  let setupLabel = $state('')
  let setupCurrent = $state(0)
  let setupTotal = $state(0)
  let setupErrors = $state<string[]>([])
  let cleanupProgressListener: (() => void) | null = null

  // Setup terminal
  let setupTerm: Terminal | null = null
  let progressState = $state(0)
  let progressValue = $state(0)

  let repoRoot = $derived(repoRootProp ?? workspaceState.repoRoot!)
  let projectName = $derived(repoRoot.split('/').pop() || 'project')
  let workspaceId = $derived(workspaceIdProp ?? workspaceState.workspace?.id)

  function isRemoteOnly(b: string): boolean {
    if (!branches.remote.includes(b)) return false
    const localName = b.slice(b.indexOf('/') + 1)
    return !branches.local.includes(localName)
  }

  let effectiveBranchName = $derived(
    mode === 'new'
      ? newBranchName
      : selectedBase && isRemoteOnly(selectedBase)
        ? selectedBase.slice(selectedBase.indexOf('/') + 1)
        : selectedBase,
  )

  // Worktree dir: <baseDir>/<projectName>/<safeBranchName>
  let worktreeDir = $derived.by(() => {
    if (!effectiveBranchName) return ''
    const baseDir = getPref('worktrees.baseDir', '~/canopy/worktrees')
    const safeName = safeDirName(effectiveBranchName)
    return `${baseDir}/${projectName}/${safeName}`
  })

  let worktreeDirDisplay = $derived(
    homedir && worktreeDir.startsWith('~/') ? homedir + worktreeDir.slice(1) : worktreeDir,
  )

  onMount(async () => {
    containerEl?.focus()
    window.api.getHomedir().then((h) => (homedir = h))
    try {
      const list = await window.api.gitBranches(repoRoot)
      branches = { local: list.local, remote: list.remote }
      if (baseBranchProp) {
        selectedBase = baseBranchProp
      }
      step = 'pickBase'
    } catch (e) {
      errorMessage = e instanceof Error ? e.message : String(e)
      step = 'error'
    }
  })

  async function refreshBranches(): Promise<void> {
    refreshing = true
    try {
      await window.api.gitFetchAll(repoRoot)
      const list = await window.api.gitBranches(repoRoot)
      branches = { local: list.local, remote: list.remote }
    } catch {
      // fetch failed — keep existing branch list
    }
    refreshing = false
  }

  onDestroy(() => {
    window.api.abortWorktreeSetup()
    cleanupProgressListener?.()
    disposeSetupTerminal()
  })

  function initSetupTerminal(container: HTMLDivElement): void {
    if (setupTerm) return
    const currentTheme = getTheme(prefs.theme || 'Default')
    const term = new Terminal({
      fontSize: 11,
      fontFamily: 'JetBrains Mono, JetBrainsMono Nerd Font, Fira Code, Menlo, monospace',
      theme: currentTheme,
      scrollback: 1000,
      disableStdin: true,
      cursorBlink: false,
      cursorInactiveStyle: 'none',
    })
    const fitAddon = new FitAddon()
    const progressAddon = new ProgressAddon()
    term.open(container)
    term.loadAddon(fitAddon)
    term.loadAddon(progressAddon)
    progressAddon.onChange(({ state, value }: IProgressState) => {
      progressState = state
      progressValue = value
    })
    requestAnimationFrame(() => fitAddon.fit())
    setupTerm = term
  }

  function disposeSetupTerminal(): void {
    if (setupTerm) {
      setupTerm.dispose()
      setupTerm = null
      progressState = 0
      progressValue = 0
    }
  }

  // Validate branch name
  let branchNameError = $derived.by(() => {
    if (!newBranchName) return null
    if (/\s/.test(newBranchName)) return 'No spaces allowed'
    if (/\.\./.test(newBranchName)) return 'Cannot contain ..'
    if (/[~^:\\]/.test(newBranchName)) return 'Invalid characters'
    if (newBranchName.startsWith('-')) return 'Cannot start with -'
    if (branches.local.includes(newBranchName)) return 'Branch already exists'
    return null
  })

  function hasSetupConfig(): boolean {
    if (!workspaceId) return false
    const raw = getPref(`workspace:${workspaceId}:worktreeSetup`, '')
    if (!raw) return false
    try {
      const actions = JSON.parse(raw) as unknown[]
      return Array.isArray(actions) && actions.length > 0
    } catch {
      return false
    }
  }

  function setMode(next: Mode): void {
    if (mode === next) return
    mode = next
    selectedBase = ''
    newBranchName = ''
    // branchQuery preserved — search stays useful across modes
  }

  async function createWorktree(): Promise<void> {
    if (!newBranchName || branchNameError || !selectedBase) return
    step = 'creating'
    try {
      await window.api.gitWorktreeAdd(repoRoot, worktreeDir, newBranchName, selectedBase)
      createdPath = worktreeDirDisplay

      if (hasSetupConfig() && workspaceId) {
        step = 'setup'
        await runSetup()
      } else {
        finishCreation()
      }
    } catch (err) {
      errorMessage = err instanceof Error ? err.message : String(err)
      step = 'error'
    }
  }

  async function createWorktreeFromExisting(): Promise<void> {
    if (!selectedBase) return
    step = 'creating'
    try {
      const createLocalTracking = isRemoteOnly(selectedBase)
      await window.api.gitWorktreeCheckout(repoRoot, worktreeDir, selectedBase, createLocalTracking)
      createdPath = worktreeDirDisplay

      if (hasSetupConfig() && workspaceId) {
        step = 'setup'
        await runSetup()
      } else {
        finishCreation()
      }
    } catch (err) {
      errorMessage = err instanceof Error ? err.message : String(err)
      step = 'error'
    }
  }

  async function runSetup(): Promise<void> {
    cleanupProgressListener = window.api.onWorktreeSetupProgress((data) => {
      setupLabel = data.label
      setupCurrent = data.actionIndex + 1
      setupTotal = data.totalActions
      if (data.outputChunk && setupTerm) {
        setupTerm.write(data.outputChunk)
      }
      if (data.status === 'error' && data.error) {
        setupErrors = [...setupErrors, `${data.label}: ${data.error}`]
      }
    })

    try {
      await window.api.runWorktreeSetup(workspaceId!, repoRoot, worktreeDirDisplay)
    } catch (err) {
      setupErrors = [...setupErrors, err instanceof Error ? err.message : String(err)]
    }

    cleanupProgressListener?.()
    cleanupProgressListener = null
    finishCreation()
  }

  function finishCreation(): void {
    if (step === 'done') return
    step = 'done'
    setTimeout(
      async () => {
        await openTool(getPref('newWorktree.toolId', 'shell'), worktreeDirDisplay).catch((err) => {
          console.error('Failed to launch tool after worktree creation:', err)
        })
        selectWorktree(worktreeDirDisplay)
        onClose()
      },
      setupErrors.length > 0 ? 2000 : 400,
    )
  }

  function skipSetup(): void {
    window.api.abortWorktreeSetup()
    cleanupProgressListener?.()
    cleanupProgressListener = null
    disposeSetupTerminal()
    finishCreation()
  }

  function setupTerminalAction(node: HTMLDivElement): { destroy: () => void } {
    initSetupTerminal(node)
    return { destroy: disposeSetupTerminal }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      if (step === 'setup') {
        skipSetup()
      } else {
        onClose()
      }
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="dialog-overlay"
  onkeydown={handleKeydown}
  onmousedown={() => (step === 'setup' ? skipSetup() : onClose())}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="modal-container"
    role="dialog"
    aria-modal="true"
    aria-labelledby="create-worktree-title"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <h3 id="create-worktree-title" class="modal-title">Create Worktree</h3>

    {#if step === 'loading'}
      <div class="modal-body center">
        <p class="status-text">Loading branches...</p>
      </div>
    {:else if step === 'pickBase'}
      {#if mode === 'new' && selectedBase}
        <!-- Name new branch -->
        <div class="modal-body">
          <p class="field-info">Base: <strong>{selectedBase}</strong></p>
          <!-- svelte-ignore a11y_label_has_associated_control -->
          <label class="field-label">New branch name</label>
          <input
            class="field-input"
            type="text"
            bind:value={newBranchName}
            placeholder="feature/my-branch"
            spellcheck="false"
            autocomplete="off"
            onkeydown={(e) => {
              if (e.key === 'Enter' && newBranchName && !branchNameError) {
                e.preventDefault()
                createWorktree()
              }
            }}
          />
          {#if branchNameError}
            <p class="field-error">{branchNameError}</p>
          {/if}
          {#if worktreeDir}
            <p class="field-detail">Path: {worktreeDirDisplay}</p>
          {/if}
          <div class="modal-actions">
            <button class="btn btn-cancel" onclick={() => (selectedBase = '')}>Back</button>
            <button
              class="btn btn-primary"
              onclick={createWorktree}
              disabled={!newBranchName || !!branchNameError}
            >
              Create
            </button>
          </div>
        </div>
      {:else}
        <!-- Branch picker (shared between new-mode base selection and existing-mode checkout) -->
        <div class="modal-body">
          <div class="mode-toggle" role="tablist">
            <button
              class="mode-btn"
              class:active={mode === 'new'}
              onclick={() => setMode('new')}
              role="tab"
              aria-selected={mode === 'new'}
              type="button"
            >
              New branch
            </button>
            <button
              class="mode-btn"
              class:active={mode === 'existing'}
              onclick={() => setMode('existing')}
              role="tab"
              aria-selected={mode === 'existing'}
              type="button"
            >
              From existing branch
            </button>
          </div>
          <BranchPicker
            {branches}
            bind:query={branchQuery}
            bind:selectedBranch={selectedBase}
            {refreshing}
            onRefresh={refreshBranches}
            label={mode === 'new' ? 'Base branch' : 'Branch to check out'}
            showRemoteOnlyTag={mode === 'existing'}
            highlightPicked={mode === 'existing'}
            onCommit={mode === 'existing' ? createWorktreeFromExisting : undefined}
          />
          {#if mode === 'existing'}
            {#if selectedBase && worktreeDir}
              <p class="field-detail">Path: {worktreeDirDisplay}</p>
            {/if}
            <div class="modal-actions">
              <button class="btn btn-cancel" onclick={onClose}>Cancel</button>
              <button
                class="btn btn-primary"
                onclick={createWorktreeFromExisting}
                disabled={!selectedBase}
              >
                Create
              </button>
            </div>
          {/if}
        </div>
      {/if}
    {:else if step === 'creating'}
      <div class="modal-body center">
        <p class="status-text">Creating worktree...</p>
      </div>
    {:else if step === 'setup'}
      <div class="modal-body setup-body">
        <p class="status-text">Running setup... ({setupCurrent}/{setupTotal})</p>
        <p class="setup-label">{setupLabel}</p>
        <div class="setup-terminal-wrapper">
          {#if progressState > 0}
            <div
              class="progress-bar"
              class:progress-error={progressState === 2}
              class:progress-indeterminate={progressState === 3}
              class:progress-warning={progressState === 4}
              style:width={progressState === 3 ? '100%' : `${progressValue}%`}
            ></div>
          {/if}
          <div class="setup-terminal" use:setupTerminalAction></div>
        </div>
        {#if setupErrors.length > 0}
          {#each setupErrors as err (err)}
            <p class="field-error">{err}</p>
          {/each}
        {/if}
        <div class="modal-actions">
          <button class="btn btn-cancel" onclick={skipSetup}>Skip</button>
        </div>
      </div>
    {:else if step === 'done'}
      <div class="modal-body center">
        <p class="status-text success">Worktree created</p>
        <p class="field-detail">{createdPath}</p>
        {#if setupErrors.length > 0}
          <div class="setup-warnings">
            <p class="status-text warning">Setup completed with warnings:</p>
            {#each setupErrors as err (err)}
              <p class="field-error">{err}</p>
            {/each}
          </div>
        {/if}
      </div>
    {:else if step === 'error'}
      <div class="modal-body center">
        <p class="status-text error">Error</p>
        <p class="field-detail">{errorMessage}</p>
        <div class="modal-actions">
          <button class="btn btn-cancel" onclick={onClose}>Close</button>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .dialog-overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 80px;
    background: var(--c-scrim);
  }

  .modal-container {
    outline: none;
    width: 480px;
    max-height: 560px;
    display: flex;
    flex-direction: column;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 10px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
    overflow: hidden;
  }

  .modal-title {
    margin: 0;
    padding: 16px 20px 12px;
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    flex-shrink: 0;
  }

  .modal-body {
    padding: 0 20px 20px;
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }

  .modal-body.center {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 30px 20px;
    gap: 8px;
  }

  .status-text {
    font-size: 13px;
    color: var(--c-text-secondary);
    margin: 0;
  }

  .status-text.success {
    color: var(--c-success);
  }

  .status-text.error {
    color: var(--c-danger-text);
  }

  .status-text.warning {
    color: var(--c-warning-text);
  }

  .setup-body {
    display: flex;
    flex-direction: column;
    padding: 0 20px 20px;
    gap: 8px;
  }

  .setup-label {
    font-size: 12px;
    font-family: monospace;
    color: var(--c-text-muted);
    margin: 0;
  }

  .setup-terminal-wrapper {
    position: relative;
    border-radius: 6px;
    overflow: hidden;
    border: 1px solid var(--c-border-subtle);
  }

  .setup-terminal {
    height: 220px;
    padding: 8px;
    box-sizing: border-box;
  }

  .progress-bar {
    position: absolute;
    top: 0;
    left: 0;
    height: 2px;
    background: var(--c-accent);
    transition: width 0.3s ease;
    z-index: 5;
  }

  .progress-error {
    background: var(--c-danger);
  }

  .progress-warning {
    background: var(--c-warning);
  }

  .progress-indeterminate {
    animation: indeterminate 1.5s ease-in-out infinite;
    background: linear-gradient(90deg, transparent, var(--c-accent), transparent);
  }

  @keyframes indeterminate {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(100%);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .progress-indeterminate {
      animation: none;
      background: var(--c-accent);
    }
  }

  .setup-warnings {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: center;
  }

  .field-label {
    display: block;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.5px;
    color: var(--c-text-muted);
    text-transform: uppercase;
  }

  .field-input {
    width: 100%;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-input);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    padding: 8px 10px;
    outline: none;
    transition: border-color 0.1s;
    box-sizing: border-box;
  }

  .field-input:focus {
    border-color: var(--c-focus-ring);
  }

  .field-input::placeholder {
    color: var(--c-text-faint);
  }

  .field-info {
    margin: 0 0 12px;
    font-size: 13px;
    color: var(--c-text-secondary);
  }

  .field-info strong {
    color: var(--c-text);
  }

  .field-error {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--c-danger-text);
  }

  .field-detail {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--c-text-faint);
    font-family: monospace;
    word-break: break-all;
  }

  .mode-toggle {
    display: flex;
    gap: 2px;
    padding: 2px;
    margin: 0 0 12px;
    background: var(--c-active);
    border-radius: 6px;
  }

  .mode-btn {
    flex: 1;
    padding: 5px 8px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--c-text-muted);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .mode-btn:hover:not(.active) {
    color: var(--c-text-secondary);
  }

  .mode-btn.active {
    background: var(--c-bg-overlay);
    color: var(--c-text);
    box-shadow: 0 1px 2px var(--c-shadow, rgba(0, 0, 0, 0.15));
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }

  .btn {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    border: none;
    outline: none;
    transition: background 0.1s;
  }

  .btn:focus-visible {
    outline: 2px solid var(--c-focus-ring);
    outline-offset: 1px;
  }

  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .btn-cancel {
    background: var(--c-active);
    color: var(--c-text);
  }

  .btn-cancel:hover {
    background: var(--c-border);
  }

  .btn-primary {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--c-accent-muted);
  }
</style>
