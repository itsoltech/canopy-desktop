<script lang="ts">
  import { tabsByWorktree, activeTabId, switchTab, closeTab } from '../../lib/stores/tabs.svelte'
  import { allPanes } from '../../lib/stores/splitTree'

  let { worktreePath }: { worktreePath: string } = $props()

  let tabs = $derived(tabsByWorktree[worktreePath] ?? [])
  let currentActiveId = $derived(activeTabId[worktreePath])

  let showOverflow = $state(false)
  let visibleCount = $state(0)
  let containerEl: HTMLDivElement | undefined = $state()

  $effect(() => {
    if (!containerEl) return undefined
    const observer = new ResizeObserver(() => {
      const containerWidth = containerEl!.clientWidth
      const minTabWidth = 80
      const maxVisible = Math.max(1, Math.floor(containerWidth / minTabWidth))
      visibleCount = maxVisible
    })
    observer.observe(containerEl)
    return () => observer.disconnect()
  })

  let visibleTabs = $derived(
    visibleCount > 0 && tabs.length > visibleCount ? tabs.slice(0, visibleCount) : tabs
  )
  let overflowTabs = $derived(
    visibleCount > 0 && tabs.length > visibleCount ? tabs.slice(visibleCount) : []
  )

  function handleMiddleClick(e: MouseEvent, tabId: string): void {
    if (e.button === 1) {
      e.preventDefault()
      closeTab(tabId)
    }
  }
</script>

{#if tabs.length > 0}
  <div class="tab-bar" bind:this={containerEl}>
    <div class="tabs-row">
      {#each visibleTabs as tab (tab.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="tab"
          class:active={tab.id === currentActiveId}
          class:exited={allPanes(tab.rootSplit).some((p) => !p.isRunning)}
          onclick={() => switchTab(tab.id)}
          onauxclick={(e) => handleMiddleClick(e, tab.id)}
          title={tab.name}
        >
          <span class="tab-name">{tab.name}</span>
          <button
            class="tab-close"
            onclick={(e: MouseEvent) => {
              e.stopPropagation()
              closeTab(tab.id)
            }}
            title="Close tab"
          >
            &times;
          </button>
        </div>
      {/each}

      {#if overflowTabs.length > 0}
        <div class="overflow-wrapper">
          <button class="overflow-trigger" onclick={() => (showOverflow = !showOverflow)}>
            &hellip;
          </button>
          {#if showOverflow}
            <div class="overflow-menu">
              {#each overflowTabs as tab (tab.id)}
                <button
                  class="overflow-item"
                  class:active={tab.id === currentActiveId}
                  onclick={() => {
                    switchTab(tab.id)
                    showOverflow = false
                  }}
                >
                  {tab.name}
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .tab-bar {
    height: 32px;
    flex-shrink: 0;
    display: flex;
    align-items: stretch;
    background: rgba(30, 30, 30, 0.6);
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    overflow: hidden;
  }

  .tabs-row {
    display: flex;
    align-items: stretch;
    flex: 1;
    min-width: 0;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 60px;
    max-width: 180px;
    flex: 1 1 120px;
    padding: 0 8px;
    border: none;
    background: transparent;
    color: rgba(255, 255, 255, 0.5);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    border-right: 1px solid rgba(255, 255, 255, 0.04);
    transition:
      background 0.1s,
      color 0.1s;
  }

  .tab:hover {
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.7);
  }

  .tab.active {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.9);
  }

  .tab.exited {
    color: rgba(255, 150, 50, 0.6);
  }

  .tab.exited.active {
    color: rgba(255, 150, 50, 0.9);
  }

  .tab-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    text-align: left;
  }

  .tab-close {
    display: none;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border: none;
    background: none;
    color: rgba(255, 255, 255, 0.4);
    font-size: 14px;
    cursor: pointer;
    border-radius: 3px;
    padding: 0;
    line-height: 1;
    flex-shrink: 0;
  }

  .tab:hover .tab-close {
    display: flex;
  }

  .tab-close:hover {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.8);
  }

  .overflow-wrapper {
    position: relative;
    flex-shrink: 0;
  }

  .overflow-trigger {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 100%;
    border: none;
    background: transparent;
    color: rgba(255, 255, 255, 0.5);
    font-size: 14px;
    cursor: pointer;
  }

  .overflow-trigger:hover {
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.8);
  }

  .overflow-menu {
    position: absolute;
    top: 100%;
    right: 0;
    min-width: 160px;
    background: rgba(40, 40, 40, 0.95);
    backdrop-filter: blur(12px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    padding: 4px;
    z-index: 100;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }

  .overflow-item {
    display: block;
    width: 100%;
    padding: 6px 10px;
    border: none;
    background: none;
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    font-family: inherit;
    text-align: left;
    cursor: pointer;
    border-radius: 4px;
  }

  .overflow-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .overflow-item.active {
    color: rgba(255, 255, 255, 0.95);
    background: rgba(255, 255, 255, 0.06);
  }
</style>
