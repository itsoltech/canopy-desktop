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
  <div class="flex items-stretch px-3 border-b border-border-subtle flex-shrink-0" role="tablist">
    <button
      class="relative flex-1 inline-flex items-center justify-center gap-1.5 h-9 text-xs font-medium font-inherit border-0 bg-transparent cursor-pointer transition-colors duration-fast"
      class:text-text={workspaceState.rightPanelTab === 'session'}
      class:text-text-faint={workspaceState.rightPanelTab !== 'session'}
      class:hover:text-text-secondary={workspaceState.rightPanelTab !== 'session'}
      role="tab"
      aria-selected={workspaceState.rightPanelTab === 'session'}
      onclick={() => (workspaceState.rightPanelTab = 'session')}
    >
      Session
      {#if workspaceState.rightPanelTab === 'session'}
        <span class="absolute inset-x-0 -bottom-px h-0.5 bg-accent" aria-hidden="true"></span>
      {/if}
    </button>
    <button
      class="relative flex-1 inline-flex items-center justify-center gap-1.5 h-9 text-xs font-medium font-inherit border-0 bg-transparent cursor-pointer transition-colors duration-fast"
      class:text-text={workspaceState.rightPanelTab === 'changes'}
      class:text-text-faint={workspaceState.rightPanelTab !== 'changes'}
      class:hover:text-text-secondary={workspaceState.rightPanelTab !== 'changes'}
      role="tab"
      aria-selected={workspaceState.rightPanelTab === 'changes'}
      onclick={() => (workspaceState.rightPanelTab = 'changes')}
    >
      Changes
      {#if workspaceState.changesCount > 0}
        <span
          class="inline-flex items-center justify-center min-w-4 h-4 px-1 rounded-sm bg-border-subtle text-text-secondary text-2xs font-semibold tracking-caps-tight tabular-nums leading-none"
        >
          {workspaceState.changesCount}
        </span>
      {/if}
      {#if workspaceState.rightPanelTab === 'changes'}
        <span class="absolute inset-x-0 -bottom-px h-0.5 bg-accent" aria-hidden="true"></span>
      {/if}
    </button>
  </div>

  <div
    class="flex-1 overflow-y-auto min-h-0"
    class:hidden={workspaceState.rightPanelTab !== 'session'}
  >
    {#if agentState}
      <AgentInspector state={agentState} />
    {:else}
      <div class="flex items-center justify-center h-full p-4">
        <span class="text-sm text-text-faint">No active agent session</span>
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
        <span class="text-sm text-text-faint">No changes yet</span>
      </div>
    {/if}
  </div>
</aside>
