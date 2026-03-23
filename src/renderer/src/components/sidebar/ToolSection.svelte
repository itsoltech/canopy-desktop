<script lang="ts">
  import { onMount } from 'svelte'

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
</script>

<section class="sidebar-section">
  <h3 class="section-title">TOOLS</h3>
  <ul class="tool-list">
    {#each tools as tool (tool.id)}
      <li>
        <button
          class="tool-item"
          class:unavailable={!availability[tool.id]}
          disabled
          title={availability[tool.id] ? tool.name : `${tool.name} — not found in PATH`}
        >
          <span class="tool-name">{tool.name}</span>
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
    cursor: default;
    text-align: left;
  }

  .tool-item.unavailable {
    color: rgba(255, 255, 255, 0.25);
  }

  .tool-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
