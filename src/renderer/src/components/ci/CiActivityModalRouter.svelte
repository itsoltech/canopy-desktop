<script lang="ts">
  import { onMount } from 'svelte'
  import { LoaderCircle, X } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import { ipcErrorMessage } from '../../lib/ci/errors'
  import CiActivityModal from './CiActivityModal.svelte'
  import GitHubActionsActivityModal from './GitHubActionsActivityModal.svelte'

  let { repoRoot, branch }: { repoRoot: string; branch?: string } = $props()
  let provider = $state<'teamcity' | 'github-actions' | ''>('')
  let error = $state('')
  let loading = $state(true)
  let dialogEl: HTMLElement | undefined = $state()

  onMount(async () => {
    dialogEl?.focus()
    try {
      const result = await window.api.ciConfig(repoRoot)
      provider = result.config?.provider ?? ''
      if (!provider) error = result.invalid?.message ?? 'No CI configured for this repository'
    } catch (cause) {
      error = ipcErrorMessage(cause, 'Could not load CI configuration')
    } finally {
      loading = false
    }
  })

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault()
      closeDialog()
    } else if (event.key === 'Tab' && dialogEl) {
      cycleFocus(dialogEl, event)
    }
  }
</script>

{#if provider === 'github-actions'}
  <GitHubActionsActivityModal {repoRoot} initialBranch={branch} />
{:else if provider === 'teamcity'}
  <CiActivityModal {repoRoot} initialBranch={branch} />
{:else}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-overlay flex items-center justify-center bg-scrim"
    onmousedown={closeDialog}
    onkeydown={handleKeydown}
  >
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      bind:this={dialogEl}
      class="outline-none w-[420px] max-w-[92vw] rounded-xl border border-border bg-bg-overlay p-5 shadow-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="ci-activity-router-title"
      tabindex="-1"
      onmousedown={(event) => event.stopPropagation()}
    >
      <div class="flex items-start justify-between gap-3">
        <h3 id="ci-activity-router-title" class="m-0 text-base font-semibold text-text">
          {loading ? 'Loading CI history' : 'Cannot load CI history'}
        </h3>
        <button
          type="button"
          class="flex size-7 shrink-0 items-center justify-center rounded-md border-0 bg-transparent text-text-muted hover:bg-hover"
          onclick={closeDialog}
          aria-label="Close"
        >
          <X size={16} />
        </button>
      </div>
      {#if loading}
        <div class="my-3 flex items-center gap-2 text-sm text-text-muted" role="status">
          <LoaderCircle size={14} class="animate-spin-slow motion-reduce:animate-none" /> Loading…
        </div>
      {:else}
        <p class="my-3 text-sm text-danger-text" role="alert">{error}</p>
      {/if}
      <div class="flex justify-end">
        <button
          type="button"
          class="px-3 py-1 rounded-md border border-border bg-transparent text-sm text-text-secondary hover:bg-hover"
          onclick={closeDialog}>Close</button
        >
      </div>
    </div>
  </div>
{/if}
