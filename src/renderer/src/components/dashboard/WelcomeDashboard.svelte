<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte'
  import { openWorkspace } from '../../lib/stores/workspace.svelte'
  import { confirm, prompt } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import WelcomeEmpty from './_partials/WelcomeEmpty.svelte'
  import WelcomeContextMenu from './_partials/WelcomeContextMenu.svelte'
  import WelcomeRecents from './_partials/WelcomeRecents.svelte'

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
  const modKey = isMac ? '⌘' : 'Ctrl'

  let workspaces = $state<WorkspaceRow[]>([])
  let filter = $state('')
  let selectedIndex = $state(0)
  let contextMenu = $state<{ x: number; y: number; workspace: WorkspaceRow } | null>(null)
  let disposed = false

  let filterInputEl: HTMLInputElement | undefined = $state()
  let openFolderBtnEl: HTMLButtonElement | undefined = $state()
  let listEl: HTMLDivElement | undefined = $state()

  const filteredWorkspaces = $derived.by(() => {
    const q = filter.trim().toLowerCase()
    if (!q) return workspaces
    return workspaces.filter(
      (w) =>
        w.name.toLowerCase().includes(q) ||
        w.path.toLowerCase().includes(q) ||
        (w.cached_branch?.toLowerCase().includes(q) ?? false),
    )
  })

  const showFilter = $derived(workspaces.length > 4)

  $effect(() => {
    void filteredWorkspaces
    if (selectedIndex >= filteredWorkspaces.length) selectedIndex = 0
  })

  onMount(async () => {
    try {
      workspaces = await window.api.listWorkspaces(20)
    } catch (err) {
      addToast(
        `Failed to load recent workspaces: ${err instanceof Error ? err.message : String(err)}`,
      )
    }

    await tick()
    if (workspaces.length === 0) {
      openFolderBtnEl?.focus()
    } else {
      focusRow(0)
    }

    for (const ws of workspaces.slice(0, 3)) {
      if (disposed) break
      if (ws.is_git_repo) {
        try {
          const fresh = await window.api.refreshWorkspaceGitStatus(ws.id, ws.path)
          if (fresh && !disposed) {
            const idx = workspaces.findIndex((w) => w.id === fresh.id)
            if (idx !== -1) workspaces[idx] = fresh
          }
        } catch {
          // path may no longer exist — ignore
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

  async function removeWorkspace(ws: WorkspaceRow): Promise<void> {
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

  async function ctxRemove(): Promise<void> {
    if (!contextMenu) return
    const ws = contextMenu.workspace
    closeContextMenu()
    await removeWorkspace(ws)
  }

  function focusRow(idx: number): void {
    listEl?.querySelectorAll<HTMLElement>('[data-row]')[idx]?.focus()
  }

  function selectAt(idx: number, refocus: boolean): void {
    const len = filteredWorkspaces.length
    if (len === 0) return
    selectedIndex = Math.max(0, Math.min(len - 1, idx))
    if (refocus) focusRow(selectedIndex)
  }

  const NAV_DELTA: Record<string, number> = {
    ArrowDown: 1,
    ArrowUp: -1,
    PageDown: 5,
    PageUp: -5,
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (contextMenu && e.key === 'Escape') {
      e.preventDefault()
      closeContextMenu()
      return
    }

    const t = e.target
    const onFilter = t === filterInputEl
    const onRow = t instanceof HTMLElement && t.hasAttribute('data-row')
    const inOtherInput =
      t instanceof HTMLElement &&
      !onFilter &&
      (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)
    if (inOtherInput) return

    if (e.key === 'Escape') {
      if (filter !== '') {
        e.preventDefault()
        filter = ''
        return
      }
      if (onFilter) {
        e.preventDefault()
        focusRow(selectedIndex)
        return
      }
    }

    if (e.key === '/' && !onFilter && showFilter) {
      e.preventDefault()
      filterInputEl?.focus()
      filterInputEl?.select()
      return
    }

    // Quick-search: printable key on a row jumps to filter (only when filter is empty).
    const printable =
      e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey && /\S/.test(e.key)
    if (onRow && showFilter && filter === '' && printable) {
      e.preventDefault()
      filter = e.key
      filterInputEl?.focus()
      void tick().then(() => {
        if (filterInputEl) {
          filterInputEl.selectionStart = filterInputEl.selectionEnd = filter.length
        }
      })
      return
    }

    // Navigation keys are scoped to filter/row so Enter on action buttons activates them.
    if (!onFilter && !onRow) return
    if (filteredWorkspaces.length === 0) return

    const delta = NAV_DELTA[e.key]
    if (delta !== undefined) {
      e.preventDefault()
      selectAt(selectedIndex + delta, !onFilter)
      return
    }
    if (e.key === 'Home' && !onFilter) {
      e.preventDefault()
      selectAt(0, true)
      return
    }
    if (e.key === 'End' && !onFilter) {
      e.preventDefault()
      selectAt(filteredWorkspaces.length - 1, true)
      return
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      handleOpen(filteredWorkspaces[selectedIndex])
      return
    }
    if (e.key === 'Backspace' && (e.metaKey || e.ctrlKey) && !onFilter) {
      e.preventDefault()
      const ws = filteredWorkspaces[selectedIndex]
      if (ws) void removeWorkspace(ws)
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if contextMenu}
  <WelcomeContextMenu
    x={contextMenu.x}
    y={contextMenu.y}
    onClose={closeContextMenu}
    onReveal={ctxRevealInFileManager}
    onCopyPath={ctxCopyPath}
    onRemove={ctxRemove}
  />
{/if}

<div class="flex items-start justify-center h-full overflow-y-auto py-12 px-5">
  <div class="w-full max-w-160 flex flex-col gap-6">
    {#if workspaces.length === 0}
      <WelcomeEmpty
        {modKey}
        onOpenFolder={handleOpenFolder}
        onOpenFromPath={handleOpenFromPath}
        bind:openFolderBtnEl
      />
    {:else}
      <WelcomeRecents
        {workspaces}
        {filteredWorkspaces}
        bind:filter
        {showFilter}
        {selectedIndex}
        {modKey}
        onOpen={handleOpen}
        onSelect={(i) => (selectedIndex = i)}
        onContextMenu={handleContextMenu}
        onOpenFolder={handleOpenFolder}
        onOpenFromPath={handleOpenFromPath}
        bind:filterInputEl
        bind:listEl
      />
    {/if}
  </div>
</div>
