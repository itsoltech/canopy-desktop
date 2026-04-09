<script lang="ts">
  import { workspaceState } from '../lib/stores/workspace.svelte'
  import { tabsByWorktree, activeTabId, getTabDisplayName } from '../lib/stores/tabs.svelte'
  import TitlebarMenu from './TitlebarMenu.svelte'

  const isMac = navigator.userAgent.includes('Mac')

  let activeTabName = $derived.by(() => {
    const path = workspaceState.selectedWorktreePath
    if (!path) return null
    const tabId = activeTabId[path]
    if (!tabId) return null
    const tab = (tabsByWorktree[path] ?? []).find((t) => t.id === tabId)
    return tab ? getTabDisplayName(tab) : null
  })

  $effect(() => {
    if (workspaceState.workspace) {
      let title = workspaceState.workspace.name
      if (workspaceState.branch) title += ` — ${workspaceState.branch}`
      if (activeTabName) title += ` — ${activeTabName}`
      if (workspaceState.isDirty) title += ' *'
      document.title = title
    } else {
      document.title = 'Canopy'
    }
  })
</script>

<div class="titlebar" class:mac={isMac}>
  {#if !isMac}
    <div class="menu-area">
      <TitlebarMenu />
    </div>
  {/if}
  {#if workspaceState.workspace}
    <span class="title">
      {workspaceState.workspace.name}
      {#if workspaceState.branch}
        <span class="branch">{workspaceState.branch}</span>
      {/if}
      {#if activeTabName}
        <span class="tab-name">{activeTabName}</span>
      {/if}
      {#if workspaceState.isDirty}
        <span class="dirty">*</span>
      {/if}
    </span>
  {:else}
    <span class="title">Canopy</span>
  {/if}
</div>

<style>
  .titlebar {
    height: 40px;
    app-region: drag;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    position: relative;
    background: var(--c-bg-glass);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .titlebar.mac {
    padding-left: 78px;
    padding-right: 78px;
  }

  .menu-area {
    position: absolute;
    left: 8px;
    top: 0;
    height: 100%;
    display: flex;
    align-items: center;
    app-region: no-drag;
  }

  .title {
    font-size: 12px;
    font-weight: 500;
    color: var(--c-text-secondary);
    letter-spacing: 0.5px;

    display: inline-block;
    max-width: 100%;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    box-sizing: border-box;
    padding: 0 8px;
    vertical-align: middle;
  }

  .branch {
    color: var(--c-text-muted);
    margin-left: 6px;
  }

  .branch::before {
    content: '\2014\00a0';
  }

  .tab-name {
    color: var(--c-text-muted);
    margin-left: 6px;
  }

  .tab-name::before {
    content: '\2014\00a0';
  }

  .dirty {
    color: var(--c-warning-text);
    margin-left: 2px;
  }

  /* Narrow windows: drop the active pane/process title first */
  @media (max-width: 820px) {
    .tab-name {
      display: none;
    }
  }

  /* Very narrow (defensive — current BrowserWindow minWidth is 600): drop branch too */
  @media (max-width: 560px) {
    .branch {
      display: none;
    }
  }
</style>
