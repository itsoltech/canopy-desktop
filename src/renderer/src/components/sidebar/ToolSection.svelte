<script lang="ts">
  import { onMount } from 'svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { getRunningCountByTool } from '../../lib/stores/tabs.svelte'

  let { onLaunchTool }: { onLaunchTool: (toolId: string) => void } = $props()

  interface ToolDef {
    id: string
    name: string
    icon: string
    category: string
  }

  let tools: ToolDef[] = $state([])
  let availability: Record<string, boolean> = $state({})

  onMount(async () => {
    const [toolList, avail] = await Promise.all([
      window.api.listTools(),
      window.api.checkToolAvailability()
    ])
    tools = toolList
    availability = avail
  })

  function runningCount(toolId: string): number {
    const path = workspaceState.selectedWorktreePath
    if (!path) return 0
    return getRunningCountByTool(path, toolId)
  }
</script>

<section class="sidebar-section">
  <h3 class="section-title">TOOLS</h3>
  <ul class="tool-list">
    {#each tools as tool (tool.id)}
      {@const count = runningCount(tool.id)}
      <li>
        <button
          class="tool-item"
          class:unavailable={!availability[tool.id]}
          disabled={!availability[tool.id]}
          onclick={() => onLaunchTool(tool.id)}
          title={availability[tool.id] ? tool.name : `${tool.name} — not found in PATH`}
        >
          <span class="tool-name">{tool.name}</span>
          {#if count > 0}
            <span class="badge">{count}</span>
          {/if}
        </button>
      </li>
    {/each}
  </ul>
</section>

<style>
  .sidebar-section {
    padding: 12px 0;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .section-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 1px;
    color: rgba(255, 255, 255, 0.4);
    padding: 0 12px 8px;
    text-transform: uppercase;
  }

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
    padding: 4px 12px;
    border: none;
    background: none;
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .tool-item:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.06);
  }

  .tool-item:disabled {
    cursor: default;
  }

  .tool-item.unavailable {
    color: rgba(255, 255, 255, 0.25);
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
    background: rgba(255, 255, 255, 0.12);
    color: rgba(255, 255, 255, 0.7);
    font-size: 10px;
    font-weight: 600;
    flex-shrink: 0;
  }
</style>
