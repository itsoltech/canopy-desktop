<script lang="ts">
  import { onMount } from 'svelte'
  import { LoaderCircle, Server, X } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import ProjectCiModal from './ProjectCiModal.svelte'
  import GitHubActionsCiConfigurator from './GitHubActionsCiConfigurator.svelte'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'

  let repoRoot = $derived(workspaceState.selectedWorktreePath ?? workspaceState.repoRoot)
  let provider = $state<'teamcity' | 'github-actions' | ''>('')
  let loading = $state(true)
  let loadError = $state('')
  let containerEl: HTMLElement | undefined = $state()

  onMount(async () => {
    containerEl?.focus()
    if (!repoRoot) {
      loading = false
      return
    }
    try {
      const result = await window.api.ciConfig(repoRoot)
      provider = result.config?.provider ?? result.invalid?.provider ?? ''
      if (result.invalid && !result.invalid.provider) loadError = result.invalid.message
    } catch (error) {
      loadError = error instanceof Error ? error.message : 'Could not load CI configuration'
    } finally {
      loading = false
    }
  })

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault()
      closeDialog()
    } else if (event.key === 'Tab' && containerEl) {
      cycleFocus(containerEl, event)
    }
  }
</script>

{#if provider === 'teamcity'}
  <ProjectCiModal />
{:else if provider === 'github-actions'}
  <GitHubActionsCiConfigurator />
{:else}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-overlay flex justify-center items-center bg-scrim"
    onkeydown={handleKeydown}
    onmousedown={closeDialog}
  >
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      bind:this={containerEl}
      class="outline-none w-[520px] max-w-[92vw] bg-bg-overlay border border-border rounded-xl shadow-modal overflow-hidden"
      role="dialog"
      aria-modal="true"
      aria-labelledby="ci-provider-title"
      tabindex="-1"
      onmousedown={(event) => event.stopPropagation()}
    >
      <header class="px-6 py-4 border-b border-border-subtle flex items-center justify-between">
        <div>
          <h2 id="ci-provider-title" class="m-0 text-lg font-semibold text-text">
            Configure CI/CD
          </h2>
          <p class="m-0 mt-1 text-xs text-text-muted">
            Choose the single CI provider used by this repository.
          </p>
        </div>
        <button
          type="button"
          class="size-7 rounded-md border-0 bg-transparent text-text-muted hover:bg-hover hover:text-text"
          onclick={closeDialog}
          aria-label="Close"><X size={16} /></button
        >
      </header>
      <div class="p-6 flex flex-col gap-3">
        {#if loading}
          <div class="flex items-center gap-2 text-sm text-text-muted" role="status">
            <LoaderCircle size={15} class="animate-spin-slow motion-reduce:animate-none" />
            Loading configuration…
          </div>
        {:else if loadError}
          <p class="m-0 text-sm text-danger-text" role="alert">{loadError}</p>
        {:else if !repoRoot}
          <p class="m-0 text-sm text-text-muted">Open a repository first.</p>
        {:else}
          <button
            type="button"
            class="p-4 rounded-lg border border-border bg-bg-input text-left text-text hover:bg-hover-strong flex gap-3"
            onclick={() => (provider = 'github-actions')}
          >
            <span class="shrink-0 mt-0.5"><TrackerProviderIcon provider="github" size={20} /></span>
            <span>
              <strong class="block text-sm">GitHub Actions</strong>
              <span class="block mt-1 text-xs text-text-muted">
                Run selected workflow_dispatch workflows from the repository origin.
              </span>
            </span>
          </button>
          <button
            type="button"
            class="p-4 rounded-lg border border-border bg-bg-input text-left text-text hover:bg-hover-strong flex gap-3"
            onclick={() => (provider = 'teamcity')}
          >
            <Server size={20} class="shrink-0 mt-0.5" />
            <span>
              <strong class="block text-sm">TeamCity</strong>
              <span class="block mt-1 text-xs text-text-muted">
                Connect a TeamCity server and select build configurations.
              </span>
            </span>
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
