<script lang="ts">
  import { onMount } from 'svelte'
  import { match } from 'ts-pattern'
  import { RotateCw, Check, X } from 'lucide-svelte'
  import { openDiffTab } from '../../lib/stores/tabs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import type { DiffFile, ParsedDiff } from '../../lib/types/diff'

  let {
    worktreePath,
    fileCount = $bindable(0),
  }: {
    worktreePath: string
    fileCount?: number
  } = $props()

  let diff = $state<ParsedDiff | null>(null)
  let loading = $state(false)
  let hoveredPath = $state<string | null>(null)
  let visibleFilePath = $derived(workspaceState.diffVisibleFile)

  // Filter state
  let filterQuery = $state('')
  let statusFilter = $state<'all' | 'added' | 'modified' | 'deleted'>('all')

  async function refresh(): Promise<void> {
    loading = true
    try {
      diff = await window.api.gitDiff(worktreePath)
    } catch {
      diff = null
    } finally {
      loading = false
    }
  }

  // Re-fetch when worktreePath changes (branch switch, worktree switch)
  $effect(() => {
    void worktreePath
    refresh()
  })

  onMount(() => {
    const unsubGit = window.api.onGitChanged(() => {
      refresh()
    })

    // React to filesystem changes so the changes list stays in sync without
    // waiting for git metadata events. Debounced 200ms to coalesce bursts.
    let fileRefreshTimer: ReturnType<typeof setTimeout> | null = null
    const unsubFileWatcher = window.api.onFilesChanged((payload) => {
      if (payload.repoRoot !== worktreePath) return
      if (fileRefreshTimer) clearTimeout(fileRefreshTimer)
      fileRefreshTimer = setTimeout(() => {
        fileRefreshTimer = null
        refresh()
      }, 200)
    })

    return () => {
      unsubGit()
      unsubFileWatcher()
      if (fileRefreshTimer != null) clearTimeout(fileRefreshTimer)
    }
  })

  // Derived filtered files
  let allFiles = $derived(diff?.files ?? [])

  let filteredFiles = $derived.by(() => {
    let result = allFiles
    if (filterQuery) {
      const q = filterQuery.toLowerCase()
      result = result.filter((f) => f.path.toLowerCase().includes(q))
    }
    if (statusFilter !== 'all') {
      result = result.filter((f) => f.status === statusFilter)
    }
    return result
  })

  // Summary stats
  let totalFiles = $derived(allFiles.length)
  let totalAdditions = $derived(allFiles.reduce((sum, f) => sum + f.additions, 0))
  let totalDeletions = $derived(allFiles.reduce((sum, f) => sum + f.deletions, 0))
  let totalChanges = $derived(totalAdditions + totalDeletions)

  let summaryBarAddWidth = $derived(
    totalChanges > 0 ? Math.round((totalAdditions / totalChanges) * 60) : 0,
  )
  let summaryBarDelWidth = $derived(totalChanges > 0 ? 60 - summaryBarAddWidth : 0)

  // Expose file count to parent via bindable prop
  $effect(() => {
    fileCount = totalFiles
  })

  function basename(filePath: string): string {
    const parts = filePath.split('/')
    return parts[parts.length - 1]
  }

  function dirname(filePath: string): string {
    const parts = filePath.split('/')
    if (parts.length <= 1) return ''
    return parts.slice(0, -1).join('/') + '/'
  }

  function statusIcon(status: DiffFile['status']): string {
    return match(status)
      .with('added', () => '+')
      .with('modified', () => 'M')
      .with('deleted', () => '\u2212')
      .with('renamed', () => '\u2192')
      .exhaustive()
  }

  function statusClass(status: DiffFile['status']): string {
    return match(status)
      .with('added', () => 'text-success')
      .with('modified', () => 'text-warning')
      .with('deleted', () => 'text-danger')
      .with('renamed', () => 'text-accent')
      .exhaustive()
  }

  function handleClick(file: DiffFile): void {
    openDiffTab(worktreePath, file.path)
  }

  async function handleStage(e: Event, path: string): Promise<void> {
    e.stopPropagation()
    try {
      await window.api.gitStageFile(worktreePath, path)
      await refresh()
    } catch (err) {
      console.error('Failed to stage file:', err)
    }
  }

  async function handleRevert(e: Event, path: string): Promise<void> {
    e.stopPropagation()
    const ok = await confirm({
      title: 'Revert File',
      message: `Revert all changes to "${path}"? This cannot be undone.`,
      confirmLabel: 'Revert',
      destructive: true,
    })
    if (!ok) return
    try {
      await window.api.gitRevertFile(worktreePath, path)
      await refresh()
    } catch (err) {
      console.error('Failed to revert file:', err)
    }
  }
</script>

