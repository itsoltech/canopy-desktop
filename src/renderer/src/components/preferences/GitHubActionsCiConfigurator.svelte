<script lang="ts">
  import { X } from '@lucide/svelte'
  import type { GitHubActionsCiRepoConfigInfo } from '../../lib/ci/types'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'
  import CiCredentialModal from './CiCredentialModal.svelte'
  import GitHubCiCredentialSection from './_partials/GitHubCiCredentialSection.svelte'
  import GitHubCiFooter from './_partials/GitHubCiFooter.svelte'
  import GitHubCiRepositorySection from './_partials/GitHubCiRepositorySection.svelte'
  import GitHubCiWorkflowsSection from './_partials/GitHubCiWorkflowsSection.svelte'
  import {
    createGitHubActionsCiConfiguratorState,
    type InvalidCiConfig,
  } from './githubActionsCiConfiguratorState.svelte'

  let {
    repoRoot,
    initialConfig,
    initialInvalid,
  }: {
    repoRoot: string
    initialConfig: GitHubActionsCiRepoConfigInfo | null
    initialInvalid?: InvalidCiConfig
  } = $props()

  // This modal is recreated for each immutable request; a new request never mutates these props.
  // svelte-ignore state_referenced_locally
  const state = createGitHubActionsCiConfiguratorState({ repoRoot, initialConfig, initialInvalid })
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
    aria-labelledby="github-ci-title"
    tabindex="-1"
    onmousedown={(event) => event.stopPropagation()}
  >
    <header class="px-6 pt-5 pb-3 border-b border-border-subtle flex justify-between gap-3">
      <div class="min-w-0">
        <h2
          id="github-ci-title"
          class="m-0 text-lg font-semibold text-text flex items-center gap-2"
        >
          <TrackerProviderIcon provider="github" size={18} /> CI/CD — GitHub Actions
        </h2>
        <p class="m-0 mt-1 text-xs text-text-muted">
          Repository and workflows are shared via
          <code class="font-mono">.canopy/config.json</code>. The token stays on this machine.
        </p>
      </div>
      <button
        type="button"
        class="size-7 rounded-md border-0 bg-transparent text-text-muted cursor-pointer hover:bg-hover hover:text-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-muted"
        onclick={state.requestClose}
        aria-label="Close"
        aria-disabled={state.saving}
      >
        <X size={16} />
      </button>
    </header>

    <div class="flex-1 overflow-y-auto px-6 py-5 flex flex-col gap-4">
      <GitHubCiRepositorySection {state} />
      <GitHubCiCredentialSection {state} />
      <GitHubCiWorkflowsSection {state} />
    </div>

    <GitHubCiFooter {state} />
  </div>
</div>

{#if state.credentialEditorOpen && state.existingConfig && repoRoot}
  <CiCredentialModal
    {repoRoot}
    config={state.existingConfig}
    onClose={state.closeCredentialEditor}
    onUpdated={state.credentialUpdated}
  />
{/if}
