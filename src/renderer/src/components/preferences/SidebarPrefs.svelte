<script lang="ts">
  import { tick } from 'svelte'
  import { ChevronUp, ChevronDown } from '@lucide/svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import {
    SECTION_DEFS,
    getSidebarConfig,
    saveSidebarConfig,
    type SidebarSectionConfig,
  } from '../../lib/stores/sidebarSections.svelte'
  import { getPref } from '../../lib/stores/preferences.svelte'

  let config: SidebarSectionConfig[] = $state(getSidebarConfig(getPref('sidebar.sections', '')))

  function toggleVisibility(index: number): void {
    const def = SECTION_DEFS.find((d) => d.id === config[index].id)
    if (def?.forced) return
    config[index] = { ...config[index], visible: !config[index].visible }
    saveSidebarConfig(config)
  }

  async function moveUp(index: number): Promise<void> {
    if (index === 0) return
    const item = config[index]
    config[index] = config[index - 1]
    config[index - 1] = item
    config = [...config]
    saveSidebarConfig(config)
    await tick()
    const btn = document.querySelector(`[data-order-up="${index - 1}"]`) as HTMLButtonElement | null
    btn?.focus()
  }

  async function moveDown(index: number): Promise<void> {
    if (index === config.length - 1) return
    const item = config[index]
    config[index] = config[index + 1]
    config[index + 1] = item
    config = [...config]
    saveSidebarConfig(config)
    await tick()
    const btn = document.querySelector(
      `[data-order-down="${index + 1}"]`,
    ) as HTMLButtonElement | null
    btn?.focus()
  }
</script>

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">Sidebar</h3>
  <p class="text-sm text-text-secondary m-0">
    Choose which sections appear in the sidebar and their order.
  </p>

  <div class="flex flex-col gap-0.5">
    {#each config as item, i (item.id)}
      {@const def = SECTION_DEFS.find((d) => d.id === item.id)}
      <div
        class="flex items-center justify-between px-2 py-1.5 rounded-lg transition-colors duration-fast hover:bg-hover"
      >
        <label class="flex items-center gap-2 text-md text-text cursor-pointer">
          <CustomCheckbox
            checked={item.visible}
            disabled={def?.forced}
            onchange={() => toggleVisibility(i)}
          />
          <span class:text-text-secondary={def?.forced}>{def?.label ?? item.id}</span>
          {#if def?.forced}
            <span class="text-xs text-text-faint ml-1">Always visible</span>
          {/if}
        </label>
        <div class="flex gap-0.5">
          <button
            class="flex items-center justify-center w-6 h-6 bg-transparent border border-transparent rounded-md text-text-secondary cursor-pointer transition-colors duration-fast enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-20 disabled:cursor-default"
            data-order-up={i}
            disabled={i === 0}
            onclick={() => moveUp(i)}
            aria-label="Move {def?.label ?? item.id} up"
          >
            <ChevronUp size={14} />
          </button>
          <button
            class="flex items-center justify-center w-6 h-6 bg-transparent border border-transparent rounded-md text-text-secondary cursor-pointer transition-colors duration-fast enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-20 disabled:cursor-default"
            data-order-down={i}
            disabled={i === config.length - 1}
            onclick={() => moveDown(i)}
            aria-label="Move {def?.label ?? item.id} down"
          >
            <ChevronDown size={14} />
          </button>
        </div>
      </div>
    {/each}
  </div>
</div>
