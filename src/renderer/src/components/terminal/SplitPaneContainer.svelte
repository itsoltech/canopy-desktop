<script lang="ts">
  import type { SplitNode } from '../../lib/stores/splitTree'
  import { buildFlatLayout, findSplitRatio } from '../../lib/stores/splitTree'
  import PaneWrapper from './PaneWrapper.svelte'
  import SplitDivider from './SplitDivider.svelte'

  let {
    node,
    tabId,
    worktreePath,
    focusedPaneId,
    active,
    onFocusPane,
    onUpdateRatio,
  }: {
    node: SplitNode
    tabId: string
    worktreePath: string
    focusedPaneId: string
    active: boolean
    onFocusPane: (paneId: string) => void
    onUpdateRatio: (splitId: string, ratio: number) => void
  } = $props()

  let containerEl: HTMLDivElement | undefined = $state()
  let containerWidth = $state(1)
  let containerHeight = $state(1)

  $effect(() => {
    if (!containerEl) return
    function measure(): void {
      containerWidth = containerEl!.clientWidth || 1
      containerHeight = containerEl!.clientHeight || 1
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(containerEl)
    return () => observer.disconnect()
  })

  let layout = $derived(buildFlatLayout(node, containerWidth, containerHeight))
  let isMultiPane = $derived(node.type !== 'leaf')

  function handleDividerDrag(
    splitId: string,
    direction: 'vertical' | 'horizontal',
    deltaPx: number,
  ): void {
    if (!containerEl) return
    const size = direction === 'vertical' ? containerEl.clientWidth : containerEl.clientHeight
    if (size <= 0) return
    const currentRatio = findSplitRatio(node, splitId)
    if (currentRatio === null) return
    const newRatio = Math.min(0.9, Math.max(0.1, currentRatio + deltaPx / size))
    onUpdateRatio(splitId, newRatio)
  }
</script>

<div class="split-root" bind:this={containerEl}>
  {#each layout.panes as rect (rect.paneId)}
    <div
      class="pane-slot"
      style:left="{rect.x * 100}%"
      style:top="{rect.y * 100}%"
      style:width="{rect.w * 100}%"
      style:height="{rect.h * 100}%"
    >
      <PaneWrapper
        pane={rect.pane}
        {tabId}
        {worktreePath}
        focused={rect.paneId === focusedPaneId}
        {active}
        {isMultiPane}
        onFocus={() => onFocusPane(rect.paneId)}
      />
    </div>
  {/each}

  {#each layout.dividers as div (div.splitId)}
    <div
      class="divider-slot"
      style:left="{div.x * 100}%"
      style:top="{div.y * 100}%"
      style:width="{div.w * 100}%"
      style:height="{div.h * 100}%"
    >
      <SplitDivider
        direction={div.direction}
        onDragDelta={(delta) => handleDividerDrag(div.splitId, div.direction, delta)}
      />
    </div>
  {/each}
</div>

<style>
  .split-root {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .pane-slot {
    position: absolute;
    overflow: hidden;
    min-width: 80px;
    min-height: 60px;
  }

  .divider-slot {
    position: absolute;
    z-index: 5;
  }
</style>
