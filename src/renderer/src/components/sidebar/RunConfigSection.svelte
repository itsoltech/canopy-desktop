<script lang="ts">
  import { onMount } from 'svelte'
  import { Play, Plus, Square, X } from '@lucide/svelte'
  import Tooltip from '../shared/Tooltip.svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import {
    discoverConfigs,
    getGroupedConfigs,
    getRunningProcesses,
    executeRunConfig,
    deleteRunConfig,
    initBackgroundListener,
    cleanupBackgroundListener,
  } from '../../lib/stores/runConfig.svelte'
  import { showRunConfigManager } from '../../lib/stores/dialogs.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import { openRunConfigTab } from '../../lib/stores/tabs.svelte'

  let grouped = $derived(getGroupedConfigs())
  let running = $derived(getRunningProcesses())

  let repoRoot = $derived(workspaceState.repoRoot)

  $effect(() => {
    if (repoRoot) discoverConfigs()
  })

  onMount(() => {
    initBackgroundListener()
    return () => cleanupBackgroundListener()
  })

  async function handlePlay(configDir: string, name: string): Promise<void> {
    const result = await executeRunConfig(configDir, name)
    if (result) {
      const worktreePath = workspaceState.selectedWorktreePath
      if (worktreePath) {
        openRunConfigTab(name, result.sessionId, result.wsUrl, worktreePath)
      }
    }
  }

  async function handleDelete(configDir: string, name: string): Promise<void> {
    const confirmed = await confirm({
      title: 'Delete Configuration',
      message: `Delete "${name}"?`,
      confirmLabel: 'Delete',
      destructive: true,
    })
    if (confirmed) {
      await deleteRunConfig(configDir, name)
    }
  }

  function getRunningSessionIds(configDir: string, name: string): string[] {
    const ids: string[] = []
    for (const proc of running.values()) {
      if (proc.configDir === configDir && proc.name === name) ids.push(proc.sessionId)
    }
    return ids
  }

  async function handleStop(configDir: string, name: string): Promise<void> {
    const ids = getRunningSessionIds(configDir, name)
    for (const id of ids) {
      await window.api.killPty(id)
      running.delete(id)
    }
  }
</script>

<CollapsibleSection title="RUN" sectionKey="runConfigs" borderTop>
  {#snippet headerExtra()}
    <button
      class="flex items-center justify-center w-6 h-6 -my-1 border-0 bg-transparent text-text-muted cursor-pointer rounded-md transition-colors duration-fast hover:bg-hover hover:text-text"
      title="Add configuration"
      onclick={() => showRunConfigManager()}
    >
      <Plus size={14} />
    </button>
  {/snippet}

  <ul class="list-none p-0 m-0">
    {#each [...grouped.entries()] as [relativePath, group] (relativePath)}
      {#if relativePath !== '.'}
        <li
          class="px-3 pt-1 pb-0.5 text-2xs font-semibold text-text-muted uppercase tracking-caps-tight"
        >
          {relativePath}
        </li>
      {/if}
      {#each group.configurations as config (config.name)}
        {@const runningCount = getRunningSessionIds(group.configDir, config.name).length}
        <li class="group flex items-center gap-1 h-7 pl-3 pr-2 hover:bg-hover">
          <button
            class="flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-sm font-inherit text-text cursor-pointer bg-transparent border-0 p-0 text-left hover:underline"
            title={`${config.command} ${config.args ?? ''}`}
            onclick={() => showRunConfigManager(group.configDir, config.name)}
          >
            {config.name}
          </button>
          <div class="flex gap-px flex-shrink-0 opacity-0 group-hover:opacity-100">
            {#if runningCount > 0}
              <Tooltip text={runningCount > 1 ? `Stop all (${runningCount})` : 'Stop'}>
                <button
                  class="relative flex items-center justify-center w-5.5 h-5.5 border-0 bg-transparent text-danger-text cursor-pointer rounded-md flex-shrink-0 hover:bg-hover-strong"
                  aria-label={runningCount > 1
                    ? `Stop all ${runningCount} sessions for ${config.name}`
                    : `Stop ${config.name}`}
                  onclick={() => handleStop(group.configDir, config.name)}
                >
                  <Square size={12} />
                  {#if runningCount > 1}
                    <span
                      class="absolute -top-0.5 -right-0.5 min-w-3.5 h-3.5 px-px rounded-2xl bg-accent-bg text-accent-text text-micro font-bold leading-3.5 text-center"
                      >{runningCount}</span
                    >
                  {/if}
                </button>
              </Tooltip>
            {:else}
              <Tooltip text="Run">
                <button
                  class="flex items-center justify-center w-5.5 h-5.5 border-0 bg-transparent text-success-text cursor-pointer rounded-md flex-shrink-0 hover:bg-hover-strong"
                  aria-label={`Run ${config.name}`}
                  onclick={() => handlePlay(group.configDir, config.name)}
                >
                  <Play size={14} />
                </button>
              </Tooltip>
            {/if}
            <Tooltip text="Delete">
              <button
                class="flex items-center justify-center w-5.5 h-5.5 border-0 bg-transparent text-text-muted cursor-pointer rounded-md flex-shrink-0 hover:bg-hover-strong hover:text-danger-text"
                aria-label={`Delete ${config.name}`}
                onclick={() => handleDelete(group.configDir, config.name)}
              >
                <X size={14} />
              </button>
            </Tooltip>
          </div>
        </li>
      {/each}
    {/each}
    {#if grouped.size === 0}
      <li class="px-3 py-2 text-xs text-text-faint">No run configurations found</li>
    {/if}
  </ul>
</CollapsibleSection>
