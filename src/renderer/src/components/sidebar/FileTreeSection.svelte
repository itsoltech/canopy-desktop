<script lang="ts">
  import { match } from 'ts-pattern'
  import { ChevronRight, Folder, FolderOpen, File, RotateCw } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import { fileTree } from '../../lib/stores/fileTree.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { openFile } from '../../lib/stores/tabs.svelte'
  import { fileManagerLabel } from '../../lib/platform'

  let refreshing = $state(false)

  async function handleRefresh(e: MouseEvent): Promise<void> {
    e.stopPropagation()
    if (refreshing) return
    refreshing = true
    try {
      await fileTree.refreshExpandedDirs()
    } finally {
      refreshing = false
    }
  }

  interface DirEntry {
    name: string
    isDirectory: boolean
    size: number
  }

  let contextMenu: { x: number; y: number; path: string } | null = $state(null)

  $effect(() => {
    const wt = workspaceState.selectedWorktreePath
    if (wt !== fileTree.rootPath) {
      fileTree.reset(wt)
      if (wt) {
        fileTree.expandDir(wt)
        if (workspaceState.repoRoot) {
          fileTree.refreshGitStatus(workspaceState.repoRoot)
        }
      }
    }
  })

  $effect(() => {
    const repoRoot = workspaceState.repoRoot
    if (!repoRoot) return
    const unsub = window.api.onGitChanged((info) => {
      if (info.repoRoot !== repoRoot) return
      if (info.changes.branch || info.changes.worktrees) {
        fileTree.refreshAll(repoRoot)
      } else {
        fileTree.refreshGitStatus(repoRoot)
      }
    })
    return unsub
  })

  $effect(() => {
    const wt = workspaceState.selectedWorktreePath
    if (!wt) return
    const unsub = window.api.onFilesChanged((payload) => {
      if (payload.repoRoot !== wt) return
      fileTree.applyFileEvents(payload.events)
    })
    return unsub
  })

  function getStatusColor(relativePath: string): string | null {
    const status = fileTree.gitFileStatus.get(relativePath)
    if (!status) return null
    return match(status)
      .with('M', () => 'var(--color-warning-text)')
      .with('A', () => 'var(--color-success)')
      .with('D', () => 'var(--color-danger-text)')
      .with('?', () => 'var(--color-text-faint)')
      .with('R', () => 'var(--color-accent-text)')
      .otherwise(() => 'var(--color-warning-text)')
  }

  function getRelativePath(absPath: string): string {
    if (!fileTree.rootPath) return absPath
    const rel = absPath.startsWith(fileTree.rootPath + '/')
      ? absPath.slice(fileTree.rootPath.length + 1)
      : absPath
    return rel
  }

  function handleFileClick(absPath: string): void {
    fileTree.selectFile(absPath)
    const wt = workspaceState.selectedWorktreePath
    if (wt) openFile(absPath, wt)
  }

  function handleContextMenu(e: MouseEvent, absPath: string): void {
    e.preventDefault()
    contextMenu = { x: e.clientX, y: e.clientY, path: absPath }
  }

  function portal(node: HTMLElement): { destroy(): void } {
    document.body.appendChild(node)
    return { destroy: () => node.remove() }
  }

  function closeContextMenu(): void {
    contextMenu = null
  }

  function contextRevealInFileManager(): void {
    if (contextMenu) {
      window.api.showInFolder(contextMenu.path)
      closeContextMenu()
    }
  }

  function contextCopyPath(): void {
    if (contextMenu) {
      navigator.clipboard.writeText(contextMenu.path)
      closeContextMenu()
    }
  }

  function contextCopyName(): void {
    if (contextMenu) {
      const name = contextMenu.path.split('/').pop() ?? contextMenu.path
      navigator.clipboard.writeText(name)
      closeContextMenu()
    }
  }
</script>

