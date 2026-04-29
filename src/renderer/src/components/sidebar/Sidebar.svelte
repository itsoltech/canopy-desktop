<script lang="ts">
  import { onMount } from 'svelte'
  import ProjectTreeSection from './ProjectTreeSection.svelte'
  import GitSection from './GitSection.svelte'
  import FileTreeSection from './FileTreeSection.svelte'
  import ToolSection from './ToolSection.svelte'
  import TaskTrackerSection from './TaskTrackerSection.svelte'
  import RunConfigSection from './RunConfigSection.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { prefs } from '../../lib/stores/preferences.svelte'
  import { getSidebarConfig } from '../../lib/stores/sidebarSections.svelte'

  let {
    onLaunchTool,
    width = 220,
  }: {
    onLaunchTool: (toolId: string, opts?: { profileId?: string }) => void
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

<aside
  class="flex-shrink-0 h-full bg-bg-glass backdrop-blur-xl border-r border-border-subtle flex flex-col overflow-hidden"
  style="width: {width}px; min-width: {width}px"
>
  <div class="flex-1 min-h-0 overflow-y-auto flex flex-col [scrollbar-gutter:stable]">
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
        {:else if section.id === 'tasks'}
          <TaskTrackerSection />
        {:else if section.id === 'runConfigs'}
          <RunConfigSection />
        {/if}
      {/if}
    {/each}
  </div>
  <div class="flex-shrink-0 px-3 h-7 border-t border-border-subtle flex items-center">
    {#if version}
      <span class="text-2xs text-text-faint tracking-caps-tight uppercase select-none"
        >v{version}</span
      >
    {/if}
  </div>
</aside>
