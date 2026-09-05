<script lang="ts">
  import { onMount } from 'svelte'
  import { X } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import {
    loadGlobalConfig,
    loadRepoConfig,
    getRepoConfig,
    getRepoConfigLoadError,
    initRepoConfig,
  } from '../../lib/stores/taskTracker.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { buildAgentSetupPrompt } from '../../lib/taskTracker/agentPrompt'
  import ProjectConnections from './ProjectConnections.svelte'
  import ProjectNamingSection from './ProjectNamingSection.svelte'

  // Project-scoped tracker config for the ACTIVE worktree — kept separate from Canopy Settings.
  let repoRoot = $derived(workspaceState.selectedWorktreePath ?? workspaceState.repoRoot)
  let repoName = $derived(
    repoRoot ? (repoRoot.split(/[\\/]/).filter(Boolean).pop() ?? repoRoot) : '',
  )
  let repoCfg = $derived(getRepoConfig())
  let repoConfigLoadError = $derived(getRepoConfigLoadError())

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
      return
    }
    // Trap focus inside the dialog (same pattern as PreferencesModal).
    if (e.key === 'Tab' && containerEl) {
      const focusable = containerEl.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      const active = document.activeElement as HTMLElement | null
      if (e.shiftKey && (active === first || !containerEl.contains(active))) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && active === last) {
        e.preventDefault()
        first.focus()
      }
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

  async function copyAgentPrompt(): Promise<void> {
    try {
      await navigator.clipboard.writeText(buildAgentSetupPrompt())
      addToast('Agent prompt copied — paste it into your agent session')
    } catch {
      addToast('Failed to copy the agent prompt')
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
          <p class="text-xs text-text-muted m-0 leading-snug">
            This configuration is shared with your team via
            <code class="font-mono">.canopy/config.json</code> in this repository.
          </p>
          <p class="text-xs text-text-muted m-0 leading-snug truncate" title={repoRoot}>
            Active branch:
            <span class="text-text-secondary font-mono">{workspaceState.branch ?? repoName}</span>
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
      {:else if repoConfigLoadError}
        <section
          class="rounded-lg border border-danger-border bg-danger-bg p-4 flex flex-col gap-2"
          role="alert"
        >
          <h3 class="m-0 text-sm font-semibold text-danger-text">Could not load project config</h3>
          <p class="m-0 text-xs text-text-muted leading-snug">{repoConfigLoadError}</p>
          <p class="m-0 text-xs text-text-muted leading-snug">
            The existing <code class="font-mono">.canopy/config.json</code> was not changed. Fix it by
            hand before editing project tracker settings.
          </p>
        </section>
      {:else}
        <!-- Connections first — you connect before anything else makes sense. -->
        <section class="rounded-lg border border-border-subtle px-4 pb-4 pt-2.5">
          <ProjectConnections />
        </section>

        {#if repoCfg}
          <ProjectNamingSection {repoRoot} />

          <!-- AI agents: hand the naming conventions to coding agents without duplicating them —
               the generated prompt references .canopy/config.json as the source of truth. -->
          <section class="rounded-lg border border-border-subtle p-4 flex flex-col gap-2">
            <h3 class="m-0 text-sm font-semibold text-text">AI agents</h3>
            <p class="m-0 text-xs text-text-muted leading-snug">
              Generate a prompt that teaches a coding agent (Claude Code, Codex, …) to follow this
              project's branch and PR conventions. The agent stores rules that point at
              <code class="font-mono">.canopy/config.json</code>, so later template changes apply
              without regenerating, and it is explicitly forbidden from editing Canopy config files.
            </p>
            <div>
              <button
                type="button"
                class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
                onclick={copyAgentPrompt}
              >
                Copy agent prompt
              </button>
            </div>
          </section>
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
