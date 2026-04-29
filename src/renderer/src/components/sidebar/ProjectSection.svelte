<script lang="ts">
  import { onMount } from 'svelte'
  import { Folder, Pin, PinOff, X } from 'lucide-svelte'
  import {
    workspaceState,
    projectList,
    loadProjectList,
    switchProject,
    toggleProjectPin,
    removeProject,
  } from '../../lib/stores/workspace.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'

  let projectIcons: Record<string, string> = $state({})

  function basename(p: string): string {
    return p.split('/').pop() || p
  }

  async function handleOpenFolder(): Promise<void> {
    const path = await window.api.openFolder()
    if (path) await switchProject(path)
  }

  async function handleRemove(e: MouseEvent, id: string, name: string): Promise<void> {
    e.stopPropagation()
    const confirmed = await confirm({
      title: 'Remove project?',
      message: `Remove "${name}" from the project list? This won't delete any files.`,
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (confirmed) await removeProject(id)
  }

  function handlePin(e: MouseEvent, id: string): void {
    e.stopPropagation()
    toggleProjectPin(id)
  }

  async function detectIcons(): Promise<void> {
    for (const ws of projectList) {
      if (projectIcons[ws.path]) continue
      window.api.detectWorkspaceIcon(ws.path).then((icon) => {
        if (icon) projectIcons[ws.path] = icon
      })
    }
  }

  onMount(async () => {
    await loadProjectList()
    detectIcons()
  })

  const pinnedProjects = $derived(projectList.filter((ws) => ws.is_pinned === 1))
  const recentProjects = $derived(projectList.filter((ws) => ws.is_pinned !== 1))
  const hasPinned = $derived(pinnedProjects.length > 0)
</script>

<CollapsibleSection title="PROJECTS" sectionKey="projects">
  {#snippet headerExtra()}
    <button
      class="inline-flex items-center h-5 px-1.5 rounded-sm font-inherit text-2xs font-medium text-text-faint bg-transparent border-0 cursor-pointer transition-colors duration-fast hover:text-text hover:bg-hover"
      onclick={handleOpenFolder}
      title="Open folder">+ open</button
    >
  {/snippet}
  <ul class="list-none p-0 m-0">
    {#each pinnedProjects as ws (ws.id)}
      <li>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="group flex items-center gap-2 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left hover:bg-hover"
          class:bg-active={ws.path === workspaceState.workspace?.path}
          onclick={() => switchProject(ws.path)}
          title={ws.path}
        >
          {#if projectIcons[ws.path]}
            <img
              class="w-3.5 h-3.5 rounded-xs flex-shrink-0"
              src={projectIcons[ws.path]}
              alt=""
              width="14"
              height="14"
            />
          {:else}
            <Folder size={14} />
          {/if}
          <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1 min-w-0"
            >{basename(ws.path)}</span
          >
          <div
            class="flex items-center gap-0.5 ml-auto opacity-0 group-hover:opacity-100 transition-opacity duration-fast flex-shrink-0"
          >
            <button
              class="inline-flex items-center justify-center size-5 border-0 bg-transparent text-text-faint rounded-sm cursor-pointer p-0 transition-colors duration-fast hover:text-text hover:bg-hover"
              onclick={(e) => handlePin(e, ws.id)}
              title="Unpin"
            >
              <PinOff size={11} />
            </button>
            <button
              class="inline-flex items-center justify-center size-5 border-0 bg-transparent text-text-faint rounded-sm cursor-pointer p-0 transition-colors duration-fast hover:text-text hover:bg-hover"
              onclick={(e) => handleRemove(e, ws.id, ws.name)}
              title="Remove"
            >
              <X size={11} />
            </button>
          </div>
        </div>
      </li>
    {/each}
    {#if hasPinned && recentProjects.length > 0}
      <li class="h-px mx-3 my-1 bg-border-subtle"></li>
    {/if}
    {#each recentProjects as ws (ws.id)}
      <li>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="group flex items-center gap-2 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left hover:bg-hover"
          class:bg-active={ws.path === workspaceState.workspace?.path}
          onclick={() => switchProject(ws.path)}
          title={ws.path}
        >
          {#if projectIcons[ws.path]}
            <img
              class="w-3.5 h-3.5 rounded-xs flex-shrink-0"
              src={projectIcons[ws.path]}
              alt=""
              width="14"
              height="14"
            />
          {:else}
            <Folder size={14} />
          {/if}
          <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1 min-w-0"
            >{basename(ws.path)}</span
          >
          <div
            class="flex items-center gap-0.5 ml-auto opacity-0 group-hover:opacity-100 transition-opacity duration-fast flex-shrink-0"
          >
            <button
              class="inline-flex items-center justify-center size-5 border-0 bg-transparent text-text-faint rounded-sm cursor-pointer p-0 transition-colors duration-fast hover:text-text hover:bg-hover"
              onclick={(e) => handlePin(e, ws.id)}
              title="Pin"
            >
              <Pin size={11} />
            </button>
            <button
              class="inline-flex items-center justify-center size-5 border-0 bg-transparent text-text-faint rounded-sm cursor-pointer p-0 transition-colors duration-fast hover:text-text hover:bg-hover"
              onclick={(e) => handleRemove(e, ws.id, ws.name)}
              title="Remove"
            >
              <X size={11} />
            </button>
          </div>
        </div>
      </li>
    {/each}
  </ul>
</CollapsibleSection>
