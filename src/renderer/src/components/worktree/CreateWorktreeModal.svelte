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
  import { isRemoteOnly } from './utils'

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

  let setupLabel = $state('')
  let setupCurrent = $state(0)
  let setupTotal = $state(0)
  let setupErrors = $state<string[]>([])
  let cleanupProgressListener: (() => void) | null = null

  let setupTerm: Terminal | null = null
  let progressState = $state(0)
  let progressValue = $state(0)
  let finishTimer: ReturnType<typeof setTimeout> | null = null

  let repoRoot = $derived(repoRootProp ?? workspaceState.repoRoot!)
  let projectName = $derived(repoRoot.split('/').pop() || 'project')
  let workspaceId = $derived(workspaceIdProp ?? workspaceState.workspace?.id)

  let effectiveBranchName = $derived(
    mode === 'new'
      ? newBranchName
      : selectedBase && isRemoteOnly(selectedBase, branches)
        ? selectedBase.slice(selectedBase.indexOf('/') + 1)
        : selectedBase,
  )

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
    if (finishTimer) clearTimeout(finishTimer)
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
      const createLocalTracking = isRemoteOnly(selectedBase, branches)
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
    if (finishTimer) clearTimeout(finishTimer)
    finishTimer = setTimeout(
      async () => {
        finishTimer = null
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

  const inputCls =
    'w-full border border-border rounded-lg bg-bg-input text-text text-md font-inherit px-2.5 py-2 outline-none transition-colors duration-fast box-border focus:border-focus-ring placeholder:text-text-faint'
  const btnCancelCls =
    'px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-active text-text transition-colors duration-fast hover:bg-border focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1 disabled:opacity-40 disabled:cursor-default'
  const btnPrimaryCls =
    'px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-accent-bg text-accent-text transition-colors duration-fast enabled:hover:bg-accent-muted focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1 disabled:opacity-40 disabled:cursor-default'
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-[1001] flex justify-center items-start pt-20 bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={() => (step === 'setup' ? skipSetup() : onClose())}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="outline-none w-[480px] max-h-[560px] flex flex-col bg-bg-overlay border border-border rounded-[10px] shadow-[0_16px_48px_oklch(0_0_0/0.6)] overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-labelledby="create-worktree-title"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <h3
      id="create-worktree-title"
      class="m-0 px-5 pt-4 pb-3 text-[15px] font-semibold text-text flex-shrink-0"
    >
      Create Worktree
    </h3>

    {#if step === 'loading'}
      <div
        class="px-5 pb-5 flex-1 overflow-y-auto min-h-0 flex flex-col items-center justify-center py-8 gap-2"
      >
        <p class="text-md text-text-secondary m-0">Loading branches...</p>
      </div>
    {:else if step === 'pickBase'}
      {#if mode === 'new' && selectedBase}
        <div class="px-5 pb-5 flex-1 overflow-y-auto min-h-0">
          <p class="m-0 mb-3 text-md text-text-secondary">
            Base: <strong class="text-text">{selectedBase}</strong>
          </p>
          <!-- svelte-ignore a11y_label_has_associated_control -->
          <label class="block text-xs font-semibold tracking-[0.5px] text-text-muted uppercase">
            New branch name
          </label>
          <input
            class={inputCls}
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
            <p class="mt-1.5 mb-0 text-sm text-danger-text">{branchNameError}</p>
          {/if}
          {#if worktreeDir}
            <p class="mt-1.5 mb-0 text-xs text-text-faint font-mono break-all">
              Path: {worktreeDirDisplay}
            </p>
          {/if}
          <div class="flex justify-end gap-2 mt-4">
            <button class={btnCancelCls} onclick={() => (selectedBase = '')}>Back</button>
            <button
              class={btnPrimaryCls}
              onclick={createWorktree}
              disabled={!newBranchName || !!branchNameError}
            >
              Create
            </button>
          </div>
        </div>
      {:else}
        <div class="px-5 pb-5 flex-1 overflow-y-auto min-h-0">
          <div
            class="flex gap-0.5 p-0.5 mb-3 bg-active rounded-lg"
            role="radiogroup"
            aria-label="Branch mode"
          >
            <button
              class="flex-1 px-2 py-[5px] border-0 rounded-md text-sm font-inherit cursor-pointer transition-all duration-fast {mode ===
              'new'
                ? '!bg-bg-overlay !text-text shadow-[0_1px_2px_oklch(0_0_0/0.15)]'
                : 'bg-transparent text-text-muted hover:text-text-secondary'}"
              onclick={() => setMode('new')}
              role="radio"
              aria-checked={mode === 'new'}
              type="button"
            >
              New branch
            </button>
            <button
              class="flex-1 px-2 py-[5px] border-0 rounded-md text-sm font-inherit cursor-pointer transition-all duration-fast {mode ===
              'existing'
                ? '!bg-bg-overlay !text-text shadow-[0_1px_2px_oklch(0_0_0/0.15)]'
                : 'bg-transparent text-text-muted hover:text-text-secondary'}"
              onclick={() => setMode('existing')}
              role="radio"
              aria-checked={mode === 'existing'}
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
              <p class="mt-1.5 mb-0 text-xs text-text-faint font-mono break-all">
                Path: {worktreeDirDisplay}
              </p>
            {/if}
            <div class="flex justify-end gap-2 mt-4">
              <button class={btnCancelCls} onclick={onClose}>Cancel</button>
              <button
                class={btnPrimaryCls}
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
      <div
        class="px-5 pb-5 flex-1 overflow-y-auto min-h-0 flex flex-col items-center justify-center py-8 gap-2"
      >
        <p class="text-md text-text-secondary m-0">Creating worktree...</p>
      </div>
    {:else if step === 'setup'}
      <div class="flex flex-col px-5 pb-5 gap-2 flex-1 overflow-y-auto min-h-0">
        <p class="text-md text-text-secondary m-0">
          Running setup... ({setupCurrent}/{setupTotal})
        </p>
        <p class="text-sm font-mono text-text-muted m-0">{setupLabel}</p>
        <div class="relative rounded-lg overflow-hidden border border-border-subtle">
          {#if progressState > 0}
            <div
              class="absolute top-0 left-0 h-0.5 z-[5] transition-[width] duration-slow ease-out"
              class:bg-accent={progressState !== 2 && progressState !== 4 && progressState !== 3}
              class:bg-danger={progressState === 2}
              class:bg-warning={progressState === 4}
              class:progress-indeterminate={progressState === 3}
              style:width={progressState === 3 ? '100%' : `${progressValue}%`}
            ></div>
          {/if}
          <div class="h-[220px] p-2 box-border" use:setupTerminalAction></div>
        </div>
        {#if setupErrors.length > 0}
          {#each setupErrors as err (err)}
            <p class="mt-1.5 mb-0 text-sm text-danger-text">{err}</p>
          {/each}
        {/if}
        <div class="flex justify-end gap-2 mt-4">
          <button class={btnCancelCls} onclick={skipSetup}>Skip</button>
        </div>
      </div>
    {:else if step === 'done'}
      <div
        class="px-5 pb-5 flex-1 overflow-y-auto min-h-0 flex flex-col items-center justify-center py-8 gap-2"
      >
        <p class="text-md text-success m-0">Worktree created</p>
        <p class="text-xs text-text-faint font-mono break-all m-0">{createdPath}</p>
        {#if setupErrors.length > 0}
          <div class="mt-2 flex flex-col gap-1 items-center">
            <p class="text-md text-warning-text m-0">Setup completed with warnings:</p>
            {#each setupErrors as err (err)}
              <p class="text-sm text-danger-text m-0">{err}</p>
            {/each}
          </div>
        {/if}
      </div>
    {:else if step === 'error'}
      <div
        class="px-5 pb-5 flex-1 overflow-y-auto min-h-0 flex flex-col items-center justify-center py-8 gap-2"
      >
        <p class="text-md text-danger-text m-0">Error</p>
        <p class="text-xs text-text-faint font-mono break-all m-0">{errorMessage}</p>
        <div class="flex justify-end gap-2 mt-4">
          <button class={btnCancelCls} onclick={onClose}>Close</button>
        </div>
      </div>
    {/if}
  </div>
</div>
