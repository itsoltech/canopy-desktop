<script lang="ts">
  import { ChevronRight, Folder, FolderOpen, File } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import { fileTree } from '../../lib/stores/fileTree.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { openFile } from '../../lib/stores/tabs.svelte'

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
    const unsub = window.api.onGitChanged(() => {
      fileTree.refreshAll(repoRoot)
    })
    return unsub
  })

  function getStatusColor(relativePath: string): string | null {
    const status = fileTree.gitFileStatus.get(relativePath)
    if (!status) return null
    switch (status) {
      case 'M':
        return 'rgba(230, 180, 80, 0.9)'
      case 'A':
        return 'rgba(80, 200, 120, 0.9)'
      case 'D':
        return 'rgba(224, 80, 80, 0.9)'
      case '?':
        return 'rgba(120, 190, 120, 0.6)'
      case 'R':
        return 'rgba(130, 170, 255, 0.9)'
      default:
        return 'rgba(230, 180, 80, 0.7)'
    }
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

  function contextShowInFinder(): void {
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

<CollapsibleSection title="FILES" sectionKey="files" borderTop>
  {#if fileTree.rootPath && fileTree.expandedDirs[fileTree.rootPath]}
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

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
{#if contextMenu}
  <div class="ctx-overlay" use:portal onclick={closeContextMenu}>
    <div
      class="ctx-menu"
      style="left: {contextMenu.x}px; top: {contextMenu.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <button class="ctx-item" onclick={contextShowInFinder}>Show in Finder</button>
      <button class="ctx-item" onclick={contextCopyPath}>Copy path</button>
      <button class="ctx-item" onclick={contextCopyName}>Copy name</button>
    </div>
  </div>
{/if}

<style>
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
    background: rgba(255, 255, 255, 0.06);
  }

  .tree-row.selected {
    background: rgba(255, 255, 255, 0.1);
  }

  .chevron {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    color: rgba(255, 255, 255, 0.3);
    transition: transform 0.12s ease;
    transform: rotate(0deg);
  }

  .chevron.open {
    transform: rotate(90deg);
  }

  .tree-row :global(.icon) {
    flex-shrink: 0;
    color: rgba(255, 255, 255, 0.35);
  }

  .tree-row :global(.folder-icon) {
    color: rgba(180, 160, 120, 0.7);
  }

  .entry-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: rgba(255, 255, 255, 0.7);
  }

  .empty-state {
    padding: 8px 12px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.3);
  }

  .ctx-overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
  }

  .ctx-menu {
    position: fixed;
    min-width: 180px;
    background: rgba(40, 40, 40, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    padding: 4px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(12px);
    z-index: 1002;
  }

  .ctx-item {
    display: block;
    width: 100%;
    padding: 6px 12px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.8);
    background: none;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    text-align: left;
  }

  .ctx-item:hover {
    background: rgba(255, 255, 255, 0.1);
  }
</style>
