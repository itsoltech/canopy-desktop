<script lang="ts">
  import { onMount } from 'svelte'
  import { match } from 'ts-pattern'
  import { RotateCw, Check, X } from 'lucide-svelte'
  import { openDiffTab } from '../../lib/stores/tabs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
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
      .with('added', () => 'status-added')
      .with('modified', () => 'status-modified')
      .with('deleted', () => 'status-deleted')
      .with('renamed', () => 'status-renamed')
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
    const ok = window.confirm('Revert all changes to this file? This cannot be undone.')
    if (!ok) return
    try {
      await window.api.gitRevertFile(worktreePath, path)
      await refresh()
    } catch (err) {
      console.error('Failed to revert file:', err)
    }
  }
</script>

<div class="changes-panel">
  <div class="panel-header">
    <span class="panel-title">Changes</span>
    <button class="refresh-btn" onclick={refresh} title="Refresh" disabled={loading}>
      <span class:spinning={loading} style="display:flex">
        <RotateCw size={13} />
      </span>
    </button>
  </div>

  {#if totalFiles > 0}
    <div class="summary-line">
      <span class="summary-text">
        {totalFiles} file{totalFiles !== 1 ? 's' : ''} changed,
        <span class="stat-add">+{totalAdditions}</span>
        <span class="stat-del">&minus;{totalDeletions}</span>
      </span>
      <span class="summary-bar">
        {#if totalAdditions > 0}
          <span class="summary-bar-add" style="width: {summaryBarAddWidth}px"></span>
        {/if}
        {#if totalDeletions > 0}
          <span class="summary-bar-del" style="width: {summaryBarDelWidth}px"></span>
        {/if}
      </span>
    </div>
  {/if}

  <div class="filter-section">
    <input
      class="filter-input"
      type="text"
      placeholder="Filter files..."
      bind:value={filterQuery}
    />
    <div class="status-filters">
      <button
        class="filter-btn"
        class:filter-active={statusFilter === 'all'}
        onclick={() => (statusFilter = 'all')}>All</button
      >
      <button
        class="filter-btn"
        class:filter-active={statusFilter === 'added'}
        onclick={() => (statusFilter = statusFilter === 'added' ? 'all' : 'added')}>A</button
      >
      <button
        class="filter-btn"
        class:filter-active={statusFilter === 'modified'}
        onclick={() => (statusFilter = statusFilter === 'modified' ? 'all' : 'modified')}>M</button
      >
      <button
        class="filter-btn"
        class:filter-active={statusFilter === 'deleted'}
        onclick={() => (statusFilter = statusFilter === 'deleted' ? 'all' : 'deleted')}>D</button
      >
    </div>
  </div>

  {#if filteredFiles.length > 0}
    <ul class="file-list">
      {#each filteredFiles as file (file.path)}
        <li
          class="file-item"
          class:file-visible={visibleFilePath === file.path}
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
          <span class="status-icon {statusClass(file.status)}">{statusIcon(file.status)}</span>
          <span class="file-path">
            <span class="dir">{dirname(file.path)}</span><span class="name"
              >{basename(file.path)}</span
            >
          </span>
          <span class="stats">
            {#if file.additions > 0}<span class="stat-add">+{file.additions}</span>{/if}
            {#if file.deletions > 0}<span class="stat-del">&minus;{file.deletions}</span>{/if}
          </span>
          {#if hoveredPath === file.path}
            <span class="actions">
              <button
                class="action-btn stage"
                onclick={(e) => handleStage(e, file.path)}
                title="Stage"><Check size={12} /></button
              >
              <button
                class="action-btn revert"
                onclick={(e) => handleRevert(e, file.path)}
                title="Revert"><X size={12} /></button
              >
            </span>
          {/if}
        </li>
      {/each}
    </ul>
  {:else if !loading}
    <div class="empty-state">
      <span class="empty-text">
        {#if filterQuery || statusFilter !== 'all'}
          No matching files
        {:else}
          No uncommitted changes
        {/if}
      </span>
    </div>
  {/if}
</div>

<style>
  .changes-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    flex-shrink: 0;
  }

  .panel-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--c-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .refresh-btn {
    background: none;
    border: none;
    color: var(--c-text-muted);
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .refresh-btn:hover {
    color: var(--c-text);
    background: var(--c-hover);
  }

  .refresh-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .spinning {
    animation: spin 1s linear infinite;
  }

  @media (prefers-reduced-motion: reduce) {
    .spinning {
      animation: none;
    }
  }

  .summary-line {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 12px 6px 12px;
    flex-shrink: 0;
    overflow: hidden;
  }

  .summary-text {
    font-size: 11px;
    color: var(--c-text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .summary-bar {
    display: flex;
    align-items: stretch;
    height: 6px;
    border-radius: 2px;
    overflow: hidden;
    flex: 1;
    min-width: 20px;
  }

  .summary-bar-add {
    height: 100%;
    background: var(--diff-add-fg);
    display: block;
  }

  .summary-bar-del {
    height: 100%;
    background: var(--diff-delete-fg);
    display: block;
  }

  .filter-section {
    padding: 0 12px 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex-shrink: 0;
  }

  .filter-input {
    background: var(--c-bg);
    border: 1px solid var(--c-border-subtle);
    border-radius: 4px;
    color: var(--c-text);
    font-size: 11px;
    padding: 3px 8px;
    width: 100%;
    outline: none;
    font-family: var(--font-mono, monospace);
    box-sizing: border-box;
  }

  .filter-input:focus {
    border-color: var(--c-accent);
  }

  .filter-input::placeholder {
    color: var(--c-text-faint);
  }

  .status-filters {
    display: flex;
    gap: 2px;
  }

  .filter-btn {
    background: none;
    border: 1px solid var(--c-border-subtle);
    color: var(--c-text-muted);
    cursor: pointer;
    font-size: 10px;
    font-weight: 600;
    padding: 1px 8px;
    border-radius: 3px;
    font-family: var(--font-mono, monospace);
  }

  .filter-btn:hover {
    color: var(--c-text);
    background: var(--c-active);
  }

  .filter-btn.filter-active {
    background: var(--c-accent);
    color: var(--c-bg);
    border-color: var(--c-accent);
  }

  .file-list {
    padding: 0;
    margin: 0;
    overflow-y: auto;
    flex: 1;
  }

  .file-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px 3px 12px;
    cursor: pointer;
    font-size: 12px;
    line-height: 1.4;
    position: relative;
  }

  .file-item:hover {
    background: var(--c-hover);
  }

  .file-item.file-visible {
    background: var(--c-border-subtle);
    border-left: 2px solid var(--c-accent);
    padding-left: 10px;
  }

  .status-icon {
    flex-shrink: 0;
    width: 16px;
    text-align: center;
    font-weight: 600;
    font-size: 11px;
    font-family: var(--font-mono, monospace);
  }

  .status-added {
    color: var(--c-success);
  }

  .status-modified {
    color: var(--c-warning);
  }

  .status-deleted {
    color: var(--c-danger);
  }

  .status-renamed {
    color: var(--c-accent);
  }

  .file-path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dir {
    color: var(--c-text-muted);
  }

  .name {
    color: var(--c-text);
  }

  .stats {
    flex-shrink: 0;
    font-size: 11px;
    font-family: var(--font-mono, monospace);
    display: flex;
    gap: 4px;
  }

  .stat-add {
    color: var(--diff-add-fg);
  }

  .stat-del {
    color: var(--diff-delete-fg);
  }

  .actions {
    display: flex;
    gap: 2px;
    flex-shrink: 0;
    width: 44px;
    justify-content: flex-end;
  }

  .action-btn {
    background: none;
    border: none;
    cursor: pointer;
    padding: 1px 3px;
    border-radius: 3px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .action-btn.stage {
    color: var(--c-success);
  }

  .action-btn.stage:hover {
    background: var(--diff-add-bg);
  }

  .action-btn.revert {
    color: var(--c-danger);
  }

  .action-btn.revert:hover {
    background: var(--diff-delete-bg);
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: 16px;
  }

  .empty-text {
    font-size: 12px;
    color: var(--c-text-muted);
  }
</style>
