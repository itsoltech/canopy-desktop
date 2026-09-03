<script lang="ts">
  import { LoaderCircle, Trash2 } from '@lucide/svelte'
  import type { ProjectCiModalState } from '../projectCiModalState.svelte'

  let { state, class: className = '' }: { state: ProjectCiModalState; class?: string } = $props()
</script>

<footer
  class={`px-6 py-3 border-t border-border-subtle shrink-0 flex items-center justify-between gap-2 ${className}`}
>
  <div>
    {#if state.existingConfig}
      <button
        type="button"
        class="flex items-center gap-1 px-2 py-1 rounded-md border-0 bg-transparent text-text-faint text-xs font-inherit cursor-pointer hover:text-danger-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:text-text-faint"
        onclick={state.removeConfiguration}
        aria-disabled={state.busy !== ''}
        aria-busy={state.busy === 'remove'}
        title={state.busy !== ''
          ? 'Disabled while an update is writing .canopy/config.json'
          : 'Removes the ci block from the git-tracked .canopy/config.json'}
      >
        {#if state.busy === 'remove'}
          <LoaderCircle size={12} class="animate-spin-slow motion-reduce:animate-none" />
        {:else}
          <Trash2 size={12} />
        {/if}
        Remove CI configuration
      </button>
    {/if}
  </div>
  <div class="flex-1 min-w-0 flex flex-col gap-0.5">
    <div class="text-xs text-danger-text break-words" aria-live="polite">
      {state.saveError}
    </div>
    <div
      id="ci-save-blocked"
      class="min-h-4 text-xs break-words {state.saveBlockedState.severity === 'warn'
        ? 'text-warning-text'
        : 'text-text-secondary'}"
    >
      {state.saveBlockedState.reason}
    </div>
  </div>
  <div class="flex items-center gap-1.5">
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-secondary"
      onclick={state.requestClose}
      aria-disabled={state.busy !== ''}
      title={state.busy !== ''
        ? 'Disabled while an update is writing .canopy/config.json'
        : 'Close without saving'}
    >
      Cancel
    </button>
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-accent-bg"
      onclick={state.saveConfiguration}
      aria-disabled={state.saveBlocked}
      aria-describedby={state.saveBlockedState.reason
        ? 'ci-save-blocked'
        : state.missingBuildTypes.length > 0 ||
            state.existingConfig?.droppedInvalid ||
            state.existingConfig?.droppedOverCap
          ? 'ci-save-warnings'
          : undefined}
      title={state.saveBlockedState.reason ||
        'Writes the ci block to .canopy/config.json - commit it to share with the team'}
    >
      {state.busy === 'save' ? 'Saving…' : 'Save configuration'}
    </button>
  </div>
</footer>
