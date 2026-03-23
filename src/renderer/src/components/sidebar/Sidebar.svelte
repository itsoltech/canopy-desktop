<script lang="ts">
  import WorktreeSection from './WorktreeSection.svelte'
  import ToolSection from './ToolSection.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'

  let { onLaunchTool }: { onLaunchTool: (toolId: string) => void } = $props()
</script>

<aside class="sidebar">
  {#if workspaceState.isGitRepo}
    <WorktreeSection />
  {/if}
  <ToolSection {onLaunchTool} />
</aside>

<style>
  .sidebar {
    width: 220px;
    min-width: 220px;
    height: 100%;
    background: rgba(30, 30, 30, 0.75);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border-right: 1px solid rgba(255, 255, 255, 0.06);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0;
  }
</style>
