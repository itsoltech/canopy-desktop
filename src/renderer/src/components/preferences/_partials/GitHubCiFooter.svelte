<script lang="ts">
  import { Trash2 } from '@lucide/svelte'
  import type { GitHubActionsCiConfiguratorState } from '../githubActionsCiConfiguratorState.svelte'

  let {
    state,
    class: className = '',
  }: { state: GitHubActionsCiConfiguratorState; class?: string } = $props()
</script>

<footer
  class={`px-6 py-3 border-t border-border-subtle flex items-center justify-between gap-3 ${className}`}
>
  <div>
    {#if state.existingConfig}
      <button
        type="button"
        class="flex items-center gap-1 px-2 py-1 border-0 bg-transparent text-xs text-text-faint cursor-pointer hover:text-danger-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:text-text-faint"
        onclick={state.removeConfiguration}
        aria-disabled={state.saving}
      >
        <Trash2 size={12} /> Remove CI configuration
      </button>
    {/if}
  </div>
  <div
    id="github-ci-save-blocked"
    class="flex-1 min-w-0 min-h-4 text-xs text-text-secondary break-words"
  >
    {state.saveBlockedReason}
  </div>
  <div class="flex items-center gap-2">
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm border border-border bg-transparent text-text-secondary cursor-pointer hover:bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent"
      onclick={state.requestClose}
      aria-disabled={state.saving}
      title={state.saving ? 'Disabled while the configuration is being saved' : 'Cancel'}
    >
      Cancel
    </button>
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm border-0 bg-accent-bg text-accent-text cursor-pointer hover:bg-accent-bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-accent-bg"
      onclick={state.saveConfiguration}
      aria-disabled={state.saveBlocked}
      aria-describedby={state.saveBlockedReason ? 'github-ci-save-blocked' : undefined}
      title={state.saveBlockedReason || 'Save configuration'}
    >
      {state.saving ? 'Saving…' : 'Save configuration'}
    </button>
  </div>
</footer>
