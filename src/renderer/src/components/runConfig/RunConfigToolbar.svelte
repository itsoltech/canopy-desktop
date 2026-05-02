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

  const iconBtnCls =
    'flex items-center justify-center w-6 h-6 border-0 bg-transparent cursor-pointer rounded-md relative hover:bg-hover'
  const countBadgeCls =
    'absolute -top-0.5 -right-0.5 min-w-[14px] h-[14px] px-[3px] rounded-[7px] bg-accent-bg text-accent-text text-[9px] font-bold leading-[14px] text-center'
</script>

{#if dropdownGroups.length > 0}
  <div class="flex items-center gap-1 [app-region:no-drag]">
    <button
      bind:this={triggerEl}
      class="inline-flex items-center justify-between gap-1.5 h-6 px-2 max-w-[180px] border border-border rounded-md bg-bg-secondary text-text text-xs font-inherit cursor-pointer outline-none hover:bg-hover"
      onclick={() => (dropdownOpen ? closeDropdown() : openDropdown())}
    >
      <span class="overflow-hidden text-ellipsis whitespace-nowrap">{activeLabel}</span>
      <ChevronDown size={12} />
    </button>

    {#if hasAnyRunning}
      <Tooltip text={`Stop all (${totalRunningCount})`}>
        <button
          class="{iconBtnCls} text-danger-text"
          onclick={handleStop}
          aria-label={`Stop all (${totalRunningCount})`}
        >
          <Square size={10} />
          <span class={countBadgeCls}>{totalRunningCount}</span>
        </button>
      </Tooltip>
    {:else}
      <Tooltip text="Run">
        <button class="{iconBtnCls} text-success-text" onclick={handlePlay} aria-label="Run">
          <Play size={12} />
        </button>
      </Tooltip>
    {/if}

    <Tooltip text="Manage configurations">
      <button
        class="{iconBtnCls} text-text-muted hover:text-text"
        onclick={() => {
          const target = getActiveTarget()
          showRunConfigManager(target?.configDir, target?.name)
        }}
        aria-label="Manage configurations"
      >
        <Settings size={12} />
      </button>
    </Tooltip>
  </div>

  {#if dropdownOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="fixed inset-0 z-[9998]" use:portal onclick={closeDropdown}>
      <div
        class="fixed bg-bg border border-border rounded-lg shadow-[0_8px_24px_oklch(0_0_0/0.3)] py-1 max-h-[60vh] overflow-y-auto min-w-[200px]"
        style="top: {dropdownTop}px; left: {dropdownLeft}px; min-width: {dropdownWidth}px;"
        onclick={(e) => e.stopPropagation()}
        role="listbox"
        aria-label="Run configurations"
        tabindex={-1}
      >
        {#each dropdownGroups as group (group.label)}
          <div
            class="px-3 pt-1.5 pb-0.5 text-[9px] font-bold text-text-muted uppercase tracking-[0.5px]"
          >
            {group.label}
          </div>
          {#each group.items as item (item.configDir + item.name)}
            {@const itemRunning = getRunningIdsFor(item.configDir, item.name).length}
            <div
              class="flex items-center gap-1.5 pl-3 pr-2 py-1 cursor-pointer text-sm text-text hover:bg-hover"
              class:!bg-active={selected?.configDir === item.configDir &&
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
              <span class="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{item.name}</span
              >
              {#if itemRunning > 0}
                <button
                  class="flex items-center justify-center w-[22px] h-[22px] border-0 bg-transparent text-danger-text cursor-pointer rounded-md flex-shrink-0 relative hover:bg-hover-strong"
                  title={itemRunning > 1 ? `Stop all (${itemRunning})` : 'Stop'}
                  aria-label={itemRunning > 1 ? `Stop all (${itemRunning})` : 'Stop'}
                  onclick={(e) => {
                    e.stopPropagation()
                    stopItem(item.configDir, item.name)
                  }}
                >
                  <Square size={10} />
                  {#if itemRunning > 1}
                    <span class={countBadgeCls}>{itemRunning}</span>
                  {/if}
                </button>
              {:else}
                <button
                  class="flex items-center justify-center w-[22px] h-[22px] border-0 bg-transparent text-success-text cursor-pointer rounded-md flex-shrink-0 hover:bg-hover-strong"
                  title="Run"
                  aria-label="Run {item.name}"
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
        <div class="h-px bg-border-subtle my-1"></div>
        <button
          class="flex items-center gap-1.5 pl-3 pr-2 py-1 cursor-pointer text-sm text-text-muted w-full text-left bg-transparent border-0 hover:bg-hover"
          onclick={openManager}
        >
          <Settings size={12} />
          <span>Edit Configurations...</span>
        </button>
      </div>
    </div>
  {/if}
{/if}
