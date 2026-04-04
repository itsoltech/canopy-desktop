<script lang="ts">
  import type { AgentSessionState } from '../../lib/agents/agentState.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import AgentInspector from '../agents/AgentInspector.svelte'
  import ChangesPanel from '../diff/ChangesPanel.svelte'

  let {
    agentState = null,
    width = 280,
    worktreePath = '',
  }: {
    agentState?: AgentSessionState | null
    width?: number
    worktreePath?: string
  } = $props()

  let changesFileCount = $state(0)

  $effect(() => {
    workspaceState.changesCount = changesFileCount
  })
</script>

<aside class="right-panel" style:width="{width}px">
  <div class="tab-header">
    <div class="segmented-control" role="tablist">
      <button
        class="segment"
        class:active={workspaceState.rightPanelTab === 'session'}
        role="tab"
        aria-selected={workspaceState.rightPanelTab === 'session'}
        onclick={() => (workspaceState.rightPanelTab = 'session')}
      >
        Session
      </button>
      <button
        class="segment"
        class:active={workspaceState.rightPanelTab === 'changes'}
        role="tab"
        aria-selected={workspaceState.rightPanelTab === 'changes'}
        onclick={() => (workspaceState.rightPanelTab = 'changes')}
      >
        Changes
        {#if workspaceState.changesCount > 0}
          <span class="badge">{workspaceState.changesCount}</span>
        {/if}
      </button>
    </div>
  </div>

  <div class="tab-content">
    {#if workspaceState.rightPanelTab === 'session'}
      {#if agentState}
        <AgentInspector state={agentState} />
      {:else}
        <div class="empty-state">
          <span class="empty-text">No active agent session</span>
        </div>
      {/if}
    {:else if worktreePath}
      <ChangesPanel {worktreePath} bind:fileCount={changesFileCount} />
    {:else}
      <div class="empty-state">
        <span class="empty-text">No changes yet</span>
      </div>
    {/if}
  </div>
</aside>

<style>
  .right-panel {
    min-width: 200px;
    max-width: 500px;
    height: 100%;
    background: var(--c-bg);
    border-left: 1px solid var(--c-border-subtle);
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
  }

  .tab-header {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 8px 12px;
    flex-shrink: 0;
  }

  .segmented-control {
    display: flex;
    background: var(--c-active);
    border-radius: 8px;
    padding: 2px;
    width: 100%;
  }

  .segment {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: 26px;
    font-size: 11px;
    font-weight: 500;
    color: var(--c-text-faint);
    cursor: pointer;
    border: none;
    background: none;
    font-family: inherit;
    border-radius: 6px;
    transition:
      background 0.2s cubic-bezier(0.25, 0.1, 0.25, 1),
      color 0.15s,
      box-shadow 0.2s cubic-bezier(0.25, 0.1, 0.25, 1);
  }

  .segment:hover:not(.active) {
    color: var(--c-text-secondary);
    background: var(--c-border-subtle);
  }

  .segment.active {
    background: var(--c-bg-elevated);
    color: var(--c-text);
    font-weight: 600;
    box-shadow:
      0 1px 4px rgba(0, 0, 0, 0.3),
      0 0.5px 1px rgba(0, 0, 0, 0.2);
  }

  .badge {
    font-size: 10px;
    font-weight: 600;
    min-width: 14px;
    height: 14px;
    border-radius: 7px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0 4px;
    background: var(--c-active);
    color: var(--c-text-secondary);
    line-height: 1;
  }

  .segment.active .badge {
    background: var(--c-border);
    color: var(--c-text);
  }

  .tab-content {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: 16px;
  }

  .empty-text {
    font-size: 12px;
    color: var(--c-text-muted);
  }

  @media (prefers-reduced-motion: reduce) {
    .segment {
      transition: none;
    }
  }
</style>