{#snippet headerRefreshButton()}
  <button
    class="flex items-center justify-center w-6 h-6 -my-1 bg-transparent border-0 rounded-md text-text-faint cursor-pointer transition-colors duration-fast enabled:hover:bg-hover enabled:hover:text-text-secondary disabled:opacity-40 disabled:cursor-default"
    onclick={handleRefresh}
    disabled={refreshing || !fileTree.rootPath}
    title="Refresh file list"
    aria-label="Refresh file list"
  >
    <RotateCw size={14} class={refreshing ? 'animate-spin-slow motion-reduce:animate-none' : ''} />
  </button>
{/snippet}

<CollapsibleSection title="FILES" sectionKey="files" borderTop headerExtra={headerRefreshButton}>
  {#if fileTree.rootPath && fileTree.expandedDirs[fileTree.rootPath]}
    <div class="flex flex-col">
      {#snippet renderEntries(entries: DirEntry[], parentPath: string, depth: number)}
        {#each entries as entry (entry.name)}
          {@const absPath = parentPath + '/' + entry.name}
          {@const isExpanded = !!fileTree.expandedDirs[absPath]}
          {@const relPath = getRelativePath(absPath)}
          {@const statusColor = getStatusColor(relPath)}
          {@const isSelected = fileTree.selectedFilePath === absPath}
          {#if entry.isDirectory}
            <button
              class="flex items-center gap-1 py-0.5 pr-2 h-6 bg-transparent border-0 cursor-pointer text-text-secondary text-sm text-left w-full hover:bg-hover"
              class:bg-hover-strong={isSelected}
              style:padding-left="{8 + depth * 16}px"
              onclick={() => fileTree.toggleDir(absPath)}
              oncontextmenu={(e) => handleContextMenu(e, absPath)}
            >
              <span
                class="flex items-center flex-shrink-0 text-text-faint transition-transform duration-fast ease-std"
                class:rotate-90={isExpanded}
              >
                <ChevronRight size={12} />
              </span>
              {#if isExpanded}
                <FolderOpen size={14} class="flex-shrink-0 text-folder-icon" />
              {:else}
                <Folder size={14} class="flex-shrink-0 text-folder-icon" />
              {/if}
              <span
                class="overflow-hidden text-ellipsis whitespace-nowrap text-text"
                style:color={statusColor}>{entry.name}</span
              >
            </button>
            {#if isExpanded && fileTree.expandedDirs[absPath]}
              {@render renderEntries(fileTree.expandedDirs[absPath], absPath, depth + 1)}
            {/if}
          {:else}
            <button
              class="flex items-center gap-1 py-0.5 pr-2 h-6 bg-transparent border-0 cursor-pointer text-text-secondary text-sm text-left w-full hover:bg-hover"
              class:bg-hover-strong={isSelected}
              style:padding-left="{8 + depth * 16 + 16}px"
              onclick={() => handleFileClick(absPath)}
              oncontextmenu={(e) => handleContextMenu(e, absPath)}
            >
              <File size={13} class="flex-shrink-0 text-text-muted" />
              <span
                class="overflow-hidden text-ellipsis whitespace-nowrap text-text"
                style:color={statusColor}>{entry.name}</span
              >
            </button>
          {/if}
        {/each}
      {/snippet}
      {@render renderEntries(fileTree.expandedDirs[fileTree.rootPath], fileTree.rootPath, 0)}
    </div>
  {:else}
    <div class="px-3 py-2 text-xs text-text-faint">No files</div>
  {/if}
</CollapsibleSection>

{#if contextMenu}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-overlay"
    use:portal
    onclick={closeContextMenu}
    onkeydown={(e) => {
      if (e.key === 'Escape') closeContextMenu()
    }}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="fixed min-w-45 bg-bg-overlay border border-border rounded-lg p-1 shadow-menu backdrop-blur-md z-popover"
      role="menu"
      style="left: {contextMenu.x}px; top: {contextMenu.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <!-- eslint-disable-next-line svelte/no-autofocus -->
      <button
        class="block w-full px-2.5 py-1.5 text-sm text-text bg-transparent border-0 rounded-md cursor-pointer text-left font-inherit hover:bg-hover-strong"
        role="menuitem"
        autofocus
        onclick={contextRevealInFileManager}
      >
        {fileManagerLabel()}
      </button>
      <button
        class="block w-full px-2.5 py-1.5 text-sm text-text bg-transparent border-0 rounded-md cursor-pointer text-left font-inherit hover:bg-hover-strong"
        role="menuitem"
        onclick={contextCopyPath}>Copy path</button
      >
      <button
        class="block w-full px-2.5 py-1.5 text-sm text-text bg-transparent border-0 rounded-md cursor-pointer text-left font-inherit hover:bg-hover-strong"
        role="menuitem"
        onclick={contextCopyName}>Copy name</button
      >
    </div>
  </div>
{/if}
