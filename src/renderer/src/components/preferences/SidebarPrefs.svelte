<script lang="ts">
  import { tick } from 'svelte'
  import { ChevronUp, ChevronDown } from '@lucide/svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import {
    SECTION_DEFS,
    getSidebarConfig,
    saveSidebarConfig,
    type SidebarSectionConfig,
  } from '../../lib/stores/sidebarSections.svelte'
  import { getPref } from '../../lib/stores/preferences.svelte'
  import { prefsSearch, matches } from './_partials/prefsSearch.svelte'

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

<div class="flex flex-col gap-7">
  <PrefsSection
    title="Sections"
    description="Pick which sections appear in the sidebar and reorder them"
  >
    <div class="flex flex-col">
      {#each config as item, i (item.id)}
        {@const def = SECTION_DEFS.find((d) => d.id === item.id)}
        {@const dim = prefsSearch.query.trim() !== '' && !matches(def?.label ?? item.id)}
        <div
          class="flex items-center justify-between gap-3 py-2 border-t border-border-subtle first:border-t-0 first:pt-0 transition-opacity duration-fast"
          class:opacity-30={dim}
        >
          <label class="flex items-center gap-2 text-md text-text cursor-pointer min-w-0 flex-1">
            <CustomCheckbox
              checked={item.visible}
              disabled={def?.forced}
              onchange={() => toggleVisibility(i)}
            />
            <span class="truncate" class:text-text-secondary={def?.forced}
              >{def?.label ?? item.id}</span
            >
            {#if def?.forced}
              <span class="text-2xs uppercase tracking-caps-tight text-text-faint shrink-0 ml-1"
                >Always shown</span
              >
            {/if}
          </label>
          <div class="flex gap-0.5 shrink-0">
            <button
              type="button"
              class="flex items-center justify-center size-6 bg-transparent border-0 rounded-md text-text-secondary cursor-pointer enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-20 disabled:cursor-default"
              data-order-up={i}
              disabled={i === 0}
              onclick={() => moveUp(i)}
              aria-label="Move {def?.label ?? item.id} up"
            >
              <ChevronUp size={14} />
            </button>
            <button
              type="button"
              class="flex items-center justify-center size-6 bg-transparent border-0 rounded-md text-text-secondary cursor-pointer enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-20 disabled:cursor-default"
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
  </PrefsSection>
</div>
