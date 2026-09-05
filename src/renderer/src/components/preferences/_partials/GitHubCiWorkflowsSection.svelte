<script lang="ts">
  import { Check, LoaderCircle } from '@lucide/svelte'
  import CiJobPicker from '../../ci/CiJobPicker.svelte'
  import { CI_MAX_WORKFLOWS } from '../../../lib/ci/limits'
  import type { GitHubActionsCiConfiguratorState } from '../githubActionsCiConfiguratorState.svelte'

  let {
    state,
    class: className = '',
  }: { state: GitHubActionsCiConfiguratorState; class?: string } = $props()
</script>

<section class={`rounded-lg border border-border-subtle p-4 flex flex-col gap-3 ${className}`}>
  <div>
    <h3 class="m-0 text-sm font-semibold text-text">Shared workflows</h3>
    <p class="m-0 mt-0.5 text-xs text-text-muted leading-snug">
      Select the workflows shown to everyone through this repository's
      <code class="font-mono">.canopy/config.json</code>.
    </p>
  </div>

  <div class="flex items-center gap-2">
    {#if state.repositoryReady && state.token.trim()}
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm border border-border bg-bg-input text-text-secondary cursor-pointer hover:bg-hover-strong aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-bg-input"
        onclick={state.testConnection}
        aria-disabled={state.testing || state.loading}
        aria-busy={state.testing}
      >
        {state.testing ? 'Testing…' : 'Test connection'}
      </button>
    {/if}
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm border border-border bg-bg-input text-text-secondary cursor-pointer hover:bg-hover-strong aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-bg-input"
      onclick={state.loadWorkflows}
      aria-disabled={state.loadBlocked}
      aria-describedby={state.loadBlockedReason ? 'github-ci-load-blocked' : undefined}
      aria-busy={state.loading}
    >
      {state.loading ? 'Loading…' : 'Load workflows'}
    </button>
    <span class="text-xs" aria-live="polite">
      {#if state.testResult === 'success'}
        <span class="text-success flex items-center gap-1"><Check size={13} /> Connected</span>
      {:else if state.testResult === 'fail'}
        <span class="text-danger-text">Connection failed</span>
      {/if}
    </span>
  </div>

  <div id="github-ci-load-blocked" class="min-h-4 text-xs text-text-muted" aria-live="polite">
    {state.loadBlockedReason}
  </div>
  <div class="min-h-5 text-xs text-danger-text break-words" role="status">{state.error}</div>

  {#if state.existingConfig?.droppedInvalid}
    {@const invalid = state.existingConfig.droppedInvalid}
    <p class="m-0 text-xs text-warning-text leading-snug break-words" role="status">
      {invalid.count} hand-edited
      {invalid.count === 1 ? 'workflow entry has' : 'workflow entries have'} an invalid path ({invalid.ids.join(
        ', ',
      )}{invalid.count > invalid.ids.length
        ? ` and ${invalid.count - invalid.ids.length} more`
        : ''}). Correct them in <code class="font-mono">.canopy/config.json</code> to keep them; saving
      drops them.
    </p>
  {/if}

  {#if state.existingConfig?.droppedOverCap}
    {@const overCap = state.existingConfig.droppedOverCap}
    {@const availableOverCap = overCap.ids.filter((path) =>
      state.workflows.some(
        (workflow) => workflow.available && workflow.path.toLowerCase() === path.toLowerCase(),
      ),
    )}
    <p class="m-0 text-xs text-warning-text leading-snug break-words" role="status">
      {overCap.count} hand-edited
      {overCap.count === 1 ? 'workflow is' : 'workflows are'} past the
      {CI_MAX_WORKFLOWS}-workflow cap and not selected{overCap.count > overCap.ids.length
        ? ` (showing ${overCap.ids.length} of ${overCap.count})`
        : ''}.
      {#if state.loaded && availableOverCap.length > 0}
        Untick another workflow first, then tick these to keep them: {availableOverCap.join(', ')}.
      {:else if !state.loaded}
        Load workflows to see which can still be selected: {overCap.ids.join(', ')}.
      {:else}
        The sampled workflows are no longer dispatchable; saving drops them.
      {/if}
    </p>
  {/if}

  {#if state.loading && !state.loaded}
    <div class="flex items-center gap-2 text-sm text-text-muted" role="status">
      <LoaderCircle size={14} class="animate-spin-slow motion-reduce:animate-none" />
      Loading dispatchable workflows…
    </div>
  {:else if state.loaded}
    {#if state.availableWorkflows.length > 0}
      <CiJobPicker
        serverTypes={state.availableWorkflows}
        provider="github-actions"
        selected={state.selected}
        onToggle={state.toggleWorkflow}
        onLabelChange={state.setLabel}
      />
    {:else}
      <p class="m-0 text-sm text-text-muted">
        No active workflows with <code class="font-mono">workflow_dispatch</code> were found.
      </p>
    {/if}
    {#if state.unavailableWorkflows.length > 0}
      <div class="flex flex-col gap-1">
        <span class="text-xs font-medium text-text-muted">Unavailable workflows</span>
        {#each state.unavailableWorkflows as workflow (workflow.path)}
          <div class="text-xs text-text-faint break-words">
            {workflow.name} — {workflow.error || 'not dispatchable'}
          </div>
        {/each}
      </div>
    {/if}
  {/if}
  <div role="status" class:sr-only={state.missingConfiguredWorkflows.length === 0}>
    {#if state.missingConfiguredWorkflows.length > 0}
      <div class="p-2 rounded-md bg-warning-bg text-xs text-warning-text break-words">
        No longer returned by GitHub and removed on Save:
        {state.missingConfiguredWorkflows.map((workflow) => workflow.label).join(', ')}
      </div>
    {/if}
  </div>
</section>
