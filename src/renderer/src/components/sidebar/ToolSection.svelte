<script lang="ts">
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { getRunningCountByTool } from '../../lib/stores/tabs.svelte'
  import { getTools, getToolAvailability } from '../../lib/stores/tools.svelte'
  import ToolIcon from '../shared/ToolIcon.svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'

  let { onLaunchTool }: { onLaunchTool: (toolId: string) => void } = $props()

  function runningCount(toolId: string): number {
    const path = workspaceState.selectedWorktreePath
    if (!path) return 0
    return getRunningCountByTool(path, toolId)
  }
</script>

<CollapsibleSection title="TOOLS" sectionKey="tools" borderTop>
  <ul class="tool-list">
    {#each getTools() as tool (tool.id)}
      {@const count = runningCount(tool.id)}
      {@const available = getToolAvailability()[tool.id] !== false}
      <li>
        <button
          class="tool-item"
          class:unavailable={!available}
          disabled={!available}
          onclick={() => onLaunchTool(tool.id)}
          title={available ? tool.name : `${tool.name} — not found in PATH`}
        >
          <ToolIcon icon={tool.icon} size={14} />
          <span class="tool-name">{tool.name}</span>
          {#if count > 0}
            <span class="badge">{count}</span>
          {/if}
        </button>
      </li>
    {/each}
  </ul>
</CollapsibleSection>

<style>
  .tool-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .tool-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 12px;
    border: none;
    background: none;
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .tool-item:hover:not(:disabled) {
    background: var(--c-hover);
  }

  .tool-item:disabled {
    cursor: default;
  }

  .tool-item.unavailable {
    color: var(--c-text-faint);
  }

  .tool-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .badge {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--c-border);
    color: var(--c-text);
    font-size: 10px;
    font-weight: 600;
    flex-shrink: 0;
  }
</style>
