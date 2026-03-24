<script lang="ts">
  import { onMount } from 'svelte'
  import { openWorkspace, loadProjectList } from '../../lib/stores/workspace.svelte'
  import { prompt } from '../../lib/stores/dialogs.svelte'

  interface WorkspaceRow {
    id: string
    path: string
    name: string
    is_git_repo: number
    is_pinned: number
    last_opened: string | null
    cached_branch: string | null
    cached_dirty: number | null
    cached_ahead_behind: string | null
    cached_worktree_count: number | null
  }

  const isMac = navigator.userAgent.includes('Mac')
  let workspaces = $state<WorkspaceRow[]>([])
  let contextMenu = $state<{ x: number; y: number; workspace: WorkspaceRow } | null>(null)

  function relativeTime(dateStr: string | null): string {
    if (!dateStr) return ''
    const diff = Date.now() - Date.parse(dateStr)
    const seconds = Math.floor(diff / 1000)
    if (seconds < 60) return 'just now'
    const minutes = Math.floor(seconds / 60)
    if (minutes < 60) return `${minutes}m ago`
    const hours = Math.floor(minutes / 60)
    if (hours < 24) return `${hours}h ago`
    const days = Math.floor(hours / 24)
    if (days === 1) return 'yesterday'
    if (days < 30) return `${days}d ago`
    const months = Math.floor(days / 30)
    return `${months}mo ago`
  }

  function parseAheadBehind(raw: string | null): { ahead: number; behind: number } | null {
    if (!raw) return null
    try {
      const parsed = JSON.parse(raw)
      if (parsed && typeof parsed.ahead === 'number') return parsed
      // Legacy format: "ahead/behind"
      const parts = raw.split('/')
      if (parts.length === 2) {
        return { ahead: parseInt(parts[0]), behind: parseInt(parts[1]) }
      }
    } catch {
      // Legacy "ahead/behind" string format
      const parts = raw.split('/')
      if (parts.length === 2) {
        const ahead = parseInt(parts[0])
        const behind = parseInt(parts[1])
        if (!isNaN(ahead) && !isNaN(behind)) return { ahead, behind }
      }
    }
    return null
  }

  onMount(async () => {
    workspaces = await window.api.listWorkspaces(10)

    // Background refresh git status for each git repo
    for (const ws of workspaces) {
      if (ws.is_git_repo) {
        window.api
          .refreshWorkspaceGitStatus(ws.id, ws.path)
          .then((fresh) => {
            if (fresh) {
              const idx = workspaces.findIndex((w) => w.id === fresh.id)
              if (idx !== -1) {
                workspaces[idx] = fresh
              }
            }
          })
          .catch(() => {
            // Path may no longer exist — ignore
          })
      }
    }
  })

  async function handleOpen(ws: WorkspaceRow): Promise<void> {
    await openWorkspace(ws.path)
    loadProjectList()
  }

  async function handleOpenFolder(): Promise<void> {
    const path = await window.api.openFolder()
    if (path) {
      await openWorkspace(path)
      loadProjectList()
    }
  }

  async function handleOpenFromPath(): Promise<void> {
    const result = await prompt({
      title: 'Open from Path',
      placeholder: '/path/to/project',
      submitLabel: 'Open',
    })
    if (result) {
      await openWorkspace(result.value)
      loadProjectList()
    }
  }

  function handleContextMenu(e: MouseEvent, ws: WorkspaceRow): void {
    e.preventDefault()
    contextMenu = { x: e.clientX, y: e.clientY, workspace: ws }
  }

  function closeContextMenu(): void {
    contextMenu = null
  }

  async function ctxShowInFinder(): Promise<void> {
    if (!contextMenu) return
    await window.api.showInFolder(contextMenu.workspace.path)
    closeContextMenu()
  }

  async function ctxCopyPath(): Promise<void> {
    if (!contextMenu) return
    await navigator.clipboard.writeText(contextMenu.workspace.path)
    closeContextMenu()
  }

  async function ctxRemove(): Promise<void> {
    if (!contextMenu) return
    const id = contextMenu.workspace.id
    await window.api.removeWorkspace(id)
    workspaces = workspaces.filter((w) => w.id !== id)
    closeContextMenu()
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (contextMenu && e.key === 'Escape') {
      e.preventDefault()
      closeContextMenu()
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
{#if contextMenu}
  <div class="ctx-overlay" onclick={closeContextMenu}>
    <div
      class="ctx-menu"
      style="left: {contextMenu.x}px; top: {contextMenu.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <button class="ctx-item" onclick={ctxShowInFinder}>Show in Finder</button>
      <button class="ctx-item" onclick={ctxCopyPath}>Copy Path</button>
      <div class="ctx-divider"></div>
      <button class="ctx-item destructive" onclick={ctxRemove}>Remove from Recent</button>
    </div>
  </div>
{/if}

<div class="dashboard">
  <div class="dashboard-inner">
    <h1 class="logo">Canopy</h1>

    {#if workspaces.length > 0}
      <h2 class="section-title">Recent</h2>
      <div class="workspace-list">
        {#each workspaces as ws (ws.id)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="workspace-card"
            onclick={() => handleOpen(ws)}
            oncontextmenu={(e) => handleContextMenu(e, ws)}
          >
            <div class="card-row-top">
              <span class="card-name">{ws.name}</span>
              <span class="card-meta">
                {#if ws.cached_branch}
                  <span class="card-branch">{ws.cached_branch}</span>
                {/if}
                {#if ws.cached_dirty === 1}
                  <span class="card-dirty">*</span>
                {/if}
                {#if parseAheadBehind(ws.cached_ahead_behind)}
                  {@const ab = parseAheadBehind(ws.cached_ahead_behind)!}
                  {#if ab.ahead > 0}
                    <span class="card-ahead">{ab.ahead}&#x2191;</span>
                  {/if}
                  {#if ab.behind > 0}
                    <span class="card-behind">{ab.behind}&#x2193;</span>
                  {/if}
                {/if}
              </span>
            </div>
            <div class="card-row-bottom">
              <span class="card-path">{ws.path}</span>
              <span class="card-info">
                {#if ws.is_git_repo && ws.cached_worktree_count && ws.cached_worktree_count > 1}
                  <span class="card-worktrees">{ws.cached_worktree_count} worktrees</span>
                {/if}
                {#if ws.last_opened}
                  <span class="card-time">{relativeTime(ws.last_opened)}</span>
                {/if}
              </span>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    <div class="quick-actions">
      <button class="action-btn" onclick={handleOpenFolder}>Open Folder</button>
      <button class="action-btn" onclick={handleOpenFromPath}>Open from Path...</button>
      <span class="action-hint">{isMac ? 'Cmd' : 'Ctrl'}+O</span>
    </div>
  </div>
</div>

<style>
  .dashboard {
    display: flex;
    align-items: flex-start;
    justify-content: center;
    height: 100%;
    overflow-y: auto;
    padding: 60px 20px;
  }

  .dashboard-inner {
    width: 100%;
    max-width: 520px;
  }

  .logo {
    font-size: 28px;
    font-weight: 700;
    color: rgba(255, 255, 255, 0.15);
    letter-spacing: 2px;
    text-align: center;
    margin: 0 0 40px;
  }

  .section-title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: rgba(255, 255, 255, 0.3);
    margin: 0 0 8px;
  }

  .workspace-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 24px;
  }

  .workspace-card {
    padding: 10px 12px;
    border-radius: 6px;
    cursor: pointer;
    transition: background 0.08s;
  }

  .workspace-card:hover {
    background: rgba(255, 255, 255, 0.06);
  }

  .card-row-top {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  .card-name {
    font-size: 13px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.85);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .card-meta {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-shrink: 0;
    font-size: 12px;
  }

  .card-branch {
    color: rgba(255, 255, 255, 0.4);
  }

  .card-dirty {
    color: rgba(255, 200, 50, 0.8);
    font-weight: 700;
  }

  .card-ahead {
    color: rgba(100, 200, 100, 0.7);
    font-size: 11px;
  }

  .card-behind {
    color: rgba(255, 150, 100, 0.7);
    font-size: 11px;
  }

  .card-row-bottom {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-top: 2px;
  }

  .card-path {
    font-size: 11px;
    font-family: monospace;
    color: rgba(255, 255, 255, 0.2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .card-info {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-shrink: 0;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.2);
  }

  .card-worktrees {
    color: rgba(255, 255, 255, 0.25);
  }

  .card-time {
    color: rgba(255, 255, 255, 0.2);
  }

  .quick-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .action-btn {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    border: none;
    outline: none;
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.6);
    transition: background 0.1s;
  }

  .action-btn:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .action-btn:focus-visible {
    outline: 2px solid rgba(116, 192, 252, 0.6);
    outline-offset: 1px;
  }

  .action-hint {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.15);
    margin-left: 4px;
  }

  /* Context menu */

  .ctx-overlay {
    position: fixed;
    inset: 0;
    z-index: 1002;
  }

  .ctx-menu {
    position: fixed;
    min-width: 180px;
    background: rgba(40, 40, 40, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    padding: 4px;
    z-index: 1003;
  }

  .ctx-item {
    display: block;
    width: 100%;
    padding: 6px 12px;
    border: none;
    border-radius: 4px;
    background: none;
    color: rgba(255, 255, 255, 0.8);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.05s;
  }

  .ctx-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .ctx-item.destructive {
    color: rgba(255, 120, 120, 0.9);
  }

  .ctx-divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.08);
    margin: 4px 8px;
  }
</style>
