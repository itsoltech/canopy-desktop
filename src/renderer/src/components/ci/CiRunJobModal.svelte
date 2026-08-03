<script lang="ts">
  import { onMount } from 'svelte'
  import { LoaderCircle, Play, X } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { triggerCiBuild } from '../../lib/stores/ci.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import BranchPicker from '../worktree/BranchPicker.svelte'
  import RunBuildDialog from './RunBuildDialog.svelte'
  import type { CiParameter } from '../../lib/ci/types'

  // Run-job dialog: pick a job and a branch (searchable — TeamCity branch lists get
  // long), then flow into the parameters form. Rendered from MainLayout so it is
  // centered on the SCREEN — inside the sidebar, its backdrop-filter would become
  // the containing block for position:fixed and pin the dialog to the sidebar column.

  let {
    repoRoot,
    initialBranch,
  }: {
    repoRoot: string
    initialBranch?: string
  } = $props()

  let config = $state<{ baseUrl: string; buildTypes: Array<{ id: string; label: string }> } | null>(
    null,
  )
  let buildTypeId = $state('')
  // BranchPicker combobox semantics (same component as the worktree creation flow):
  // picking writes the branch into the input and collapses the list; hand-editing the
  // query clears the selection, so Run can never fire on a branch the user didn't pick.
  let selectedBranch = $state(initialBranch ?? '')
  let branchQuery = $state(initialBranch ?? '')
  let branches = $state<string[]>([])
  let branchesLoading = $state(false)
  let error = $state('')
  let starting = $state(false)
  let params = $state<CiParameter[] | null>(null)
  let submitting = $state(false)
  let paramsError = $state('')
  let dialogEl = $state<HTMLElement>()
  let branchesSeq = 0

  let label = $derived(config?.buildTypes.find((bt) => bt.id === buildTypeId)?.label ?? buildTypeId)

  onMount(async () => {
    try {
      const res = await window.api.ciConfig(repoRoot)
      config = res.config
      // `invalid` distinguishes "the block cannot be used" from "no ci block" —
      // this dialog only opens from entries that exist BECAUSE the repo is
      // configured, so "not configured" here would send the user hunting for a
      // setting they already have.
      if (!config) {
        error = res.invalid?.message ?? 'No CI configured for this repository'
        return
      }
    } catch (e) {
      config = null
      error = e instanceof Error ? e.message : "Could not read this repository's CI configuration"
      return
    }
    buildTypeId = config.buildTypes[0]?.id ?? ''
    void loadBranches()
  })

  $effect(() => {
    if (!params) dialogEl?.focus()
  })

  async function loadBranches(): Promise<void> {
    if (!buildTypeId) return
    const seq = ++branchesSeq
    branchesLoading = true
    error = ''
    try {
      const list = await window.api.ciBranches(repoRoot, buildTypeId)
      if (seq !== branchesSeq) return
      branches = list
      // Deliberately NO auto-select of list[0]: this is a trigger dialog, and arming
      // Run on TeamCity's first-listed (default) branch would let a single click
      // queue a job on a branch the user never chose. A branch prefilled by the
      // opening flow (worktree/section) stays selected even when TC doesn't list it.
    } catch (e) {
      if (seq !== branchesSeq) return
      branches = []
      error = e instanceof Error ? e.message : 'Failed to load branches'
    } finally {
      if (seq === branchesSeq) branchesLoading = false
    }
  }

  function selectJob(id: string): void {
    buildTypeId = id
    selectedBranch = initialBranch ?? ''
    branchQuery = initialBranch ?? ''
    void loadBranches()
  }

  /** Fetch the job's prompt parameters — none means trigger right away. */
  async function startRun(): Promise<void> {
    if (!buildTypeId || !selectedBranch) return
    starting = true
    error = ''
    try {
      const fetched = await window.api.ciBuildParameters(repoRoot, buildTypeId)
      if (fetched.length === 0) {
        // triggerCiBuild reports failure via its return value, not by throwing —
        // without surfacing it here the dialog would look untouched after Run.
        const failure = await triggerCiBuild(repoRoot, buildTypeId, selectedBranch, label)
        if (failure) error = failure
        else closeDialog()
      } else {
        params = fetched
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load build parameters'
    } finally {
      starting = false
    }
  }

  async function runWithParameters(
    properties: Array<{ name: string; value: string }>,
  ): Promise<void> {
    submitting = true
    paramsError = ''
    try {
      const failure = await triggerCiBuild(repoRoot, buildTypeId, selectedBranch, label, properties)
      if (failure) paramsError = failure
      else closeDialog()
    } finally {
      submitting = false
    }
  }

  /** Same rule as ProjectCiModal.requestClose: a trigger failure has NO surface
      outside this dialog (triggerCiBuild deliberately returns the message rather
      than toasting, because the scrim paints over z-banner), so dismissing
      mid-POST would discard it — and the typed parameter values with it. */
  function requestClose(): void {
    if (!starting && !submitting) closeDialog()
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      requestClose()
      return
    }
    if (e.key === 'Tab' && dialogEl) cycleFocus(dialogEl, e)
  }
