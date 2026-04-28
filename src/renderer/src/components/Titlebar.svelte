<script lang="ts">
  import { workspaceState } from '../lib/stores/workspace.svelte'
  import { tabsByWorktree, activeTabId, getTabDisplayName } from '../lib/stores/tabs.svelte'
  import TitlebarMenu from './TitlebarMenu.svelte'
  import RunConfigToolbar from './runConfig/RunConfigToolbar.svelte'
  import { getPref } from '../lib/stores/preferences.svelte'

  const isMac = navigator.userAgent.includes('Mac')

  let showRunToolbar = $derived(
    workspaceState.repoRoot && getPref('runConfig.showInTitlebar', 'false') === 'true',
  )

  let activeTabName = $derived.by(() => {
    const path = workspaceState.selectedWorktreePath
    if (!path) return null
    const tabId = activeTabId[path]
    if (!tabId) return null
    const tab = (tabsByWorktree[path] ?? []).find((t) => t.id === tabId)
    return tab ? getTabDisplayName(tab) : null
  })

  let fullTitle = $derived.by(() => {
    if (!workspaceState.workspace) return 'Canopy'
    let title = workspaceState.workspace.name
    if (workspaceState.branch) title += ` — ${workspaceState.branch}`
    if (activeTabName) title += ` — ${activeTabName}`
    if (workspaceState.isDirty) title += ' *'
    return title
  })

  $effect(() => {
    document.title = fullTitle
  })
</script>

<div
  class="h-10 flex items-center justify-center flex-shrink-0 relative bg-bg-glass backdrop-blur-xl border-b border-border-subtle app-drag"
  class:px-mac-traffic={isMac}
>
  {#if !isMac}
    <div class="absolute left-2 top-0 h-full flex items-center app-no-drag">
      <TitlebarMenu />
    </div>
  {/if}
  {#if workspaceState.workspace}
    <span
      class="inline-block max-w-full min-w-0 truncate px-2 align-middle text-sm font-medium text-text-secondary tracking-caps-looser"
      title={fullTitle}
    >
      {workspaceState.workspace.name}
      {#if workspaceState.branch}
        <span class="text-text-muted ml-1.5 max-tight:hidden">— {workspaceState.branch}</span>
      {/if}
      {#if activeTabName}
        <span class="text-text-muted ml-1.5 max-narrow:hidden">— {activeTabName}</span>
      {/if}
      {#if workspaceState.isDirty}
        <span class="text-warning-text ml-0.5">*</span>
      {/if}
    </span>
  {:else}
    <span
      class="inline-block max-w-full min-w-0 truncate px-2 align-middle text-sm font-medium text-text-secondary tracking-caps-looser"
      >Canopy</span
    >
  {/if}
  {#if showRunToolbar}
    <div class="absolute right-3 top-0 h-full flex items-center app-no-drag">
      <RunConfigToolbar />
    </div>
  {/if}
</div>
