<script lang="ts">
  import BranchPicker from '../worktree/BranchPicker.svelte'
  import CiRunConfirmation from './CiRunConfirmation.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import { onMount, untrack } from 'svelte'
  import { KeyRound, LoaderCircle, Play, X } from '@lucide/svelte'
  import { closeDialog, showProjectCi } from '../../lib/stores/dialogs.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import { triggerCiJob } from '../../lib/stores/ci.svelte'
  import { isCiAuthFailure } from '../../lib/ci/errors'
  import type { CiParameter, CiRef, GitHubActionsCiRepoConfigInfo } from '../../lib/ci/types'
  import {
    changedProperties,
    initialFormValues,
    isCheckboxChecked,
    missingRequired,
    toInputs,
    toggleCheckbox,
  } from '../../lib/ci/runBuildForm'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'

  let {
    repoRoot,
    initialBranch,
    initialConfig,
  }: {
    repoRoot: string
    initialBranch?: string
    initialConfig: GitHubActionsCiRepoConfigInfo
  } = $props()
  let containerEl: HTMLElement | undefined = $state()
  let config = $state<GitHubActionsCiRepoConfigInfo | null>(null)
  let jobId = $state('')
  let refs = $state<CiRef[]>([])
  // BranchPicker deals in plain names; the kind is resolved back from `refs`, with branches
  // and tags in separate buckets so a name shared by both is unambiguous.
  let selectedRefName = $state('')
  let refQuery = $state('')
  let parameters = $state<CiParameter[] | null>(null)
  let schemaRevision = $state('')
  let values = $state<Record<string, string>>({})
  let loading = $state(true)
  let running = $state(false)
  let error = $state('')
  // A 403 here is not "wrong token" — reading workflows already worked to get this far. It is
  // the one permission dispatch needs and read does not, so the message names it and offers the
  // link that pre-selects it, instead of leaving the user with "Forbidden".
  let dispatchDenied = $derived(isCiAuthFailure(error))
  let loadSequence = 0

  let branchNames = $derived(refs.filter((r) => r.kind === 'branch').map((r) => r.name))
  let tagNames = $derived(refs.filter((r) => r.kind === 'tag').map((r) => r.name))
  let selectedRef = $derived(
    selectedRefName
      ? (refs.find((r) => r.kind === 'branch' && r.name === selectedRefName) ??
          refs.find((r) => r.name === selectedRefName))
      : undefined,
  )
  let label = $derived(
    config?.workflows.find((workflow) => workflow.path === jobId)?.label ?? jobId,
  )
  let missing = $derived(parameters ? missingRequired(parameters, values) : [])
  let canRun = $derived(
    !!jobId && !!selectedRef && parameters !== null && missing.length === 0 && !loading && !running,
  )
  let runBlockedReason = $derived(
    running
      ? 'The workflow request is in progress.'
      : loading
        ? 'Loading workflow details…'
        : !jobId
          ? 'Select a workflow.'
          : !selectedRef
            ? 'Select a remote branch or tag.'
            : parameters === null
              ? 'Workflow inputs could not be loaded.'
              : missing.length > 0
                ? 'Fill the required workflow inputs.'
                : '',
  )
  let runBlockedHint = $derived(
    running || loading
      ? ''
      : !jobId
        ? 'Select a workflow before running.'
        : !selectedRef
          ? 'Select a remote branch or tag before running.'
          : '',
  )

  onMount(async () => {
    containerEl?.focus()
    try {
      config = initialConfig
      jobId = initialConfig.workflows[0]?.path ?? ''
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
      requestClose()
    } else if (event.key === 'Tab' && containerEl) {
      cycleFocus(containerEl, event)
    }
  }

  function requestClose(): void {
    if (!running) closeDialog()
  }

  async function selectJob(path: string): Promise<void> {
    jobId = path
    selectedRefName = ''
    refQuery = ''
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
      selectedRefName = worktreeRef ? worktreeRef.name : ''
      refQuery = selectedRefName
    } catch (cause) {
      if (sequence !== loadSequence) return
      error = cause instanceof Error ? cause.message : 'Could not load GitHub refs'
    } finally {
      if (sequence === loadSequence) loading = false
    }
  }

  // BranchPicker's row click calls pick() without onCommit, so the selection is OBSERVED —
  // one path for mouse, Enter and the worktree preselect. Guarded so a re-render cannot
  // reload the same ref.
  let loadedForRef = ''
  $effect(() => {
    const name = selectedRefName
    if (name === loadedForRef) return
    loadedForRef = name
    untrack(() => void refPicked())
  })

  /** Picking a ref invalidates the parameter schema, which is per-ref. */
  async function refPicked(): Promise<void> {
    parameters = null
    schemaRevision = ''
    values = {}
    if (selectedRefName) await loadParameters()
  }

  async function loadParameters(): Promise<void> {
    if (!selectedRef || !jobId) return
    const sequence = ++loadSequence
    loading = true
    error = ''
    try {
      // $state.snapshot: `selectedRef` is an element of a reactive array, so it reaches the
      // bridge as a Proxy — structured clone rejects those with "An object could not be
      // cloned", which surfaced as an error under the ref picker.
      const result = await window.api.ciJobParameters(repoRoot, jobId, $state.snapshot(selectedRef))
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

  // Two-step: the same confirmation the TeamCity dialog uses, in the app rather than a
  // native message box, so both providers ask the same question the same way.
  // Two screens, matching TeamCity: pick the workflow and ref, then configure inputs if the
  // workflow declares any. Inputs are per-REF (the YAML can differ per branch), so the ref has
  // to be chosen first — but the split itself is for consistency, not forced by that.
  let stage = $state<'select' | 'configure'>('select')
  let hasInputs = $derived((parameters?.length ?? 0) > 0)
  $effect(() => {
    void jobId
    void selectedRefName
    stage = 'select'
    pending = false
  })

  let pending = $state(false)
  let submitted = $derived(
    parameters ? parameters.map((p) => ({ name: p.name, value: values[p.name] ?? '' })) : [],
  )
  let pendingChanged = $derived(parameters ? changedProperties(parameters, submitted) : [])

  function primaryAction(): void {
    if (!canRun) return
    if (stage === 'select' && hasInputs) {
      stage = 'configure'
      return
    }
    if (!pending) {
      pending = true
      return
    }
    void runWorkflow()
  }

  /** Cancel steps back out of the inputs screen before it closes the dialog, like TeamCity. */
  function cancelOrBack(): void {
    if (running) return
    if (stage === 'configure') {
      stage = 'select'
      pending = false
      return
    }
    requestClose()
  }

  async function runWorkflow(): Promise<void> {
    if (!canRun || !selectedRef || !parameters) return
    pending = false
    running = true
    error = ''
    const issue = await triggerCiJob(
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
    if (issue?.kind === 'cancelled') return
    if (issue?.kind === 'failure') {
      if (issue.code === 'CiWorkflowSchemaChanged') {
        await loadParameters()
        error = error ? `${issue.message} ${error}` : issue.message
      } else {
        error = issue.message
      }
      return
    }
    closeDialog()
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex items-center justify-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={requestClose}
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
        class="size-7 rounded-md border-0 bg-transparent text-text-muted cursor-pointer hover:bg-hover hover:text-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-muted"
        onclick={requestClose}
        aria-label="Close"
        aria-disabled={running}
        title={running ? 'Disabled while the workflow request is in progress' : 'Close'}
        ><X size={16} /></button
      >
    </header>

    {#if config}
      <!-- Above the fields, not under the button: it names what still has to be CHOSEN, so it
           belongs where the choosing happens. Its own container rather than the aria-live
           error slot — this toggles with every selection and would announce on each one.
           Rendered only when it says something: a reserved empty line left a permanent gap
           under the header, which the TeamCity dialog does not have. -->
      {#if runBlockedHint}
        <p class="m-0 break-words text-xs text-text-secondary" id="github-run-blocked-hint">
          {runBlockedHint}
        </p>
      {/if}

      {#if stage === 'select'}
        <div class="flex flex-col gap-1">
          <span class="text-xs font-semibold text-text-faint">Workflow</span>
          <!-- Same picker as the TeamCity job list: a native select renders in the OS theme,
               which is what left a bright system highlight in the middle of the dialog. -->
          <CustomSelect
            value={jobId}
            options={config.workflows.map((workflow) => ({
              value: workflow.path,
              label: workflow.label,
            }))}
            onchange={(value) => void selectJob(value)}
          />
        </div>

        <div class="flex flex-col">
          <!-- Same picker as the TeamCity dialog. A native select cannot be searched and renders
             in the OS theme; this repository lists hundreds of refs (audit/*, dependabot/*). -->
          <BranchPicker
            branches={{ local: branchNames, remote: tagNames }}
            label="Branch or tag"
            bind:query={refQuery}
            bind:selectedBranch={selectedRefName}
            refreshing={loading}
            onRefresh={loadRefs}
            fillQueryOnPick={true}
            highlightPicked={true}
            collapseConfirmedSelection={true}
            startCollapsed={true}
          />
          {#if initialBranch && !refs.some((ref) => ref.kind === 'branch' && ref.name === initialBranch)}
            <span class="text-xs text-text-muted"
              >The worktree branch is not present on GitHub; choose another ref.</span
            >
          {/if}
        </div>
      {/if}

      <!-- Its own screen now, reached with Configure — the same shape as the TeamCity
           parameters form, so both providers read alike. -->
      {#if stage === 'configure' && parameters && parameters.length > 0}
        <!-- What is being configured: without it the screen shows inputs with no idea which
             workflow or ref they belong to. The TeamCity form states the same two facts. -->
        <p class="m-0 text-xs text-text-muted">
          {label} · <span class="font-mono">{selectedRefName}</span>
        </p>
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
    {#if dispatchDenied}
      <div
        class="flex items-start gap-2 rounded-lg border border-experimental-border bg-experimental-bg px-3 py-2"
        role="alert"
      >
        <KeyRound size={13} class="mt-0.5 shrink-0 text-warning-text" />
        <span class="flex-1 min-w-0 text-xs text-text-secondary leading-snug">
          This token can read workflows but not start them. It needs <strong
            class="font-semibold text-text">Actions: read and write</strong
          >{config ? ` for ${config.repository}` : ''}. Update it in the CI/CD configurator, which
          has the token field and a link that pre-selects that permission.
          <span class="text-text-faint">({error})</span>
        </span>
        <button
          type="button"
          class="shrink-0 self-center px-2 py-0.5 rounded-md border border-border bg-transparent text-xs text-text-secondary font-inherit cursor-pointer hover:border-accent-muted hover:text-accent-text"
          onclick={showProjectCi}
        >
          Update token
        </button>
      </div>
    {:else}
      <div class="min-h-4 text-xs text-danger-text" aria-live="polite">
        {missing.length > 0 ? 'Fill the required workflow inputs.' : error}
      </div>
    {/if}

    {#if pending && selectedRef}
      <CiRunConfirmation
        ref={`${selectedRef.kind} ${selectedRef.name}`}
        changed={pendingChanged}
        total={submitted.length}
        noun="inputs"
      />
    {/if}
    <footer class="flex justify-end gap-2 border-t border-border-subtle pt-3">
      <button
        type="button"
        class="px-3 py-1 rounded-md border border-border bg-transparent text-sm text-text-secondary cursor-pointer hover:bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent"
        onclick={cancelOrBack}
        aria-disabled={running}
        title={running ? 'Disabled while the workflow request is in progress' : 'Cancel'}
        >Cancel</button
      >
      <button
        type="button"
        class="px-3 py-1 rounded-md border-0 bg-accent-bg text-accent-text text-sm cursor-pointer flex items-center gap-1.5 hover:bg-accent-bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-accent-bg"
        onclick={primaryAction}
        aria-disabled={!canRun}
        aria-busy={running}
        aria-describedby={runBlockedHint ? 'github-run-blocked-hint' : undefined}
        title={runBlockedReason || 'Run the selected workflow'}
      >
        {#if running}<LoaderCircle
            size={13}
            class="animate-spin-slow motion-reduce:animate-none"
          />{:else}<Play size={13} />{/if}
        {running
          ? 'Starting…'
          : pending
            ? 'Start workflow'
            : hasInputs && stage === 'select'
              ? 'Configure'
              : 'Confirm'}
      </button>
    </footer>
  </div>
</div>
