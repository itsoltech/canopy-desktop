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
    executeBackground,
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

  async function handlePlayBackground(configDir: string, name: string): Promise<void> {
    await executeBackground(configDir, name)
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

  function getRunningSessionId(name: string): string | null {
    for (const proc of running.values()) {
      if (proc.name === name) return proc.sessionId
    }
    return null
  }

  async function handleStop(name: string): Promise<void> {
    const sessionId = getRunningSessionId(name)
    if (sessionId) {
      await window.api.killPty(sessionId)
      running.delete(sessionId)
    }
  }
</script>

<CollapsibleSection title="RUN" sectionKey="runConfigs" borderTop>
  {#snippet headerExtra()}
    <button class="header-btn" title="Add configuration" onclick={() => showRunConfigManager()}>
      <Plus size={14} />
    </button>
  {/snippet}

  <ul class="config-list">
    {#each [...grouped.entries()] as [relativePath, group] (relativePath)}
      {#if relativePath !== '.'}
        <li class="group-header">{relativePath}</li>
      {/if}
      {#each group.configurations as config (config.name)}
        {@const runningId = getRunningSessionId(config.name)}
        <li class="config-item">
          <span
            class="config-name"
            title={`${config.command} ${config.args ?? ''}`}
            onclick={() => showRunConfigManager(group.configDir, config.name)}
            role="button"
            tabindex="-1"
            onkeydown={() => {}}
          >
            {config.name}
          </span>
          <div class="config-actions">
            {#if runningId}
              <Tooltip text="Stop">
                <button class="config-action stop" onclick={() => handleStop(config.name)}>
                  <Square size={12} />
                </button>
              </Tooltip>
            {:else}
              <Tooltip text="Run">
                <button
                  class="config-action play"
                  onclick={() => handlePlay(group.configDir, config.name)}
                >
                  <Play size={14} />
                </button>
              </Tooltip>
              <Tooltip text="Run in background">
                <button
                  class="config-action play-bg"
                  onclick={() => handlePlayBackground(group.configDir, config.name)}
                >
                  <Play size={12} />
                  <span class="bg-dot"></span>
                </button>
              </Tooltip>
            {/if}
            <Tooltip text="Delete">
              <button
                class="config-action danger"
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
      <li class="empty">No run configurations found</li>
    {/if}
  </ul>
</CollapsibleSection>

<style>
  .config-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .group-header {
    padding: 4px 12px 2px;
    font-size: 10px;
    font-weight: 600;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .config-item {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 28px;
    padding: 0 8px 0 12px;
  }

  .config-item:hover {
    background: var(--c-hover);
  }

  .config-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--c-text);
    cursor: pointer;
  }

  .config-name:hover {
    text-decoration: underline;
  }

  .config-actions {
    display: flex;
    gap: 1px;
    flex-shrink: 0;
    opacity: 0;
  }

  .config-item:hover .config-actions {
    opacity: 1;
  }

  .config-action {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
    border-radius: 4px;
    flex-shrink: 0;
    position: relative;
  }

  .config-action:hover {
    background: var(--c-hover-strong);
  }

  .config-action.play {
    color: var(--c-success-text);
  }

  .config-action.play-bg {
    color: var(--c-text-muted);
  }

  .config-action.play-bg:hover {
    color: var(--c-success-text);
  }

  .config-action.stop {
    color: var(--c-danger-text);
  }

  .config-action.danger:hover {
    color: var(--c-danger-text);
  }

  .bg-dot {
    position: absolute;
    bottom: 2px;
    right: 2px;
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: currentColor;
  }

  .header-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
    border-radius: 4px;
  }

  .header-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .empty {
    padding: 8px 12px;
    font-size: 11px;
    color: var(--c-text-faint);
  }

  :global(.running-indicator) {
    color: var(--c-success-text);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }
</style>
