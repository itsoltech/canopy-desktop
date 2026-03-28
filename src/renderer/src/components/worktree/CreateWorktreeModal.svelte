<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { workspaceState, selectWorktree } from '../../lib/stores/workspace.svelte'
  import { getPref } from '../../lib/stores/preferences.svelte'

  let {
    onClose,
    repoRoot: repoRootProp,
    workspaceId: workspaceIdProp,
  }: {
    onClose: () => void
    repoRoot?: string
    workspaceId?: string
  } = $props()

  type Step = 'loading' | 'pickBase' | 'creating' | 'setup' | 'done' | 'error'

  let step = $state<Step>('loading')
  let branches = $state<{ local: string[]; remote: string[] }>({ local: [], remote: [] })
  let branchQuery = $state('')
  let selectedBase = $state('')
  let newBranchName = $state('')
  let selectedBranchIdx = $state(0)
  let errorMessage = $state('')
  let createdPath = $state('')
  let homedir = $state('')
  let refreshing = $state(false)

  // Setup progress state
  let setupLabel = $state('')
  let setupCurrent = $state(0)
  let setupTotal = $state(0)
  let setupErrors = $state<string[]>([])
  let cleanupProgressListener: (() => void) | null = null

  let repoRoot = $derived(repoRootProp ?? workspaceState.repoRoot!)
  let projectName = $derived(repoRoot.split('/').pop() || 'project')
  let workspaceId = $derived(workspaceIdProp ?? workspaceState.workspace?.id)

  // Worktree dir: <baseDir>/<projectName>/<safeBranchName>
  let worktreeDir = $derived.by(() => {
    if (!newBranchName) return ''
    const baseDir = getPref('worktrees.baseDir', '~/canopy/worktrees')
    const safeName = newBranchName.replace(/\//g, '-')
    return `${baseDir}/${projectName}/${safeName}`
  })

  let worktreeDirDisplay = $derived(
    homedir && worktreeDir.startsWith('~/') ? homedir + worktreeDir.slice(1) : worktreeDir,
  )

  onMount(async () => {
    window.api.getHomedir().then((h) => (homedir = h))
    try {
      const list = await window.api.gitBranches(repoRoot)
      branches = { local: list.local, remote: list.remote }
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
    cleanupProgressListener?.()
  })

  // Fuzzy match for branch search
  function fuzzyMatch(text: string, q: string): boolean {
    if (!q) return true
    const lower = text.toLowerCase()
    let qi = 0
    for (let i = 0; i < lower.length && qi < q.length; i++) {
      if (lower[i] === q[qi]) qi++
    }
    return qi === q.length
  }

  let allBranches = $derived([...branches.local, ...branches.remote])
  let filteredBranches = $derived(
    branchQuery ? allBranches.filter((b) => fuzzyMatch(b, branchQuery.toLowerCase())) : allBranches,
  )

  $effect(() => {
    if (selectedBranchIdx >= filteredBranches.length) {
      selectedBranchIdx = Math.max(0, filteredBranches.length - 1)
    }
  })

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

  function selectBranch(branch: string): void {
    selectedBase = branch
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

  async function runSetup(): Promise<void> {
    cleanupProgressListener = window.api.onWorktreeSetupProgress((data) => {
      setupLabel = data.label
      setupCurrent = data.actionIndex + 1
      setupTotal = data.totalActions
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
    step = 'done'
    setTimeout(
      () => {
        selectWorktree(worktreeDirDisplay)
        onClose()
      },
      setupErrors.length > 0 ? 2000 : 400,
    )
  }

  function skipSetup(): void {
    cleanupProgressListener?.()
    cleanupProgressListener = null
    finishCreation()
  }

  function handleBranchListKeydown(e: KeyboardEvent): void {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      selectedBranchIdx = (selectedBranchIdx + 1) % Math.max(1, filteredBranches.length)
      scrollIntoView()
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      selectedBranchIdx =
        (selectedBranchIdx - 1 + filteredBranches.length) % Math.max(1, filteredBranches.length)
      scrollIntoView()
    } else if (e.key === 'Enter' && filteredBranches.length > 0) {
      e.preventDefault()
      selectBranch(filteredBranches[selectedBranchIdx])
    }
  }

  function scrollIntoView(): void {
    requestAnimationFrame(() => {
      const el = document.querySelector('.branch-item.selected')
      el?.scrollIntoView({ block: 'nearest' })
    })
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onClose()
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="dialog-overlay" onkeydown={handleKeydown} onclick={onClose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="modal-container"
    role="dialog"
    aria-modal="true"
    aria-labelledby="create-worktree-title"
    onclick={(e) => e.stopPropagation()}
  >
    <h3 id="create-worktree-title" class="modal-title">Create Worktree</h3>

    {#if step === 'loading'}
      <div class="modal-body center">
        <p class="status-text">Loading branches...</p>
      </div>
    {:else if step === 'pickBase'}
      {#if !selectedBase}
        <!-- Pick base branch -->
        <div class="modal-body" onkeydown={handleBranchListKeydown}>
          <div class="field-header">
            <!-- svelte-ignore a11y_label_has_associated_control -->
            <label class="field-label">Base branch</label>
            <button
              class="btn-refresh"
              onclick={refreshBranches}
              disabled={refreshing}
              title="Fetch from remote"
            >
              <svg
                class="refresh-icon"
                class:spinning={refreshing}
                width="14"
                height="14"
                viewBox="0 0 16 16"
                fill="currentColor"
              >
                <path d="M13.65 2.35A8 8 0 1 0 16 8h-2a6 6 0 1 1-1.76-4.24L10 6h6V0l-2.35 2.35z" />
              </svg>
            </button>
          </div>
          <input
            class="field-input"
            type="text"
            bind:value={branchQuery}
            placeholder="Search branches..."
            spellcheck="false"
            autocomplete="off"
          />
          <div class="branch-list">
            {#if filteredBranches.length === 0}
              <div class="branch-empty">No branches found</div>
            {:else}
              {#each filteredBranches as branch, i (branch)}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="branch-item"
                  class:selected={i === selectedBranchIdx}
                  onclick={() => selectBranch(branch)}
                  onpointerenter={() => (selectedBranchIdx = i)}
                >
                  {branch}
                </div>
              {/each}
            {/if}
          </div>
        </div>
      {:else}
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
      {/if}
    {:else if step === 'creating'}
      <div class="modal-body center">
        <p class="status-text">Creating worktree...</p>
      </div>
    {:else if step === 'setup'}
      <div class="modal-body center">
        <p class="status-text">Running setup... ({setupCurrent}/{setupTotal})</p>
        <p class="setup-label">{setupLabel}</p>
        {#if setupErrors.length > 0}
          {#each setupErrors as err, i (i)}
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
            {#each setupErrors as err, i (i)}
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
    background: rgba(0, 0, 0, 0.5);
  }

  .modal-container {
    width: 480px;
    max-height: 500px;
    display: flex;
    flex-direction: column;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 10px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
    overflow: hidden;
  }

  .modal-title {
    margin: 0;
    padding: 16px 20px 12px;
    font-size: 15px;
    font-weight: 600;
    color: #e0e0e0;
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
    color: rgba(255, 255, 255, 0.5);
    margin: 0;
  }

  .status-text.success {
    color: rgba(100, 220, 100, 0.9);
  }

  .status-text.error {
    color: rgba(255, 120, 120, 0.9);
  }

  .status-text.warning {
    color: rgba(255, 200, 100, 0.9);
  }

  .setup-label {
    font-size: 12px;
    font-family: monospace;
    color: rgba(255, 255, 255, 0.4);
    margin: 0;
  }

  .setup-warnings {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: center;
  }

  .field-header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
  }

  .field-label {
    display: block;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.5px;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
  }

  .btn-refresh {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: rgba(255, 255, 255, 0.35);
    cursor: pointer;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .btn-refresh:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.6);
  }

  .btn-refresh:disabled {
    cursor: default;
    opacity: 0.5;
  }

  .refresh-icon {
    transition: transform 0.2s;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .refresh-icon.spinning {
    animation: spin 0.8s linear infinite;
  }

  @media (prefers-reduced-motion: reduce) {
    .refresh-icon.spinning {
      animation: none;
    }
  }

  .field-input {
    width: 100%;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.3);
    color: #e0e0e0;
    font-size: 13px;
    font-family: inherit;
    padding: 8px 10px;
    outline: none;
    transition: border-color 0.1s;
    box-sizing: border-box;
  }

  .field-input:focus {
    border-color: rgba(116, 192, 252, 0.5);
  }

  .field-input::placeholder {
    color: rgba(255, 255, 255, 0.25);
  }

  .field-info {
    margin: 0 0 12px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.6);
  }

  .field-info strong {
    color: rgba(255, 255, 255, 0.9);
  }

  .field-error {
    margin: 6px 0 0;
    font-size: 12px;
    color: rgba(255, 120, 120, 0.9);
  }

  .field-detail {
    margin: 6px 0 0;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.3);
    font-family: monospace;
    word-break: break-all;
  }

  .branch-list {
    margin-top: 8px;
    max-height: 260px;
    overflow-y: auto;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 6px;
  }

  .branch-item {
    padding: 6px 10px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    transition: background 0.05s;
  }

  .branch-item:hover,
  .branch-item.selected {
    background: rgba(255, 255, 255, 0.08);
  }

  .branch-empty {
    padding: 16px 10px;
    text-align: center;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.3);
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
    outline: 2px solid rgba(116, 192, 252, 0.6);
    outline-offset: 1px;
  }

  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .btn-cancel {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .btn-cancel:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .btn-primary {
    background: rgba(116, 192, 252, 0.2);
    color: rgba(116, 192, 252, 0.9);
  }

  .btn-primary:hover:not(:disabled) {
    background: rgba(116, 192, 252, 0.3);
  }
</style>
