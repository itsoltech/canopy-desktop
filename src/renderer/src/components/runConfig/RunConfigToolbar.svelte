<script lang="ts">
  import { Play, Square, Settings, ChevronDown } from '@lucide/svelte'
  import Tooltip from '../shared/Tooltip.svelte'
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

  let sources = $derived(getSources())
  let selected = $derived(getSelectedConfig())
  let running = $derived(getRunningProcesses())

  let dropdownOpen = $state(false)
  let triggerEl: HTMLButtonElement | undefined = $state()
  let dropdownTop = $state(0)
  let dropdownLeft = $state(0)
  let dropdownWidth = $state(0)

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

  let dropdownGroups = $derived.by(() => {
    const groups: {
      label: string
      items: { configDir: string; name: string }[]
    }[] = []
    for (const source of sources) {
      if (source.file.configurations.length === 0) continue
      const groupLabel = source.relativePath === '.' ? 'Root' : source.relativePath
      groups.push({
        label: groupLabel,
        items: source.file.configurations.map((c) => ({
          configDir: source.configDir,
          name: c.name,
        })),
      })
    }
    return groups
  })

  let activeLabel = $derived.by(() => {
    const target = getActiveTarget()
    return target?.name ?? 'No configurations'
  })

  function getActiveTarget(): { configDir: string; name: string } | null {
    if (selected) return selected
    const values = Object.values(configLookup)
    return values[0] ?? null
  }

  function openDropdown(): void {
    if (!triggerEl) return
    const rect = triggerEl.getBoundingClientRect()
    dropdownTop = rect.bottom + 4
    dropdownLeft = rect.left
    dropdownWidth = rect.width
    dropdownOpen = true
  }

  function closeDropdown(): void {
    dropdownOpen = false
  }

  function selectAndClose(configDir: string, name: string): void {
    selectRunConfig(configDir, name)
    closeDropdown()
  }

  async function runItem(configDir: string, name: string): Promise<void> {
    closeDropdown()
    const result = await executeRunConfig(configDir, name)
    if (result) {
      const worktreePath = workspaceState.selectedWorktreePath
      if (worktreePath) openRunConfigTab(name, result.sessionId, result.wsUrl, worktreePath)
    }
  }

  function openManager(): void {
    closeDropdown()
    showRunConfigManager()
  }

  function portal(node: HTMLElement): { destroy(): void } {
    document.body.appendChild(node)
    return { destroy: () => node.remove() }
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

  function getRunningIdsFor(configDir: string, name: string): string[] {
    const ids: string[] = []
    for (const proc of running.values()) {
      if (proc.configDir === configDir && proc.name === name) ids.push(proc.sessionId)
    }
    return ids
  }

  async function handleStop(): Promise<void> {
    const ids = [...running.keys()]
    for (const id of ids) {
      await window.api.killPty(id)
      running.delete(id)
    }
  }

  async function stopItem(configDir: string, name: string): Promise<void> {
    const ids = getRunningIdsFor(configDir, name)
    for (const id of ids) {
      await window.api.killPty(id)
      running.delete(id)
    }
  }

  let totalRunningCount = $derived(running.size)
  let hasAnyRunning = $derived(totalRunningCount > 0)
</script>

{#if dropdownGroups.length > 0}
  <div class="toolbar">
    <button
      bind:this={triggerEl}
      class="select-trigger"
      onclick={() => (dropdownOpen ? closeDropdown() : openDropdown())}
    >
      <span class="select-label">{activeLabel}</span>
      <ChevronDown size={12} />
    </button>

    {#if hasAnyRunning}
      <Tooltip text={`Stop all (${totalRunningCount})`}>
        <button class="stop-btn" onclick={handleStop}>
          <Square size={10} />
          <span class="count-badge">{totalRunningCount}</span>
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

  {#if dropdownOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="dropdown-overlay" use:portal onclick={closeDropdown}>
      <div
        class="dropdown"
        style="top: {dropdownTop}px; left: {dropdownLeft}px; min-width: {dropdownWidth}px;"
        onclick={(e) => e.stopPropagation()}
      >
        {#each dropdownGroups as group (group.label)}
          <div class="dropdown-group-label">{group.label}</div>
          {#each group.items as item (item.configDir + item.name)}
            {@const itemRunning = getRunningIdsFor(item.configDir, item.name).length}
            <div
              class="dropdown-item"
              class:selected={selected?.configDir === item.configDir &&
                selected?.name === item.name}
              onclick={() => selectAndClose(item.configDir, item.name)}
              onkeydown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault()
                  selectAndClose(item.configDir, item.name)
                }
              }}
              role="option"
              aria-selected={selected?.configDir === item.configDir && selected?.name === item.name}
              tabindex="0"
            >
              <span class="dropdown-item-name">{item.name}</span>
              {#if itemRunning > 0}
                <button
                  class="dropdown-item-stop"
                  title={itemRunning > 1 ? `Stop all (${itemRunning})` : 'Stop'}
                  onclick={(e) => {
                    e.stopPropagation()
                    stopItem(item.configDir, item.name)
                  }}
                >
                  <Square size={10} />
                  {#if itemRunning > 1}
                    <span class="count-badge">{itemRunning}</span>
                  {/if}
                </button>
              {:else}
                <button
                  class="dropdown-item-play"
                  title="Run"
                  onclick={(e) => {
                    e.stopPropagation()
                    runItem(item.configDir, item.name)
                  }}
                >
                  <Play size={12} />
                </button>
              {/if}
            </div>
          {/each}
        {/each}
        <div class="dropdown-separator"></div>
        <button class="dropdown-item edit-item" onclick={openManager}>
          <Settings size={12} />
          <span>Edit Configurations...</span>
        </button>
      </div>
    </div>
  {/if}
{/if}

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    app-region: no-drag;
  }

  .select-trigger {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    height: 24px;
    padding: 0 8px;
    max-width: 180px;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    background: var(--c-bg-secondary);
    color: var(--c-text);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    outline: none;
  }

  .select-trigger:hover {
    background: var(--c-hover);
  }

  .select-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dropdown-overlay {
    position: fixed;
    inset: 0;
    z-index: 9998;
  }

  .dropdown {
    position: fixed;
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
    padding: 4px 0;
    max-height: 60vh;
    overflow-y: auto;
    min-width: 200px;
  }

  .dropdown-group-label {
    padding: 6px 12px 2px;
    font-size: 9px;
    font-weight: 700;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .dropdown-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px 4px 12px;
    cursor: pointer;
    font-size: 12px;
    color: var(--c-text);
  }

  .dropdown-item:hover {
    background: var(--c-hover);
  }

  .dropdown-item.selected {
    background: var(--c-active);
  }

  .dropdown-item-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dropdown-item-play {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    background: none;
    color: var(--c-success-text);
    cursor: pointer;
    border-radius: 4px;
    flex-shrink: 0;
  }

  .dropdown-item-play:hover {
    background: var(--c-hover-strong);
  }

  .dropdown-item-stop {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    background: none;
    color: var(--c-danger-text);
    cursor: pointer;
    border-radius: 4px;
    flex-shrink: 0;
    position: relative;
  }

  .dropdown-item-stop:hover {
    background: var(--c-hover-strong);
  }

  .dropdown-separator {
    height: 1px;
    background: var(--c-border-subtle);
    margin: 4px 0;
  }

  .dropdown-item.edit-item {
    color: var(--c-text-muted);
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
    position: relative;
  }

  .stop-btn:hover {
    background: var(--c-hover);
  }

  .count-badge {
    position: absolute;
    top: -2px;
    right: -2px;
    min-width: 14px;
    height: 14px;
    padding: 0 3px;
    border-radius: 7px;
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
    font-size: 9px;
    font-weight: 700;
    line-height: 14px;
    text-align: center;
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
