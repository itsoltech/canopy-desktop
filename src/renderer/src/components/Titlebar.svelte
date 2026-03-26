<script lang="ts">
  import { workspaceState } from '../lib/stores/workspace.svelte'
  import TitlebarMenu from './TitlebarMenu.svelte'

  const isMac = navigator.userAgent.includes('Mac')

  $effect(() => {
    if (workspaceState.workspace) {
      let title = workspaceState.workspace.name
      if (workspaceState.branch) title += ` — ${workspaceState.branch}`
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
    background: rgba(30, 30, 30, 0.75);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
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
    color: rgba(255, 255, 255, 0.6);
    letter-spacing: 0.5px;
  }

  .branch {
    color: rgba(255, 255, 255, 0.4);
    margin-left: 6px;
  }

  .branch::before {
    content: '\2014\00a0';
  }

  .dirty {
    color: rgba(255, 200, 50, 0.7);
    margin-left: 2px;
  }
</style>
