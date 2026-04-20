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
    // Stop propagation so clicking the button doesn't also toggle the section
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

  // Reset tree when worktree changes
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

  // Refresh on git changes
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

  // Refresh on file tree changes (filesystem watcher push events)
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
      .with('M', () => 'var(--c-warning-text)')
      .with('A', () => 'var(--c-success)')
      .with('D', () => 'var(--c-danger-text)')
      .with('?', () => 'var(--c-text-faint)')
      .with('R', () => 'var(--c-accent-text)')
      .otherwise(() => 'var(--c-warning-text)')
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

  // Move element to document.body to escape sidebar overflow/backdrop-filter
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
    class="refresh-btn"
    class:spinning={refreshing}
    onclick={handleRefresh}
    disabled={refreshing || !fileTree.rootPath}
    title="Refresh file list"
    aria-label="Refresh file list"
  >
    <RotateCw size={14} />
  </button>
{/snippet}

<CollapsibleSection title="FILES" sectionKey="files" borderTop headerExtra={headerRefreshButton}>
  {#if fileTree.rootPath && fileTree.expandedDirs[fileTree.rootPath]}
    <!-- TODO: add virtualization for large directory trees (>500 items) -->
    <div class="file-tree">
      {#snippet renderEntries(entries: DirEntry[], parentPath: string, depth: number)}
        {#each entries as entry (entry.name)}
          {@const absPath = parentPath + '/' + entry.name}
          {@const isExpanded = !!fileTree.expandedDirs[absPath]}
          {@const relPath = getRelativePath(absPath)}
          {@const statusColor = getStatusColor(relPath)}
          {@const isSelected = fileTree.selectedFilePath === absPath}
          {#if entry.isDirectory}
            <button
              class="tree-row"
              class:selected={isSelected}
              style:padding-left="{8 + depth * 16}px"
              onclick={() => fileTree.toggleDir(absPath)}
              oncontextmenu={(e) => handleContextMenu(e, absPath)}
            >
              <span class="chevron" class:open={isExpanded}>
                <ChevronRight size={12} />
              </span>
              {#if isExpanded}
                <FolderOpen size={14} class="icon folder-icon" />
              {:else}
                <Folder size={14} class="icon folder-icon" />
              {/if}
              <span class="entry-name" style:color={statusColor}>{entry.name}</span>
            </button>
            {#if isExpanded && fileTree.expandedDirs[absPath]}
              {@render renderEntries(fileTree.expandedDirs[absPath], absPath, depth + 1)}
            {/if}
          {:else}
            <button
              class="tree-row file-row"
              class:selected={isSelected}
              style:padding-left="{8 + depth * 16 + 16}px"
              onclick={() => handleFileClick(absPath)}
              oncontextmenu={(e) => handleContextMenu(e, absPath)}
            >
              <File size={13} class="icon file-icon" />
              <span class="entry-name" style:color={statusColor}>{entry.name}</span>
            </button>
          {/if}
        {/each}
      {/snippet}
      {@render renderEntries(fileTree.expandedDirs[fileTree.rootPath], fileTree.rootPath, 0)}
    </div>
  {:else}
    <div class="empty-state">No files</div>
  {/if}
</CollapsibleSection>

{#if contextMenu}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="ctx-overlay"
    use:portal
    onclick={closeContextMenu}
    onkeydown={(e) => {
      if (e.key === 'Escape') closeContextMenu()
    }}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="ctx-menu"
      role="menu"
      style="left: {contextMenu.x}px; top: {contextMenu.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <!-- eslint-disable-next-line svelte/no-autofocus -->
      <button class="ctx-item" role="menuitem" autofocus onclick={contextRevealInFileManager}>
        {fileManagerLabel()}
      </button>
      <button class="ctx-item" role="menuitem" onclick={contextCopyPath}>Copy path</button>
      <button class="ctx-item" role="menuitem" onclick={contextCopyName}>Copy name</button>
    </div>
  </div>
{/if}

<style>
  .refresh-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    margin: -4px 0;
    background: none;
    border: none;
    border-radius: 4px;
    color: var(--c-text-faint);
    cursor: pointer;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .refresh-btn:hover:not(:disabled) {
    background: var(--c-hover);
    color: var(--c-text-secondary);
  }

  .refresh-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .refresh-btn.spinning :global(svg) {
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .refresh-btn.spinning :global(svg) {
      animation: none;
    }
  }

  .file-tree {
    display: flex;
    flex-direction: column;
  }

  .tree-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    height: 24px;
    background: none;
    border: none;
    cursor: pointer;
    color: inherit;
    font-size: 12px;
    text-align: left;
    width: 100%;
  }

  .tree-row:hover {
    background: var(--c-hover);
  }

  .tree-row.selected {
    background: var(--c-hover-strong);
  }

  .chevron {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--c-text-faint);
    transition: transform 0.12s ease;
    transform: rotate(0deg);
  }

  .chevron.open {
    transform: rotate(90deg);
  }

  .tree-row :global(.icon) {
    flex-shrink: 0;
    color: var(--c-text-muted);
  }

  .tree-row :global(.folder-icon) {
    color: color-mix(in srgb, var(--c-blazing-text) 70%, transparent);
  }

  .entry-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--c-text);
  }

  .empty-state {
    padding: 8px 12px;
    font-size: 11px;
    color: var(--c-text-faint);
  }

  .ctx-overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
  }

  .ctx-menu {
    position: fixed;
    min-width: 180px;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 6px;
    padding: 4px;
    box-shadow: var(--shadow-menu);
    backdrop-filter: blur(12px);
    z-index: 1002;
  }

  .ctx-item {
    display: block;
    width: 100%;
    padding: 6px 10px;
    font-size: 12px;
    color: var(--c-text);
    background: none;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    text-align: left;
  }

  .ctx-item:hover {
    background: var(--c-hover-strong);
  }
</style>
