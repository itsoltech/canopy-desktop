<script lang="ts">
  import { Play, Square, Settings } from '@lucide/svelte'
  import Tooltip from '../shared/Tooltip.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import {
    getSources,
    getSelectedConfig,
    getRunningProcesses,
    selectRunConfig,
    executeRunConfig,
  } from '../../lib/stores/runConfig.svelte'
  import { showRunConfigManager } from '../../lib/stores/dialogs.svelte'
  import { openRunConfigTab } from '../../lib/stores/tabs.svelte'

  const EDIT_VALUE = '__edit__'

  let sources = $derived(getSources())
  let selected = $derived(getSelectedConfig())
  let running = $derived(getRunningProcesses())

  let configLookup = $derived.by(() => {
    const lookup: Record<string, { configDir: string; name: string }> = {}
    for (const source of sources) {
      for (const config of source.file.configurations) {
        lookup[`${source.configDir}::${config.name}`] = {
          configDir: source.configDir,
          name: config.name,
        }
      }
    }
    return lookup
  })

  let selectGroups = $derived.by(() => {
    const groups: { label: string; options: { value: string; label: string }[] }[] = []
    for (const source of sources) {
      const groupLabel = source.relativePath === '.' ? 'Root' : source.relativePath
      const options = source.file.configurations.map((config) => ({
        value: `${source.configDir}::${config.name}`,
        label: config.name,
      }))
      if (options.length > 0) groups.push({ label: groupLabel, options })
    }
    groups.push({
      label: '',
      options: [{ value: EDIT_VALUE, label: 'Edit Configurations...' }],
    })
    return groups
  })

  let selectedValue = $derived.by(() => {
    if (selected) return `${selected.configDir}::${selected.name}`
    const keys = Object.keys(configLookup)
    return keys[0] ?? ''
  })

  function handleSelectChange(value: string): void {
    if (value === EDIT_VALUE) {
      showRunConfigManager()
      return
    }
    const item = configLookup[value]
    if (item) selectRunConfig(item.configDir, item.name)
  }

  function getActiveTarget(): { configDir: string; name: string } | null {
    if (selected) return selected
    const values = Object.values(configLookup)
    return values[0] ?? null
  }

  async function handlePlay(): Promise<void> {
    const target = getActiveTarget()
    if (!target) return
    const result = await executeRunConfig(target.configDir, target.name)
    if (result) {
      const worktreePath = workspaceState.selectedWorktreePath
      if (worktreePath) {
        openRunConfigTab(target.name, result.sessionId, result.wsUrl, worktreePath)
      }
    }
  }

  function getSelectedRunningId(): string | null {
    const target = getActiveTarget()
    if (!target) return null
    for (const proc of running.values()) {
      if (proc.name === target.name) return proc.sessionId
    }
    return null
  }

  async function handleStop(): Promise<void> {
    const sessionId = getSelectedRunningId()
    if (sessionId) {
      await window.api.killPty(sessionId)
      running.delete(sessionId)
    }
  }

  let isSelectedRunning = $derived(!!getSelectedRunningId())
</script>

{#if selectGroups.length > 1}
  <div class="toolbar">
    <CustomSelect
      value={selectedValue}
      groups={selectGroups}
      onchange={handleSelectChange}
      maxWidth="180px"
    />

    {#if isSelectedRunning}
      <Tooltip text="Stop">
        <button class="stop-btn" onclick={handleStop}>
          <Square size={10} />
        </button>
      </Tooltip>
    {:else}
      <Tooltip text="Run">
        <button class="play-btn" onclick={handlePlay}>
          <Play size={12} />
        </button>
      </Tooltip>
    {/if}

    <Tooltip text="Manage configurations">
      <button
        class="settings-btn"
        onclick={() => {
          const target = getActiveTarget()
          showRunConfigManager(target?.configDir, target?.name)
        }}
      >
        <Settings size={12} />
      </button>
    </Tooltip>
  </div>
{/if}

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    app-region: no-drag;
  }

  .play-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    background: none;
    color: var(--c-success-text);
    cursor: pointer;
    border-radius: 4px;
    position: relative;
  }

  .play-btn:hover {
    background: var(--c-hover);
  }

  .stop-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    background: none;
    color: var(--c-danger-text);
    cursor: pointer;
    border-radius: 4px;
  }

  .stop-btn:hover {
    background: var(--c-hover);
  }

  .settings-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
    border-radius: 4px;
  }

  .settings-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }
</style>