</script>

{#if params}
  <RunBuildDialog
    {label}
    branch={selectedBranch}
    parameters={params}
    running={submitting}
    error={paramsError}
    onCancel={() => {
      params = null
      // The error belongs to the run that failed — it must not resurface when the
      // parameters dialog is opened again for a fresh attempt.
      paramsError = ''
    }}
    onRun={runWithParameters}
  />
{:else}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="fixed inset-0 z-overlay flex justify-center items-center bg-scrim"
    onmousedown={requestClose}
    onkeydown={handleKeydown}
  >
    <div
      bind:this={dialogEl}
      class="outline-none w-[460px] max-w-[92vw] max-h-[80vh] flex flex-col gap-3 bg-bg-overlay border border-border rounded-xl shadow-modal p-5"
      role="dialog"
      aria-modal="true"
      aria-label="Run job"
      tabindex="-1"
      onmousedown={(e) => e.stopPropagation()}
    >
      <header class="flex items-start justify-between gap-3">
        <h3 class="text-base font-semibold text-text m-0 leading-tight">Run job</h3>
        <button
          type="button"
          class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0 aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-muted"
          onclick={requestClose}
          aria-disabled={starting || submitting}
          aria-label="Close"
          title={starting || submitting ? 'Disabled while the run request is in flight' : 'Close'}
        >
          <X size={16} />
        </button>
      </header>

      {#if config}
        <div class="flex flex-col gap-1">
          <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
            >Job</span
          >
          <CustomSelect
            value={buildTypeId}
            options={config.buildTypes.map((bt) => ({ value: bt.id, label: bt.label }))}
            onchange={selectJob}
          />
        </div>

        <div class="flex flex-col min-h-0 flex-1">
          {#if branchesLoading && branches.length === 0}
            <span class="flex items-center gap-2 px-1 py-1.5 text-sm text-text-faint">
              <LoaderCircle size={13} class="animate-spin-slow motion-reduce:animate-none" />
              Loading branches…
            </span>
          {:else}
            <BranchPicker
              branches={{ local: branches, remote: [] }}
              label="Branch"
              bind:query={branchQuery}
              bind:selectedBranch
              refreshing={branchesLoading}
              onRefresh={loadBranches}
              fillQueryOnPick={true}
              highlightPicked={true}
            />
          {/if}
        </div>

        <div class="min-h-4.5" aria-live="polite">
          {#if error}
            <span class="text-xs text-danger-text">{error}</span>
          {/if}
        </div>

        <div class="flex gap-1.5 justify-end">
          <button
            type="button"
            class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-secondary"
            onclick={requestClose}
            aria-disabled={starting}
            title={starting
              ? 'Disabled while the run request is in flight'
              : 'Close without running'}>Cancel</button
          >
          <button
            type="button"
            class="flex items-center gap-1.5 px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
            onclick={startRun}
            disabled={starting || branchesLoading || !selectedBranch || !buildTypeId}
            title="Fetches the job's parameters — configurations without prompts run immediately"
          >
            {#if starting}
              <LoaderCircle size={13} class="animate-spin-slow motion-reduce:animate-none" />
            {:else}
              <Play size={13} />
            {/if}
            Run
          </button>
        </div>
      {:else}
        <!-- Persistent region: the Loading… → error swap must arrive as a MUTATION
             or it is never announced. -->
        <div class="min-h-4.5" aria-live="polite">
          <span class="text-sm {error ? 'text-danger-text' : 'text-text-faint'}"
            >{error || 'Loading…'}</span
          >
        </div>
        {#if error}
          <div class="flex gap-1.5 justify-end">
            <button
              type="button"
              class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
              onclick={closeDialog}>Close</button
            >
          </div>
        {/if}
      {/if}
    </div>
  </div>
{/if}
