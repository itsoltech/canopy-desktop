<script lang="ts">
  import { tick } from 'svelte'
  import { ChevronUp, ChevronDown } from '@lucide/svelte'
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

<div class="section">
  <h3 class="section-title">Sidebar</h3>
  <p class="section-desc">Choose which sections appear in the sidebar and their order.</p>

  <div class="section-list">
    {#each config as item, i (item.id)}
      {@const def = SECTION_DEFS.find((d) => d.id === item.id)}
      <div class="section-row">
        <label class="checkbox-row">
          <input
            type="checkbox"
            checked={item.visible}
            disabled={def?.forced}
            onchange={() => toggleVisibility(i)}
          />
          <span class:forced={def?.forced}>{def?.label ?? item.id}</span>
          {#if def?.forced}
            <span class="hint">Always visible</span>
          {/if}
        </label>
        <div class="order-buttons">
          <button
            class="order-btn"
            data-order-up={i}
            disabled={i === 0}
            onclick={() => moveUp(i)}
            aria-label="Move {def?.label ?? item.id} up"
          >
            <ChevronUp size={14} />
          </button>
          <button
            class="order-btn"
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

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0;
  }

  .section-desc {
    font-size: 12px;
    color: var(--c-text-secondary);
    margin: 0;
  }

  .section-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .section-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px;
    border-radius: 6px;
    transition: background 0.1s;
  }

  .section-row:hover {
    background: var(--c-hover);
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--c-text);
    cursor: pointer;
  }

  .checkbox-row input[type='checkbox'] {
    accent-color: var(--c-accent);
  }

  .checkbox-row input[type='checkbox']:disabled {
    opacity: 0.4;
  }

  .forced {
    color: var(--c-text-secondary);
  }

  .hint {
    font-size: 11px;
    color: var(--c-text-faint);
    margin-left: 4px;
  }

  .order-buttons {
    display: flex;
    gap: 2px;
  }

  .order-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: none;
    border: 1px solid transparent;
    border-radius: 4px;
    color: var(--c-text-secondary);
    cursor: pointer;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .order-btn:hover:not(:disabled) {
    background: var(--c-hover-strong);
    color: var(--c-text);
  }

  .order-btn:disabled {
    opacity: 0.2;
    cursor: default;
  }
</style>
