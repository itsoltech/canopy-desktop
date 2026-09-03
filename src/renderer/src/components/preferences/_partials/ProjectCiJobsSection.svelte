<script lang="ts">
  import { Check, LoaderCircle, X } from '@lucide/svelte'
  import CiJobPicker from '../../ci/CiJobPicker.svelte'
  import { CI_MAX_BUILD_TYPES } from '../../../lib/ci/limits'
  import type { ProjectCiModalState } from '../projectCiModalState.svelte'

  let { state, class: className = '' }: { state: ProjectCiModalState; class?: string } = $props()

  const shorten = (id: string): string => (id.length > 80 ? `${id.slice(0, 80)}…` : id)
</script>

<section class={`rounded-lg border border-border-subtle p-4 flex flex-col gap-3 ${className}`}>
  <div>
    <h3 class="m-0 text-sm font-semibold text-text">Shared jobs</h3>
    <p class="m-0 mt-0.5 text-xs text-text-muted leading-snug">
      Select the jobs shown to everyone through this repository's
      <code class="font-mono">.canopy/config.json</code>.
    </p>
  </div>

  <div class="flex items-center gap-1.5">
    {#if state.trimmedFormToken}
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg-input text-text-secondary hover:bg-hover-strong hover:text-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-bg-input aria-disabled:hover:text-text-secondary"
        onclick={state.testConnection}
        aria-disabled={state.testing || state.typesLoading || !state.urlValid}
        aria-busy={state.testing}
        aria-describedby={state.serverBlockedReason ? 'ci-server-blocked' : undefined}
        title={state.testBlockedTitle ||
          'Check the connection against the server - nothing is saved'}
      >
        {state.testing ? 'Testing…' : 'Test'}
      </button>
    {/if}
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg-input text-text-secondary hover:bg-hover-strong hover:text-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-bg-input aria-disabled:hover:text-text-secondary"
      onclick={state.loadBuildTypes}
      aria-disabled={state.typesLoading || state.testing || !state.canLoadTypes}
      aria-busy={state.typesLoading}
      aria-describedby={state.serverBlockedReason ? 'ci-server-blocked' : undefined}
      title={state.loadBlockedTitle ||
        'Saves the token (when entered) and fetches the list of jobs (build configurations) from the TeamCity server'}
    >
      {state.typesLoading ? 'Loading…' : 'Load available jobs'}
    </button>
    <span class="min-w-4" aria-live="polite">
      {#if state.testResult === 'success'}
        <span class="flex items-center gap-1 text-xs text-success"><Check size={13} /> OK</span>
      {:else if state.testResult === 'fail'}
        <span class="flex items-center gap-1 text-xs text-danger-text"><X size={13} /> Failed</span>
      {/if}
    </span>
  </div>

  <div id="ci-server-blocked" class="min-h-4 text-xs text-text-secondary break-words">
    {state.serverBlockedReason}
  </div>

  <div class:sr-only={!state.typesError} role="status">
    {#if state.typesError}
      <span class="text-xs text-danger-text">{state.typesError}</span>
    {/if}
  </div>

  <div
    role="status"
    id="ci-save-warnings"
    class:sr-only={!state.existingConfig?.droppedInvalid &&
      !state.existingConfig?.droppedOverCap &&
      !(state.typesLoaded && state.missingBuildTypes.length > 0)}
  >
    {#if state.existingConfig?.droppedInvalid}
      {@const inv = state.existingConfig.droppedInvalid}
      <p class="m-0 text-xs text-warning-text leading-snug break-words">
        {inv.count} hand-edited
        {inv.count === 1 ? 'entry has an invalid id' : 'entries have invalid ids'}
        ({inv.ids.join(', ')}{inv.count > inv.ids.length
          ? ` and ${inv.count - inv.ids.length} more`
          : ''}) - not TeamCity ids, so they cannot appear below. Correct them in
        <code class="font-mono">.canopy/config.json</code> to keep them; saving without doing so drops
        them.
      </p>
    {/if}
    {#if state.existingConfig?.droppedOverCap}
      {@const cap = state.existingConfig.droppedOverCap}
      {@const capPresent = cap.ids.filter((id) =>
        state.serverTypes.some((buildType) => buildType.id === id),
      )}
      {@const capGone = cap.ids.filter(
        (id) => !state.serverTypes.some((buildType) => buildType.id === id),
      )}
      <p class="m-0 text-xs text-warning-text leading-snug break-words">
        {cap.count} hand-edited {cap.count === 1 ? 'entry is' : 'entries are'} past the
        {CI_MAX_BUILD_TYPES}-job cap and not selected{cap.count > cap.ids.length
          ? ` (showing ${cap.ids.length} of ${cap.count})`
          : ''}.
        {#if !state.typesLoaded}
          Load the available jobs to see which of these can still be ticked: {cap.ids
            .map(shorten)
            .join(', ')}.
        {:else}
          {#if capPresent.length > 0}
            Untick another job below first, then tick these to keep them: {capPresent
              .map(shorten)
              .join(', ')}.
          {/if}
          {#if capGone.length > 0}
            No longer on this server - saving drops them and there is nothing to re-tick: {capGone
              .map(shorten)
              .join(', ')}.
          {/if}
        {/if}
        Or trim the hand-edited list; saving writes only the selection below.
      </p>
    {/if}
    {#if state.typesLoaded && state.missingBuildTypes.length > 0}
      {@const missingNames = state.missingBuildTypes
        .map((id) => {
          const label = state.selected.get(id)
          return label && label !== id ? `${label} (${id})` : id
        })
        .join(', ')}
      <p class="m-0 text-xs text-warning-text leading-snug break-words">
        {#if state.allConfiguredStale && state.effectiveBuildTypes.length === 0}
          None of this repository's configured jobs exist on this server any more ({missingNames}).
          Save is disabled until you tick at least one job below - or use
          <strong>Remove CI configuration</strong> to drop the
          <code class="font-mono">ci</code> block entirely.
        {:else if state.effectiveBuildTypes.length > 0}
          {state.missingBuildTypes.length} configured
          {state.missingBuildTypes.length === 1 ? 'job is' : 'jobs are'} no longer on this server ({missingNames}).
          Saving drops {state.missingBuildTypes.length === 1 ? 'it' : 'them'} from
          <code class="font-mono">.canopy/config.json</code>.
        {:else}
          {state.missingBuildTypes.length} configured
          {state.missingBuildTypes.length === 1 ? 'job is' : 'jobs are'} no longer on this server ({missingNames}).
          Tick at least one job below to save - the missing
          {state.missingBuildTypes.length === 1 ? 'entry is' : 'entries are'} dropped from
          <code class="font-mono">.canopy/config.json</code> when you do.
        {/if}
      </p>
    {/if}
  </div>

  {#if state.typesLoading && !state.typesLoaded}
    <div class="flex items-center gap-2 text-sm text-text-faint">
      <LoaderCircle size={13} class="animate-spin-slow motion-reduce:animate-none" />
      Loading available jobs…
    </div>
  {:else if state.typesLoaded}
    <div class={state.typesLoading ? 'opacity-50 pointer-events-none' : ''}>
      <CiJobPicker
        serverTypes={state.serverTypes}
        selected={state.selected}
        onToggle={state.toggleType}
        onLabelChange={state.setLabel}
      />
    </div>
  {/if}
</section>
