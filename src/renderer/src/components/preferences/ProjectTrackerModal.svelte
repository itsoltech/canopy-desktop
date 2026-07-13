<script lang="ts">
  import { onMount } from 'svelte'
  import { X } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import {
    loadGlobalConfig,
    loadRepoConfig,
    getRepoConfig,
    initRepoConfig,
  } from '../../lib/stores/taskTracker.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import ProjectConnections from './ProjectConnections.svelte'
  import ProjectNamingSection from './ProjectNamingSection.svelte'

  // Project-scoped tracker config for the ACTIVE worktree — kept separate from Canopy Settings.
  let repoRoot = $derived(workspaceState.selectedWorktreePath ?? workspaceState.repoRoot)
  let repoName = $derived(
    repoRoot ? (repoRoot.split(/[\\/]/).filter(Boolean).pop() ?? repoRoot) : '',
  )
  let repoCfg = $derived(getRepoConfig())

  let containerEl: HTMLElement | undefined = $state()

  onMount(async () => {
    await loadGlobalConfig()
    if (repoRoot) await loadRepoConfig(repoRoot)
    containerEl?.focus()
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      closeDialog()
    }
  }

  async function handleInit(): Promise<void> {
    if (!repoRoot) return
    try {
      await initRepoConfig(repoRoot)
      addToast('Project tracker configuration initialized')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to initialize config')
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex justify-center items-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={closeDialog}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="outline-none w-[680px] max-w-[92vw] max-h-[85vh] flex flex-col bg-bg-overlay border border-border rounded-xl shadow-modal overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-label="Project tracker configuration"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <header
      class="px-6 pt-5 pb-3 border-b border-border-subtle shrink-0 flex items-start justify-between gap-3"
    >
      <div class="flex flex-col gap-0.5 min-w-0">
        <h2 class="text-lg font-semibold text-text m-0 leading-tight">Project tracker</h2>
        {#if repoRoot}
          <p class="text-xs text-text-muted m-0 leading-snug truncate" title={repoRoot}>
            Active worktree: <span class="text-text-secondary">{repoName}</span> — shared with your
            team via <code class="font-mono">.canopy/config.json</code> (git).
          </p>
        {/if}
      </div>
      <button
        type="button"
        class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0"
        onclick={closeDialog}
        aria-label="Close"
        title="Close"
      >
        <X size={16} />
      </button>
    </header>

    <div class="flex-1 overflow-y-auto px-6 py-5 flex flex-col gap-5">
      {#if !repoRoot}
        <p class="text-sm text-text-faint m-0">Open a repository to configure its tracker.</p>
      {:else}
        <!-- Connections first — you connect before anything else makes sense. -->
        <section class="rounded-lg border border-border-subtle px-4 pb-4 pt-2.5">
          <ProjectConnections />
        </section>

        {#if repoCfg}
          <ProjectNamingSection {repoRoot} />
        {:else}
          <section class="rounded-lg border border-border-subtle p-4">
            <div class="flex flex-col gap-2 items-start">
              <p class="text-sm text-text-faint m-0">
                No <code class="font-mono text-text-secondary">.canopy/config.json</code> in this worktree
                — initialize it to define project branch/PR templates.
              </p>
              <button
                type="button"
                class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
                onclick={handleInit}>Initialize project config</button
              >
            </div>
          </section>
        {/if}
      {/if}
    </div>
  </div>
</div>
