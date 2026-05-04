<script lang="ts">
  import { match } from 'ts-pattern'
  import { ChevronRight, Folder, FolderOpen, RotateCw, Plus } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import FileTypeIcon from './_partials/FileTypeIcon.svelte'
  import { fileTree } from '../../lib/stores/fileTree.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { openFile } from '../../lib/stores/tabs.svelte'
  import { fileManagerLabel } from '../../lib/platform'
  import { prompt, confirm } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'

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
    let statusTimer: ReturnType<typeof setTimeout> | null = null
    const unsub = window.api.onFilesChanged((payload) => {
      if (payload.repoRoot !== wt) return
      fileTree.applyFileEvents(payload.events)
      const repoRoot = workspaceState.repoRoot
      if (!repoRoot) return
      if (statusTimer) clearTimeout(statusTimer)
      statusTimer = setTimeout(() => {
        statusTimer = null
        fileTree.refreshGitStatus(repoRoot)
      }, 150)
    })
    return () => {
      if (statusTimer) clearTimeout(statusTimer)
      unsub()
    }
  })

  function getStatus(relativePath: string): string | null {
    return fileTree.gitFileStatus.get(relativePath) ?? null
  }

  function statusLetter(status: string): string {
    return match(status)
      .with('?', () => 'U')
      .otherwise((s) => s.toUpperCase())
  }

  function statusTone(status: string): string {
    return match(status)
      .with('M', () => 'text-warning-text')
      .with('A', () => 'text-success-text')
      .with('D', () => 'text-danger-text')
      .with('R', () => 'text-warning-text')
      .with('?', () => 'text-accent-text')
      .otherwise(() => 'text-warning-text')
  }

  function folderHasChanges(relPath: string): boolean {
    if (!relPath) return fileTree.gitFileStatus.size > 0
    const prefix = relPath + '/'
    for (const key of fileTree.gitFileStatus.keys()) {
      if (key.startsWith(prefix)) return true
    }
    return false
  }

  function isIgnored(status: string | null): boolean {
    return status === '!'
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

  function validateNewName(value: string): string | null {
    const trimmed = value.trim()
    if (!trimmed) return 'Name is required'
    if (trimmed.startsWith('/')) return 'Use a relative path'
    if (trimmed.split('/').includes('..')) return 'Path cannot contain ".."'
    return null
  }

  async function createInDir(dirAbsPath: string): Promise<void> {
    const dirLabel =
      dirAbsPath === fileTree.rootPath
        ? '/'
        : dirAbsPath.replace((fileTree.rootPath ?? '') + '/', '')
    const result = await prompt({
      title: `New file or folder in ${dirLabel}`,
      placeholder: 'name.ts  or  folder/',
      submitLabel: 'Create',
      validate: validateNewName,
    })
    if (!result) return

    const name = result.value.trim()
    const isFolder = name.endsWith('/')
    const cleaned = isFolder ? name.replace(/\/+$/, '') : name
    const target = `${dirAbsPath}/${cleaned}`

    try {
      if (isFolder) {
        await window.api.mkdir(target)
      } else {
        await window.api.createFile(target)
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      if (msg.includes('EEXIST')) {
        await confirm({
          title: 'Already exists',
          message: `"${cleaned}" already exists in this folder.`,
          confirmLabel: 'OK',
        })
      } else {
        addToast(`Create failed: ${msg}`)
      }
      return
    }

    // Make sure the parent (and any intermediate dirs) are expanded so the
    // new entry is visible without the user having to click around.
    const segments = cleaned.split('/').slice(0, -1)
    let walked = dirAbsPath
    await fileTree.expandDir(walked)
    for (const segment of segments) {
      walked = `${walked}/${segment}`
      await fileTree.expandDir(walked)
    }

    if (!isFolder) {
      const wt = workspaceState.selectedWorktreePath
      if (wt) openFile(target, wt)
    }
  }

  async function handleCreateRoot(e: MouseEvent): Promise<void> {
    e.stopPropagation()
    if (!fileTree.rootPath) return
    await createInDir(fileTree.rootPath)
  }

  async function handleCreateInRow(e: MouseEvent, dirAbsPath: string): Promise<void> {
    e.stopPropagation()
    await createInDir(dirAbsPath)
  }
</script>

{#snippet headerActions()}
  <span class="inline-flex items-center gap-0.5 -my-1">
    <button
      class="inline-flex items-center justify-center size-5 bg-transparent border-0 rounded-sm text-text-faint cursor-pointer transition-colors duration-fast enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-40 disabled:cursor-default"
      onclick={handleCreateRoot}
      disabled={!fileTree.rootPath}
      title="New file or folder"
      aria-label="New file or folder"
    >
      <Plus size={12} />
    </button>
    <button
      class="inline-flex items-center justify-center size-5 bg-transparent border-0 rounded-sm text-text-faint cursor-pointer transition-colors duration-fast enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-40 disabled:cursor-default"
      onclick={handleRefresh}
      disabled={refreshing || !fileTree.rootPath}
      title="Refresh file list"
      aria-label="Refresh file list"
    >
      <RotateCw
        size={12}
        class={refreshing ? 'animate-spin-slow motion-reduce:animate-none' : ''}
      />
    </button>
  </span>
{/snippet}

<CollapsibleSection title="FILES" sectionKey="files" borderTop headerExtra={headerActions}>
  {#if fileTree.rootPath && fileTree.expandedDirs[fileTree.rootPath]}
    <div class="flex flex-col">
      {#snippet renderEntries(entries: DirEntry[], parentPath: string, depth: number)}
        {#each entries as entry (entry.name)}
          {@const absPath = parentPath + '/' + entry.name}
          {@const isExpanded = !!fileTree.expandedDirs[absPath]}
          {@const relPath = getRelativePath(absPath)}
          {@const status = getStatus(relPath)}
          {@const ignored = isIgnored(status)}
          {@const isSelected = fileTree.selectedFilePath === absPath}
          {#if entry.isDirectory}
            {@const hasChanges = folderHasChanges(relPath)}
            <div
              class="group relative flex items-center w-full transition-colors duration-fast hover:bg-hover"
              class:bg-active={isSelected}
              class:text-text-faint={ignored}
              class:italic={ignored}
            >
              <button
                class="flex-1 min-w-0 flex items-center gap-2 pr-1 h-7 bg-transparent border-0 cursor-pointer text-sm text-left transition-colors duration-fast"
                class:text-text={!ignored}
                style:padding-left="{12 + depth * 12}px"
                onclick={() => fileTree.toggleDir(absPath)}
                oncontextmenu={(e) => handleContextMenu(e, absPath)}
                title={entry.name}
              >
                <span
                  class="inline-flex items-center justify-center size-3 flex-shrink-0 text-text-faint transition-transform duration-fast ease-std"
                  class:rotate-90={isExpanded}
                >
                  <ChevronRight size={12} />
                </span>
                {#if isExpanded}
                  <FolderOpen size={14} class="flex-shrink-0 text-text-muted" />
                {:else}
                  <Folder size={14} class="flex-shrink-0 text-text-muted" />
                {/if}
                <span class="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
                  {entry.name}
                </span>
                {#if hasChanges && !ignored}
                  <span
                    class="ml-auto size-1.5 rounded-full bg-accent-text/70 flex-shrink-0"
                    aria-hidden="true"
                    title="Contains changes"
                  ></span>
                {/if}
              </button>
              <button
                class="inline-flex items-center justify-center size-5 mr-3 ml-1 bg-transparent border-0 rounded-sm text-text-faint cursor-pointer flex-shrink-0 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 transition-opacity duration-fast hover:text-text hover:bg-hover-strong"
                onclick={(e) => handleCreateInRow(e, absPath)}
                title={`New file or folder in ${entry.name}`}
                aria-label={`New file or folder in ${entry.name}`}
              >
                <Plus size={11} />
              </button>
            </div>
            {#if isExpanded && fileTree.expandedDirs[absPath]}
              {@render renderEntries(fileTree.expandedDirs[absPath], absPath, depth + 1)}
            {/if}
          {:else}
            <button
              class="group flex items-center gap-2 pr-3 h-7 bg-transparent border-0 cursor-pointer text-sm text-left w-full transition-colors duration-fast hover:bg-hover"
              class:bg-active={isSelected}
              class:text-text={!ignored}
              class:text-text-faint={ignored}
              class:italic={ignored}
              style:padding-left="{12 + depth * 12 + 12}px"
              onclick={() => handleFileClick(absPath)}
              oncontextmenu={(e) => handleContextMenu(e, absPath)}
              title={entry.name}
            >
              <FileTypeIcon name={entry.name} size={13} />
              <span class="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
                {entry.name}
              </span>
              {#if status && status !== '!'}
                <span
                  class="ml-auto text-2xs font-mono font-semibold leading-none flex-shrink-0 {statusTone(
                    status,
                  )}"
                  aria-hidden="true"
                  title={`Git status: ${status}`}
                >
                  {statusLetter(status)}
                </span>
              {/if}
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
      class="fixed min-w-45 bg-bg-overlay border border-border rounded-md p-1 shadow-menu backdrop-blur-md z-popover"
      role="menu"
      style="left: {contextMenu.x}px; top: {contextMenu.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <!-- eslint-disable-next-line svelte/no-autofocus -->
      <button
        class="block w-full px-2.5 py-1.5 text-sm text-text bg-transparent border-0 rounded-sm cursor-pointer text-left font-inherit transition-colors duration-fast hover:bg-hover"
        role="menuitem"
        autofocus
        onclick={contextRevealInFileManager}
      >
        {fileManagerLabel()}
      </button>
      <button
        class="block w-full px-2.5 py-1.5 text-sm text-text bg-transparent border-0 rounded-sm cursor-pointer text-left font-inherit transition-colors duration-fast hover:bg-hover"
        role="menuitem"
        onclick={contextCopyPath}>Copy path</button
      >
      <button
        class="block w-full px-2.5 py-1.5 text-sm text-text bg-transparent border-0 rounded-sm cursor-pointer text-left font-inherit transition-colors duration-fast hover:bg-hover"
        role="menuitem"
        onclick={contextCopyName}>Copy name</button
      >
    </div>
  </div>
{/if}
