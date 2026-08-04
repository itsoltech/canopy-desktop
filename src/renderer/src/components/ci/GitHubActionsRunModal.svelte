<script lang="ts">
  import { onMount } from 'svelte'
  import { LoaderCircle, Play, X } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import { triggerCiJob } from '../../lib/stores/ci.svelte'
  import type { CiParameter, CiRef, GitHubActionsCiRepoConfigInfo } from '../../lib/ci/types'
  import {
    initialFormValues,
    isCheckboxChecked,
    missingRequired,
    toInputs,
    toggleCheckbox,
  } from '../../lib/ci/runBuildForm'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'

  let { repoRoot, initialBranch }: { repoRoot: string; initialBranch?: string } = $props()
  let containerEl: HTMLElement | undefined = $state()
  let config = $state<GitHubActionsCiRepoConfigInfo | null>(null)
  let jobId = $state('')
  let refs = $state<CiRef[]>([])
  let selectedRefKey = $state('')
  let parameters = $state<CiParameter[] | null>(null)
  let schemaRevision = $state('')
  let values = $state<Record<string, string>>({})
  let loading = $state(true)
  let running = $state(false)
  let error = $state('')
  let loadSequence = 0

  let selectedRef = $derived(refs.find((ref) => `${ref.kind}:${ref.name}` === selectedRefKey))
  let label = $derived(
    config?.workflows.find((workflow) => workflow.path === jobId)?.label ?? jobId,
  )
  let missing = $derived(parameters ? missingRequired(parameters, values) : [])
  let canRun = $derived(
    !!jobId && !!selectedRef && parameters !== null && missing.length === 0 && !loading && !running,
  )

  onMount(async () => {
    containerEl?.focus()
    try {
      const result = await window.api.ciConfig(repoRoot)
      if (result.config?.provider !== 'github-actions') {
        throw new Error('GitHub Actions is not configured for this repository')
      }
      config = result.config
      jobId = result.config.workflows[0]?.path ?? ''
      await loadRefs()
    } catch (cause) {
      error = cause instanceof Error ? cause.message : 'Could not load GitHub Actions'
    } finally {
      loading = false
    }
  })

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault()
      if (!running) closeDialog()
    } else if (event.key === 'Tab' && containerEl) {
      cycleFocus(containerEl, event)
    }
  }

  async function selectJob(path: string): Promise<void> {
    jobId = path
    selectedRefKey = ''
    refs = []
    parameters = null
    schemaRevision = ''
    values = {}
    await loadRefs()
  }

  async function loadRefs(): Promise<void> {
    if (!jobId) return
    const sequence = ++loadSequence
    loading = true
    error = ''
    try {
      const loadedRefs = await window.api.ciJobRefs(repoRoot, jobId)
      if (sequence !== loadSequence) return
      refs = loadedRefs
      const worktreeRef = initialBranch
        ? loadedRefs.find((ref) => ref.kind === 'branch' && ref.name === initialBranch)
        : undefined
      selectedRefKey = worktreeRef ? `branch:${worktreeRef.name}` : ''
      if (worktreeRef) await loadParameters()
    } catch (cause) {
      if (sequence !== loadSequence) return
      error = cause instanceof Error ? cause.message : 'Could not load GitHub refs'
    } finally {
      if (sequence === loadSequence) loading = false
    }
  }

  async function selectRef(key: string): Promise<void> {
    selectedRefKey = key
    parameters = null
    schemaRevision = ''
    values = {}
    if (key) await loadParameters()
  }

  async function loadParameters(): Promise<void> {
    if (!selectedRef || !jobId) return
    const sequence = ++loadSequence
    loading = true
    error = ''
    try {
      const result = await window.api.ciJobParameters(repoRoot, jobId, selectedRef)
      if (sequence !== loadSequence) return
      parameters = result.parameters
      schemaRevision = result.schemaRevision
      values = initialFormValues(result.parameters)
    } catch (cause) {
      if (sequence !== loadSequence) return
      error = cause instanceof Error ? cause.message : 'Could not load workflow inputs'
    } finally {
      if (sequence === loadSequence) loading = false
    }
  }

  async function runWorkflow(): Promise<void> {
    if (!canRun || !selectedRef || !parameters) return
    running = true
    error = ''
    const failure = await triggerCiJob(
      repoRoot,
      {
        jobId,
        ref: { name: selectedRef.name, kind: selectedRef.kind },
        schemaRevision,
        inputs: toInputs(parameters, values),
      },
      label,
    )
    running = false
    if (failure) {
      error = failure
      if (failure.includes('workflow inputs changed')) await loadParameters()
      return
    }
    closeDialog()
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex items-center justify-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={() => !running && closeDialog()}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="outline-none w-[500px] max-w-[92vw] max-h-[85vh] overflow-y-auto rounded-xl border border-border bg-bg-overlay p-5 shadow-modal flex flex-col gap-4"
    role="dialog"
    aria-modal="true"
    aria-labelledby="github-run-title"
    tabindex="-1"
    onmousedown={(event) => event.stopPropagation()}
  >
    <header class="flex items-start justify-between gap-3">
      <div>
        <h3
          id="github-run-title"
          class="m-0 text-base font-semibold text-text flex items-center gap-2"
        >
          <TrackerProviderIcon provider="github" size={17} /> Run GitHub Actions workflow
        </h3>
        {#if config}
          <p class="m-0 mt-1 text-xs text-text-muted">{config.repository}</p>
        {/if}
      </div>
      <button
        type="button"
        class="size-7 rounded-md border-0 bg-transparent text-text-muted hover:bg-hover hover:text-text aria-disabled:opacity-50"
        onclick={closeDialog}
        aria-label="Close"
        disabled={running}><X size={16} /></button
      >
    </header>

    {#if config}
      <div class="flex flex-col gap-1">
        <label for="github-run-workflow" class="text-xs font-semibold text-text-faint"
          >Workflow</label
        >
        <select
          id="github-run-workflow"
          class="px-2.5 py-1.5 rounded-md border border-border bg-bg-input text-sm text-text"
          value={jobId}
          onchange={(event) => void selectJob(event.currentTarget.value)}
          disabled={running}
        >
          {#each config.workflows as workflow (workflow.path)}
            <option value={workflow.path}>{workflow.label}</option>
          {/each}
        </select>
      </div>

      <div class="flex flex-col gap-1">
        <label for="github-run-ref" class="text-xs font-semibold text-text-faint"
          >Branch or tag</label
        >
        <select
          id="github-run-ref"
          class="px-2.5 py-1.5 rounded-md border border-border bg-bg-input text-sm text-text"
          value={selectedRefKey}
          onchange={(event) => void selectRef(event.currentTarget.value)}
          disabled={loading || running}
        >
          <option value="">Select a remote branch or tag…</option>
          {#each refs as ref (`${ref.kind}:${ref.name}`)}
            <option value={`${ref.kind}:${ref.name}`}
              >{ref.kind === 'tag' ? 'tag' : 'branch'} — {ref.name}</option
            >
          {/each}
        </select>
        {#if initialBranch && !refs.some((ref) => ref.kind === 'branch' && ref.name === initialBranch)}
          <span class="text-xs text-text-muted"
            >The worktree branch is not present on GitHub; choose another ref.</span
          >
        {/if}
      </div>

      {#if parameters}
        <p class="m-0 text-xs text-warning-text">
          Workflow inputs are plain GitHub Actions inputs, not secrets. Do not paste credentials
          here.
        </p>
        <div class="flex flex-col gap-3">
          {#each parameters as parameter (parameter.name)}
            <div class="flex flex-col gap-1">
              {#if parameter.kind === 'checkbox'}
                <label class="flex items-center gap-2 text-sm text-text-secondary">
                  <CustomCheckbox
                    checked={isCheckboxChecked(parameter, values[parameter.name] ?? '')}
                    onchange={() =>
                      (values[parameter.name] = toggleCheckbox(
                        parameter,
                        values[parameter.name] ?? '',
                      ))}
                  />
                  <span>{parameter.label}{parameter.required ? ' *' : ''}</span>
                </label>
              {:else if parameter.kind === 'select'}
                <label
                  for={`github-input-${parameter.name}`}
                  class="text-xs font-semibold text-text-faint"
                >
                  {parameter.label}{parameter.required ? ' *' : ''}
                </label>
                <CustomSelect
                  id={`github-input-${parameter.name}`}
                  value={values[parameter.name] ?? ''}
                  options={[
                    ...(parameter.required ? [] : [{ value: '', label: 'Use workflow default' }]),
                    ...(parameter.options ?? []).map((option) => ({
                      value: option,
                      label: option,
                    })),
                  ]}
                  onchange={(value) => (values[parameter.name] = value)}
                  ariaDescribedby={parameter.description
                    ? `github-input-${parameter.name}-description`
                    : undefined}
                />
              {:else}
                <label
                  class="text-xs font-semibold text-text-faint"
                  for={`github-input-${parameter.name}`}
                >
                  {parameter.label}{parameter.required ? ' *' : ''}
                </label>
                <input
                  id={`github-input-${parameter.name}`}
                  class="px-2.5 py-1.5 rounded-md border border-border bg-bg-input text-sm text-text outline-none focus:border-focus-ring"
                  bind:value={values[parameter.name]}
                  aria-describedby={parameter.description
                    ? `github-input-${parameter.name}-description`
                    : undefined}
                />
              {/if}
              {#if parameter.description}
                <span
                  id={`github-input-${parameter.name}-description`}
                  class="text-xs text-text-faint">{parameter.description}</span
                >
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    {/if}

    {#if loading}
      <div class="flex items-center gap-2 text-xs text-text-muted" role="status">
        <LoaderCircle size={13} class="animate-spin-slow motion-reduce:animate-none" /> Loading…
      </div>
    {/if}
    <div class="min-h-4 text-xs text-danger-text" aria-live="polite">
      {missing.length > 0 ? 'Fill the required workflow inputs.' : error}
    </div>

    <footer class="flex justify-end gap-2 border-t border-border-subtle pt-3">
      <button
        type="button"
        class="px-3 py-1 rounded-md border border-border bg-transparent text-sm text-text-secondary hover:bg-hover"
        onclick={closeDialog}
        disabled={running}>Cancel</button
      >
      <button
        type="button"
        class="px-3 py-1 rounded-md border-0 bg-accent-bg text-accent-text text-sm flex items-center gap-1.5 hover:bg-accent-bg-hover aria-disabled:opacity-50"
        onclick={runWorkflow}
        aria-disabled={!canRun}
        aria-busy={running}
        disabled={!canRun}
      >
        {#if running}<LoaderCircle
            size={13}
            class="animate-spin-slow motion-reduce:animate-none"
          />{:else}<Play size={13} />{/if}
        {running ? 'Starting…' : 'Run workflow'}
      </button>
    </footer>
  </div>
</div>
