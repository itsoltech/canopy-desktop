<script lang="ts">
  import type { SplitNode } from '../../lib/stores/splitTree'
  import SplitPaneContainer from './SplitPaneContainer.svelte'
  import PaneWrapper from './PaneWrapper.svelte'
  import SplitDivider from './SplitDivider.svelte'

  let {
    node,
    tabId,
    worktreePath,
    focusedPaneId,
    active,
    onFocusPane,
    onUpdateRatio
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

  function handleDrag(deltaPx: number): void {
    if (!containerEl || node.type === 'leaf') return
    const size = node.type === 'vsplit' ? containerEl.clientWidth : containerEl.clientHeight
    if (size <= 0) return
    const newRatio = Math.min(0.9, Math.max(0.1, node.ratio + deltaPx / size))
    onUpdateRatio(node.id, newRatio)
  }
</script>

{#if node.type === 'leaf'}
  <PaneWrapper
    pane={node.pane}
    {tabId}
    {worktreePath}
    focused={node.pane.id === focusedPaneId}
    {active}
    onFocus={() => onFocusPane(node.pane.id)}
  />
{:else}
  <div
    class="split-container"
    class:vsplit={node.type === 'vsplit'}
    class:hsplit={node.type === 'hsplit'}
    bind:this={containerEl}
  >
    <div class="split-child" style:flex-basis="{node.ratio * 100}%">
      <SplitPaneContainer
        node={node.first}
        {tabId}
        {worktreePath}
        {focusedPaneId}
        {active}
        {onFocusPane}
        {onUpdateRatio}
      />
    </div>
    <SplitDivider
      direction={node.type === 'vsplit' ? 'vertical' : 'horizontal'}
      onDragDelta={handleDrag}
    />
    <div class="split-child" style:flex-basis="{(1 - node.ratio) * 100}%">
      <SplitPaneContainer
        node={node.second}
        {tabId}
        {worktreePath}
        {focusedPaneId}
        {active}
        {onFocusPane}
        {onUpdateRatio}
      />
    </div>
  </div>
{/if}

<style>
  .split-container {
    display: flex;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .split-container.vsplit {
    flex-direction: row;
  }

  .split-container.hsplit {
    flex-direction: column;
  }

  .split-child {
    min-width: 80px;
    min-height: 60px;
    overflow: hidden;
    position: relative;
  }
</style>
