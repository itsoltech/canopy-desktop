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

<section class="sidebar-section">
  <div class="section-header">
    <h3 class="section-title">PROJECTS</h3>
    <button class="new-btn" onclick={handleOpenFolder} title="Open folder">+ open</button>
  </div>
  <ul class="project-list">
    {#each pinnedProjects as ws (ws.id)}
      <li>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="project-item"
          class:active={ws.path === workspaceState.workspace?.path}
          onclick={() => switchProject(ws.path)}
          title={ws.path}
        >
          {#if projectIcons[ws.path]}
            <img class="project-icon" src={projectIcons[ws.path]} alt="" width="14" height="14" />
          {:else}
            <Folder size={14} />
          {/if}
          <span class="project-name">{basename(ws.path)}</span>
          <div class="project-actions">
            <button class="action-btn" onclick={(e) => handlePin(e, ws.id)} title="Unpin">
              <PinOff size={11} />
            </button>
            <button
              class="action-btn"
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
      <li class="separator"></li>
    {/if}
    {#each recentProjects as ws (ws.id)}
      <li>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="project-item"
          class:active={ws.path === workspaceState.workspace?.path}
          onclick={() => switchProject(ws.path)}
          title={ws.path}
        >
          {#if projectIcons[ws.path]}
            <img class="project-icon" src={projectIcons[ws.path]} alt="" width="14" height="14" />
          {:else}
            <Folder size={14} />
          {/if}
          <span class="project-name">{basename(ws.path)}</span>
          <div class="project-actions">
            <button class="action-btn" onclick={(e) => handlePin(e, ws.id)} title="Pin">
              <Pin size={11} />
            </button>
            <button
              class="action-btn"
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
</section>

<style>
  .sidebar-section {
    padding: 12px 0;
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px 8px;
  }

  .section-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 1px;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
  }

  .new-btn {
    font-size: 10px;
    font-weight: 500;
    font-family: inherit;
    color: rgba(255, 255, 255, 0.35);
    background: none;
    border: none;
    padding: 1px 5px;
    border-radius: 4px;
    cursor: pointer;
    transition:
      color 0.1s,
      background 0.1s;
  }

  .new-btn:hover {
    color: rgba(255, 255, 255, 0.7);
    background: rgba(255, 255, 255, 0.08);
  }

  .project-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .separator {
    height: 1px;
    background: rgba(255, 255, 255, 0.06);
    margin: 4px 12px;
  }

  .project-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 12px;
    border: none;
    background: none;
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
  }

  .project-item:hover {
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.9);
  }

  .project-item.active {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .project-item:hover .project-actions {
    opacity: 1;
  }

  .project-icon {
    width: 14px;
    height: 14px;
    border-radius: 2px;
    flex-shrink: 0;
  }

  .project-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }

  .project-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: auto;
    opacity: 0;
    transition: opacity 0.1s;
    flex-shrink: 0;
  }

  .action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border: none;
    background: none;
    color: rgba(255, 255, 255, 0.4);
    border-radius: 3px;
    cursor: pointer;
    padding: 0;
  }

  .action-btn:hover {
    color: rgba(255, 255, 255, 0.8);
    background: rgba(255, 255, 255, 0.1);
  }
</style>