<div class="flex flex-col h-full">
  <div class="flex items-center justify-between px-3 py-2 flex-shrink-0">
    <span class="text-xs font-semibold text-text-secondary uppercase tracking-caps-looser"
      >Changes</span
    >
    <button
      class="bg-transparent border-0 text-text-muted cursor-pointer px-1 py-0.5 rounded-md flex items-center justify-center enabled:hover:text-text enabled:hover:bg-hover disabled:opacity-50 disabled:cursor-default"
      onclick={refresh}
      title="Refresh"
      disabled={loading}
    >
      <RotateCw size={13} class={loading ? 'animate-spin-slow motion-reduce:animate-none' : ''} />
    </button>
  </div>

  {#if totalFiles > 0}
    <div class="flex items-center gap-2 px-3 pt-0.5 pb-2 flex-shrink-0 overflow-hidden">
      <span class="text-xs text-text-muted whitespace-nowrap flex-shrink-0">
        {totalFiles} file{totalFiles !== 1 ? 's' : ''} changed,
        <span class="text-diff-add-fg">+{totalAdditions}</span>
        <span class="text-diff-delete-fg">&minus;{totalDeletions}</span>
      </span>
      <span class="flex items-stretch h-1.5 rounded-xs overflow-hidden flex-1 min-w-5">
        {#if totalAdditions > 0}
          <span class="block h-full bg-diff-add-fg" style="width: {summaryBarAddWidth}px"></span>
        {/if}
        {#if totalDeletions > 0}
          <span class="block h-full bg-diff-delete-fg" style="width: {summaryBarDelWidth}px"></span>
        {/if}
      </span>
    </div>
  {/if}

  <div class="px-3 pb-2 flex flex-col gap-1.5 flex-shrink-0">
    <input
      class="bg-bg border border-border-subtle rounded-md text-text text-xs px-2 py-1 w-full outline-none font-mono box-border focus:border-accent placeholder:text-text-faint"
      type="text"
      placeholder="Filter files..."
      bind:value={filterQuery}
    />
    <div class="flex gap-1">
      {#each [['all', 'All', 'Show all files'], ['added', 'A', 'Filter added files'], ['modified', 'M', 'Filter modified files'], ['deleted', 'D', 'Filter deleted files']] as [val, label, aria] (val)}
        <button
          class="bg-transparent border border-border-subtle text-text-muted cursor-pointer text-2xs font-semibold px-2 py-0.5 rounded-sm font-mono"
          class:bg-accent={statusFilter === val}
          class:text-bg={statusFilter === val}
          class:border-accent={statusFilter === val}
          class:hover:text-text={statusFilter !== val}
          class:hover:bg-active={statusFilter !== val}
          aria-label={aria}
          aria-pressed={statusFilter === val}
          onclick={() =>
            (statusFilter =
              val === 'all' ? 'all' : statusFilter === val ? 'all' : (val as typeof statusFilter))}
        >
          {label}
        </button>
      {/each}
    </div>
  </div>

  {#if filteredFiles.length > 0}
    <ul class="p-0 m-0 overflow-y-auto flex-1">
      {#each filteredFiles as file (file.path)}
        {@const isVisibleFile = visibleFilePath === file.path}
        <li
          class="flex items-center gap-1.5 py-1 cursor-pointer text-xs leading-snug relative hover:bg-hover"
          class:bg-border-subtle={isVisibleFile}
          class:border-l-2={isVisibleFile}
          class:border-accent={isVisibleFile}
          class:pl-2={isVisibleFile}
          class:pr-2.5={true}
          class:pl-2.5={!isVisibleFile}
          role="button"
          tabindex="0"
          onclick={() => handleClick(file)}
          onkeydown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault()
              handleClick(file)
            }
          }}
          onpointerenter={() => (hoveredPath = file.path)}
          onpointerleave={() => (hoveredPath = null)}
        >
          <span
            class="flex-shrink-0 w-4 text-center font-semibold text-xs font-mono {statusClass(
              file.status,
            )}">{statusIcon(file.status)}</span
          >
          <span class="flex-1 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">
            <span class="text-text-muted">{dirname(file.path)}</span><span class="text-text"
              >{basename(file.path)}</span
            >
          </span>
          <span class="flex-shrink-0 text-xs font-mono flex gap-1">
            {#if file.additions > 0}<span class="text-diff-add-fg">+{file.additions}</span>{/if}
            {#if file.deletions > 0}<span class="text-diff-delete-fg">&minus;{file.deletions}</span
              >{/if}
          </span>
          {#if hoveredPath === file.path}
            <span class="flex gap-0.5 flex-shrink-0 w-11 justify-end">
              <button
                class="bg-transparent border-0 cursor-pointer px-0.5 py-px rounded-sm flex items-center justify-center text-success hover:bg-diff-add-bg"
                onclick={(e) => handleStage(e, file.path)}
                title="Stage"
                aria-label="Stage file"><Check size={12} /></button
              >
              <button
                class="bg-transparent border-0 cursor-pointer px-0.5 py-px rounded-sm flex items-center justify-center text-danger hover:bg-diff-delete-bg"
                onclick={(e) => handleRevert(e, file.path)}
                title="Revert"
                aria-label="Revert file"><X size={12} /></button
              >
            </span>
          {/if}
        </li>
      {/each}
    </ul>
  {:else if !loading}
    <div class="flex items-center justify-center h-full p-4">
      <span class="text-sm text-text-muted">
        {#if filterQuery || statusFilter !== 'all'}
          No matching files
        {:else}
          No uncommitted changes
        {/if}
      </span>
    </div>
  {/if}
</div>
