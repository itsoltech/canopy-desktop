<script lang="ts">
  import { X } from '@lucide/svelte'
  import type { CiCredentialStatus, TeamCityCiRepoConfigInfo } from '../../lib/ci/types'
  import CiCredentialModal from './CiCredentialModal.svelte'
  import ProjectCiCredentialSection from './_partials/ProjectCiCredentialSection.svelte'
  import ProjectCiFooter from './_partials/ProjectCiFooter.svelte'
  import ProjectCiJobsSection from './_partials/ProjectCiJobsSection.svelte'
  import ProjectCiServerSection from './_partials/ProjectCiServerSection.svelte'
  import { createProjectCiModalState, type InvalidCiConfig } from './projectCiModalState.svelte'

  let {
    repoRoot,
    initialConfig,
    initialCredential,
    initialInvalid,
  }: {
    repoRoot: string
    initialConfig: TeamCityCiRepoConfigInfo | null
    initialCredential?: CiCredentialStatus
    initialInvalid?: InvalidCiConfig
  } = $props()

  // This modal is recreated for each immutable request; a new request never mutates these props.
  // svelte-ignore state_referenced_locally
  const state = createProjectCiModalState({
    repoRoot,
    initialConfig,
    initialCredential,
    initialInvalid,
  })
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex justify-center items-center bg-scrim"
  onkeydown={state.handleKeydown}
  onmousedown={state.requestClose}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={state.containerEl}
    class="outline-none w-[620px] max-w-[92vw] max-h-[85vh] flex flex-col bg-bg-overlay border border-border rounded-xl shadow-modal overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-label="CI/CD configuration"
    tabindex="-1"
    onmousedown={(event) => event.stopPropagation()}
  >
    <header
      class="px-6 pt-5 pb-3 border-b border-border-subtle shrink-0 flex items-start justify-between gap-3"
    >
      <div class="flex flex-col gap-0.5 min-w-0">
        <h2 class="text-lg font-semibold text-text m-0 leading-tight">CI/CD - TeamCity</h2>
        <p class="text-xs text-text-muted m-0 leading-snug">
          The server and the available build configurations are shared with your team via
          <code class="font-mono">.canopy/config.json</code> in this repository. Tokens stay on this machine.
        </p>
      </div>
      <button
        type="button"
        class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0 aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-muted"
        onclick={state.requestClose}
        aria-disabled={state.busy !== ''}
        aria-label="Close"
        title={state.busy !== ''
          ? 'Disabled while an update is writing .canopy/config.json'
          : 'Close'}
      >
        <X size={16} />
      </button>
    </header>

    <div class="flex-1 overflow-y-auto px-6 py-5 flex flex-col gap-4">
      {#if !repoRoot}
        <p class="text-sm text-text-faint m-0">Open a repository first.</p>
      {:else}
        <div role="status" class:sr-only={!state.configLoadError}>
          {#if state.configLoadError}
            <p class="m-0 text-xs text-warning-text leading-snug" title={state.configLoadError}>
              {state.configLoadError}
              {#if state.configLoadScope === 'file'}
                - fix <code class="font-mono">.canopy/config.json</code> by hand; Save is disabled here
                because writing would require reading the file first (nothing is ever re-initialized over
                it).
              {:else if state.configLoadScope === 'block'}
                - pick the server and jobs below and Save to replace the invalid
                <code class="font-mono">ci</code> block - the rest of the file is untouched.
              {/if}
            </p>
          {/if}
        </div>
        <ProjectCiServerSection {state} />
        <ProjectCiCredentialSection {state} />
        <ProjectCiJobsSection {state} />
      {/if}
    </div>

    <ProjectCiFooter {state} />
  </div>
</div>

{#if state.credentialEditorOpen && state.existingConfig && repoRoot && state.urlValid}
  <CiCredentialModal
    {repoRoot}
    config={{ ...state.existingConfig, baseUrl: state.effectiveUrl }}
    onClose={state.closeCredentialEditor}
    onUpdated={state.credentialUpdated}
  />
{/if}
