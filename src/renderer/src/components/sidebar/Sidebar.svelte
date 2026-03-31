<script lang="ts">
  import { onMount } from 'svelte'
  import ProjectTreeSection from './ProjectTreeSection.svelte'
  import GitSection from './GitSection.svelte'
  import FileTreeSection from './FileTreeSection.svelte'
  import ToolSection from './ToolSection.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { prefs } from '../../lib/stores/preferences.svelte'
  import { getSidebarConfig } from '../../lib/stores/sidebarSections.svelte'

  let {
    onLaunchTool,
    width = 220,
  }: {
    onLaunchTool: (toolId: string) => void
    width?: number
  } = $props()

  let version = $state('')

  let sections = $derived.by(() => {
    const raw = prefs['sidebar.sections'] ?? ''
    return getSidebarConfig(raw)
  })

  onMount(async () => {
    const info = await window.api.getAboutInfo()
    version = info.version
  })
</script>

<aside class="sidebar" style="width: {width}px; min-width: {width}px">
  {#each sections as section (section.id)}
    {#if section.visible}
      {#if section.id === 'projects'}
        <ProjectTreeSection />
      {:else if section.id === 'git'}
        {#if workspaceState.isGitRepo && workspaceState.selectedWorktreePath}
          <GitSection />
        {/if}
      {:else if section.id === 'files'}
        <FileTreeSection />
      {:else if section.id === 'tools'}
        <ToolSection {onLaunchTool} />
      {/if}
    {/if}
  {/each}
  <div class="sidebar-footer">
    {#if version}
      <span class="version-label">v{version}</span>
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    flex-shrink: 0;
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

  .sidebar-footer {
    margin-top: auto;
    padding: 8px 12px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .version-label {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.3);
    user-select: none;
  }
</style>
