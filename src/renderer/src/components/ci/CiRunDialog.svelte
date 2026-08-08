<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { KeyRound, LoaderCircle, Play, X } from '@lucide/svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import { createCiRunDialogState } from '../../lib/ci/runDialogState.svelte'
  import { showProjectCi } from '../../lib/stores/dialogs.svelte'
  import type { CiRepoConfigInfo } from '../../lib/ci/types'
  import BranchPicker from '../worktree/BranchPicker.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'
  import CiRunConfirmation from './CiRunConfirmation.svelte'
  import CiRunParameterFields from './CiRunParameterFields.svelte'

  let {
    repoRoot,
    initialBranch,
    config,
  }: {
    repoRoot: string
    initialBranch?: string
    config: CiRepoConfigInfo
  } = $props()

  let dialogEl = $state<HTMLElement>()
  // A dialog instance is scoped to one repository/config. The router remounts it for another
  // request, so the controller deliberately captures this instance's initial props.
  const runDialog = untrack(() => createCiRunDialogState(repoRoot, initialBranch, config))

  onMount(() => {
    dialogEl?.focus()
    runDialog.initialize()
  })

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault()
      runDialog.close()
    } else if (event.key === 'Tab' && dialogEl) {
      cycleFocus(dialogEl, event)
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex items-center justify-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={runDialog.close}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={dialogEl}
    class="outline-none w-[500px] max-w-[92vw] max-h-[85vh] overflow-y-auto rounded-xl border border-border bg-bg-overlay p-5 shadow-modal flex flex-col gap-4"
    role="dialog"
    aria-modal="true"
    aria-labelledby="ci-run-title"
    tabindex="-1"
    onmousedown={(event) => event.stopPropagation()}
  >
    <header class="flex items-start justify-between gap-3">
      <div class="min-w-0">
        <h3
          id="ci-run-title"
          class="m-0 text-base font-semibold text-text leading-tight flex items-center gap-2"
        >
          <TrackerProviderIcon provider={runDialog.isTeamCity ? 'teamcity' : 'github'} size={17} />
          Run {runDialog.providerName}
          {runDialog.runNoun}
        </h3>
        <p class="m-0 mt-1 text-xs text-text-muted truncate" title={runDialog.providerLine}>
          {runDialog.providerLine}
        </p>
      </div>
      <button
        type="button"
        class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0 aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-muted"
        onclick={runDialog.close}
        aria-disabled={runDialog.running}
        aria-label="Close"
        title={runDialog.running
          ? `Disabled while the ${runDialog.runNoun} request is in flight`
          : 'Close'}
      >
        <X size={16} />
      </button>
    </header>

    {#if runDialog.stage === 'confirm' && runDialog.selectedRef}
      <CiRunConfirmation
        title={runDialog.jobLabel}
        ref={runDialog.isTeamCity
          ? runDialog.selectedRef.name
          : `${runDialog.selectedRef.kind} ${runDialog.selectedRef.name}`}
        changed={runDialog.changed}
        total={runDialog.submitted.length}
        noun={runDialog.isTeamCity ? 'parameters' : 'inputs'}
      />
    {:else if runDialog.stage === 'select'}
      {#if runDialog.runBlockedHint}
        <p class="m-0 break-words text-xs text-text-secondary" id="ci-run-blocked-hint">
          {runDialog.runBlockedHint}
        </p>
      {/if}

      <div class="flex flex-col gap-1">
        <label for="ci-run-job" class="text-xs font-semibold text-text-faint">
          {runDialog.isTeamCity ? 'Job' : 'Workflow'}
        </label>
        <CustomSelect
          id="ci-run-job"
          value={runDialog.jobId}
          options={runDialog.jobs}
          onchange={(value) => void runDialog.selectJob(value)}
        />
      </div>

      <div class="flex flex-col min-h-0 flex-1">
        <BranchPicker
          branches={{ local: runDialog.branchNames, remote: runDialog.tagNames }}
          label={runDialog.refLabel}
          bind:query={runDialog.refQuery}
          bind:selectedBranch={runDialog.selectedRefName}
          refreshing={runDialog.refsLoading}
          onRefresh={runDialog.loadRefs}
          fillQueryOnPick={true}
          highlightPicked={true}
          collapseConfirmedSelection={true}
          startCollapsed={true}
        />
        {#if runDialog.worktreeBranchMissing}
          <span class="text-xs text-text-muted">
            The worktree branch is unavailable on GitHub; choose another branch or tag.
          </span>
        {/if}
        {#if runDialog.ambiguousRefNames.length > 0}
          <span class="text-xs text-text-muted">
            Branch/tag name collisions are unavailable:
            {runDialog.ambiguousRefNames.join(', ')}.
          </span>
        {/if}
      </div>
    {:else if runDialog.parameters}
      <p class="m-0 text-xs text-text-muted">
        {runDialog.jobLabel} · <span class="font-mono">{runDialog.selectedRefName}</span>
      </p>
      <CiRunParameterFields
        provider={config.provider}
        parameters={runDialog.parameters}
        bind:values={runDialog.values}
        validationErrorId="ci-run-validation-error"
      />
    {/if}

    {#if runDialog.loading}
      <div class="flex items-center gap-2 text-xs text-text-muted" role="status">
        <LoaderCircle size={13} class="animate-spin-slow motion-reduce:animate-none" /> Loading…
      </div>
    {/if}

    {#if runDialog.dispatchDenied}
      <div
        class="flex items-start gap-2 rounded-lg border border-experimental-border bg-experimental-bg px-3 py-2"
        role="alert"
      >
        <KeyRound size={13} class="mt-0.5 shrink-0 text-warning-text" />
        <span class="flex-1 min-w-0 text-xs text-text-secondary leading-snug">
          This token can read workflows but not start them. It needs <strong
            class="font-semibold text-text">Actions: read and write</strong
          >{config.provider === 'github-actions' ? ` for ${config.repository}` : ''}. Update it in
          the CI/CD configurator, which has the token field and a link that pre-selects that
          permission.
          <span class="text-text-faint">({runDialog.triggerError})</span>
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
      <div
        id="ci-run-validation-error"
        class:sr-only={!runDialog.visibleError}
        class="text-xs text-danger-text"
        aria-live="polite"
      >
        {runDialog.visibleError}
      </div>
    {/if}

    <footer class="flex justify-end gap-2 border-t border-border-subtle pt-3">
      {#if runDialog.parametersError}
        <button
          type="button"
          class="mr-auto px-3 py-1 rounded-md border border-border bg-transparent text-sm text-text-secondary cursor-pointer hover:bg-hover"
          onclick={runDialog.loadParameters}>Retry {runDialog.parameterNoun}</button
        >
      {/if}
      <button
        type="button"
        class="px-3 py-1 rounded-md border border-border bg-transparent text-sm text-text-secondary cursor-pointer hover:bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent"
        onclick={runDialog.cancelOrBack}
        aria-disabled={runDialog.running}
        title={runDialog.running
          ? `Disabled while the ${runDialog.runNoun} request is in flight`
          : runDialog.stage === 'select'
            ? 'Close without running'
            : 'Back to the previous step'}
      >
        {runDialog.stage === 'select' ? 'Cancel' : 'Back'}
      </button>
      <button
        type="button"
        class="flex items-center justify-center gap-1.5 min-w-28 px-3 py-1 rounded-md border-0 bg-accent-bg text-accent-text text-sm cursor-pointer hover:bg-accent-bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-accent-bg"
        onclick={runDialog.primaryAction}
        aria-disabled={!runDialog.canContinue}
        aria-busy={runDialog.running || runDialog.loading}
        aria-describedby={runDialog.runBlockedHint ? 'ci-run-blocked-hint' : undefined}
        title={runDialog.primaryTitle}
      >
        {#if runDialog.running || runDialog.loading}
          <LoaderCircle size={13} class="animate-spin-slow motion-reduce:animate-none" />
        {:else}
          <Play size={13} />
        {/if}
        {runDialog.primaryLabel}
      </button>
    </footer>
  </div>
</div>
