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

<aside
  class="min-w-50 max-w-125 h-full bg-bg border-l border-border-subtle flex flex-col flex-shrink-0"
  style:width="{width}px"
>
  <div class="flex items-center justify-center px-3 py-2 flex-shrink-0">
    <div class="flex gap-1 w-full bg-bg-input rounded-lg p-0.5" role="tablist">
      <button
        class="flex-1 flex items-center justify-center gap-1 h-6 text-xs font-medium cursor-pointer border-0 font-inherit rounded-md transition-colors duration-base motion-reduce:transition-none"
        class:bg-active={workspaceState.rightPanelTab === 'session'}
        class:bg-transparent={workspaceState.rightPanelTab !== 'session'}
        class:text-text={workspaceState.rightPanelTab === 'session'}
        class:text-text-secondary={workspaceState.rightPanelTab !== 'session'}
        class:hover:bg-hover={workspaceState.rightPanelTab !== 'session'}
        class:hover:text-text={workspaceState.rightPanelTab !== 'session'}
        role="tab"
        aria-selected={workspaceState.rightPanelTab === 'session'}
        onclick={() => (workspaceState.rightPanelTab = 'session')}
      >
        Session
      </button>
      <button
        class="flex-1 flex items-center justify-center gap-1 h-6 text-xs font-medium cursor-pointer border-0 font-inherit rounded-md transition-colors duration-base motion-reduce:transition-none"
        class:bg-active={workspaceState.rightPanelTab === 'changes'}
        class:bg-transparent={workspaceState.rightPanelTab !== 'changes'}
        class:text-text={workspaceState.rightPanelTab === 'changes'}
        class:text-text-secondary={workspaceState.rightPanelTab !== 'changes'}
        class:hover:bg-hover={workspaceState.rightPanelTab !== 'changes'}
        class:hover:text-text={workspaceState.rightPanelTab !== 'changes'}
        role="tab"
        aria-selected={workspaceState.rightPanelTab === 'changes'}
        onclick={() => (workspaceState.rightPanelTab = 'changes')}
      >
        Changes
        {#if workspaceState.changesCount > 0}
          <span
            class="text-2xs font-semibold min-w-3.5 h-3.5 rounded-3xl inline-flex items-center justify-center px-1 leading-none"
            class:bg-border={workspaceState.rightPanelTab === 'changes'}
            class:text-text={workspaceState.rightPanelTab === 'changes'}
            class:bg-active={workspaceState.rightPanelTab !== 'changes'}
            class:text-text-secondary={workspaceState.rightPanelTab !== 'changes'}
          >
            {workspaceState.changesCount}
          </span>
        {/if}
      </button>
    </div>
  </div>

  <div
    class="flex-1 overflow-y-auto min-h-0"
    class:hidden={workspaceState.rightPanelTab !== 'session'}
  >
    {#if agentState}
      <AgentInspector state={agentState} />
    {:else}
      <div class="flex items-center justify-center h-full p-4">
        <span class="text-sm text-text-muted">No active agent session</span>
      </div>
    {/if}
  </div>

  <div
    class="flex-1 overflow-y-auto min-h-0"
    class:hidden={workspaceState.rightPanelTab !== 'changes'}
  >
    {#if worktreePath}
      <ChangesPanel {worktreePath} bind:fileCount={changesFileCount} />
    {:else}
      <div class="flex items-center justify-center h-full p-4">
        <span class="text-sm text-text-muted">No changes yet</span>
      </div>
    {/if}
  </div>
</aside>
