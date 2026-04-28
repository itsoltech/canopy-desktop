<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { openWorkspace } from '../../lib/stores/workspace.svelte'
  import { confirm, prompt } from '../../lib/stores/dialogs.svelte'
  import { fileManagerLabel } from '../../lib/platform'

  interface WorkspaceRow {
    id: string
    path: string
    name: string
    is_git_repo: number
    last_opened: string | null
    cached_branch: string | null
    cached_dirty: number | null
    cached_ahead_behind: string | null
    cached_worktree_count: number | null
  }

  const isMac = navigator.userAgent.includes('Mac')
  let workspaces = $state<WorkspaceRow[]>([])
  let contextMenu = $state<{ x: number; y: number; workspace: WorkspaceRow } | null>(null)
  let disposed = false

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

    // Refresh a few recent repos sequentially so the welcome screen stays cheap to render.
    for (const ws of workspaces.slice(0, 3)) {
      if (disposed) break
      if (ws.is_git_repo) {
        try {
          const fresh = await window.api.refreshWorkspaceGitStatus(ws.id, ws.path)
          if (fresh && !disposed) {
            const idx = workspaces.findIndex((w) => w.id === fresh.id)
            if (idx !== -1) {
              workspaces[idx] = fresh
            }
          }
        } catch {
          // Path may no longer exist — ignore
        }
      }
    }
  })

  onDestroy(() => {
    disposed = true
  })

  function handleOpen(ws: WorkspaceRow): void {
    openWorkspace(ws.path)
  }

  async function handleOpenFolder(): Promise<void> {
    const path = await window.api.openFolder()
    if (path) openWorkspace(path)
  }

  async function handleOpenFromPath(): Promise<void> {
    const result = await prompt({
      title: 'Open from Path',
      placeholder: '/path/to/project',
      submitLabel: 'Open',
    })
    if (result) openWorkspace(result.value)
  }

  function handleContextMenu(e: MouseEvent, ws: WorkspaceRow): void {
    e.preventDefault()
    contextMenu = { x: e.clientX, y: e.clientY, workspace: ws }
  }

  function closeContextMenu(): void {
    contextMenu = null
  }

  async function ctxRevealInFileManager(): Promise<void> {
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
    const ws = contextMenu.workspace
    closeContextMenu()
    const ok = await confirm({
      title: 'Remove from Recent?',
      message: `Remove "${ws.name}" from your recent workspaces?`,
      details: ws.path,
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (!ok) return
    await window.api.removeWorkspace(ws.id)
    workspaces = workspaces.filter((w) => w.id !== ws.id)
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
  <div class="fixed inset-0 z-overlay" onclick={closeContextMenu}>
    <div
      class="fixed min-w-45 bg-bg-overlay border border-border rounded-xl shadow-ctx p-1 z-popover"
      style="left: {contextMenu.x}px; top: {contextMenu.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <button
        class="block w-full px-3 py-1.5 border-0 rounded-md bg-transparent text-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-active"
        onclick={ctxRevealInFileManager}>{fileManagerLabel()}</button
      >
      <button
        class="block w-full px-3 py-1.5 border-0 rounded-md bg-transparent text-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-active"
        onclick={ctxCopyPath}>Copy Path</button
      >
      <div class="h-px mx-2 my-1 bg-active"></div>
      <button
        class="block w-full px-3 py-1.5 border-0 rounded-md bg-transparent text-danger-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-active"
        onclick={ctxRemove}>Remove from Recent</button
      >
    </div>
  </div>
{/if}

<div class="flex items-start justify-center h-full overflow-y-auto py-15 px-5">
  <div class="w-full max-w-130">
    <h1 class="text-3xl font-bold text-text-faint tracking-caps text-center m-0 mb-10">Canopy</h1>

    {#if workspaces.length > 0}
      <h2 class="text-xs font-semibold tracking-caps-tight uppercase text-text-faint m-0 mb-2">
        Recent
      </h2>
      <div class="flex flex-col gap-0.5 mb-6">
        {#each workspaces as ws (ws.id)}
          <button
            class="block w-full px-3 py-2.5 border-0 rounded-lg bg-transparent text-inherit font-inherit text-inherit text-left cursor-pointer transition-colors duration-fast outline-none hover:bg-hover focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
            onclick={() => handleOpen(ws)}
            oncontextmenu={(e) => handleContextMenu(e, ws)}
          >
            <div class="flex items-baseline justify-between gap-3">
              <span
                class="text-md font-semibold text-text whitespace-nowrap overflow-hidden text-ellipsis"
                >{ws.name}</span
              >
              <span class="flex items-baseline gap-1.5 flex-shrink-0 text-sm">
                {#if ws.cached_branch}
                  <span class="text-text-muted">{ws.cached_branch}</span>
                {/if}
                {#if ws.cached_dirty === 1}
                  <span class="text-warning-text font-bold">*</span>
                {/if}
                {#if parseAheadBehind(ws.cached_ahead_behind)}
                  {@const ab = parseAheadBehind(ws.cached_ahead_behind)!}
                  {#if ab.ahead > 0}
                    <span class="text-success text-xs">{ab.ahead}&#x2191;</span>
                  {/if}
                  {#if ab.behind > 0}
                    <span class="text-warning-text text-xs">{ab.behind}&#x2193;</span>
                  {/if}
                {/if}
              </span>
            </div>
            <div class="flex items-baseline justify-between gap-3 mt-0.5">
              <span
                class="text-xs font-mono text-text-faint whitespace-nowrap overflow-hidden text-ellipsis"
                >{ws.path}</span
              >
              <span class="flex items-baseline gap-2 flex-shrink-0 text-xs text-text-faint">
                {#if ws.is_git_repo && ws.cached_worktree_count && ws.cached_worktree_count > 1}
                  <span>{ws.cached_worktree_count} worktrees</span>
                {/if}
                {#if ws.last_opened}
                  <span>{relativeTime(ws.last_opened)}</span>
                {/if}
              </span>
            </div>
          </button>
        {/each}
      </div>
    {/if}

    <div class="flex items-center gap-2">
      <button
        class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-hover text-text-secondary transition-colors duration-fast hover:bg-hover-strong focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
        onclick={handleOpenFolder}>Open Folder</button
      >
      <button
        class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-hover text-text-secondary transition-colors duration-fast hover:bg-hover-strong focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
        onclick={handleOpenFromPath}>Open from Path...</button
      >
      <span class="text-xs text-text-faint ml-1">{isMac ? 'Cmd' : 'Ctrl'}+O</span>
    </div>
  </div>
</div>
