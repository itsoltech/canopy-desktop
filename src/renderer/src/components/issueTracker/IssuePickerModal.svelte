<script lang="ts">
  import { onMount } from 'svelte'
  import { Search, X, Loader2, Send, Filter } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { getActivePtySessionId } from '../../lib/stores/tabs.svelte'
  import { createBranchFromIssue } from '../../lib/issueTracker/branchCreation'

  let { connectionId }: { connectionId: string } = $props()

  let allIssues: TrackerIssue[] = $state([])
  let loading = $state(true)
  let error = $state('')
  let searchQuery = $state('')
  let selectedIndex = $state(0)

  // Board selector
  let boards: TrackerBoard[] = $state([])
  let selectedBoardId = $state('')
  let loadingBoards = $state(true)

  // Status filter
  let availableStatuses: string[] = $derived.by(() => {
    const seen = new Set<string>()
    for (const i of allIssues) {
      if (i.status) seen.add(i.status)
    }
    return [...seen].sort()
  })
  let excludedStatuses: Set<string> = $state(new Set())
  let showFilters = $state(false)
  let assignedToMe = $state(true)

  let filteredIssues = $derived.by(() => {
    let result = allIssues
    if (excludedStatuses.size > 0) {
      result = result.filter((i) => !excludedStatuses.has(i.status))
    }
    if (searchQuery) {
      const q = searchQuery.toLowerCase()
      result = result.filter(
        (i) => i.key.toLowerCase().includes(q) || i.summary.toLowerCase().includes(q),
      )
    }
    return result
  })

  onMount(async () => {
    await loadBoards()
  })

  async function loadBoards(): Promise<void> {
    loadingBoards = true
    try {
      boards = await window.api.issueTrackerFetchBoards(connectionId)
      if (boards.length > 0) {
        selectedBoardId = boards[0].id
      }
    } catch {
      // no boards available
    }
    await fetchIssues()
  }

  async function onBoardChange(): Promise<void> {
    excludedStatuses = new Set()
    await fetchIssues()
  }

  async function fetchIssues(): Promise<void> {
    loading = true
    error = ''
    try {
      allIssues = await window.api.issueTrackerFetchIssues(connectionId, {
        assignedToMe,
        boardId: selectedBoardId || undefined,
      })
      // Auto-exclude done/closed on first load
      if (excludedStatuses.size === 0 && allIssues.length > 0) {
        const donePattern = /^(done|closed|resolved|cancelled|rejected|complete|gotowe|zamkni)/i
        for (const issue of allIssues) {
          if (donePattern.test(issue.status)) {
            excludedStatuses.add(issue.status)
          }
        }
        excludedStatuses = new Set(excludedStatuses)
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to fetch issues'
    } finally {
      loading = false
      loadingBoards = false
    }
  }

  function toggleStatus(status: string): void {
    if (excludedStatuses.has(status)) {
      excludedStatuses.delete(status)
    } else {
      excludedStatuses.add(status)
    }
    excludedStatuses = new Set(excludedStatuses)
  }

  function toggleAssignedToMe(): void {
    assignedToMe = !assignedToMe
    fetchIssues()
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      closeDialog()
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      selectedIndex = Math.min(selectedIndex + 1, filteredIssues.length - 1)
      scrollToSelected()
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      selectedIndex = Math.max(selectedIndex - 1, 0)
      scrollToSelected()
    } else if (e.key === 'Enter' && filteredIssues[selectedIndex]) {
      selectIssue(filteredIssues[selectedIndex])
    }
  }

  function scrollToSelected(): void {
    const el = document.querySelector('.issue-row.selected')
    el?.scrollIntoView({ block: 'nearest' })
  }

  async function selectIssue(issue: TrackerIssue): Promise<void> {
    closeDialog()
    await createBranchFromIssue(connectionId, issue)
  }

  function sendToTerminal(issue: TrackerIssue, e: MouseEvent): void {
    e.stopPropagation()
    const sessionId = getActivePtySessionId()
    if (!sessionId) return
    const text = `Issue: ${issue.key} - ${issue.summary}\n\n${issue.description || '(no description)'}`
    window.api.writePty(sessionId, text)
    closeDialog()
  }

  function priorityColor(priority: string): string {
    const p = priority.toLowerCase()
    if (p.includes('critical') || p.includes('highest')) return 'rgba(255, 100, 100, 0.8)'
    if (p.includes('high')) return 'rgba(255, 160, 100, 0.8)'
    if (p.includes('medium') || p.includes('normal')) return 'rgba(255, 210, 100, 0.8)'
    if (p.includes('low')) return 'rgba(100, 200, 255, 0.8)'
    return 'rgba(255, 255, 255, 0.5)'
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="dialog-overlay" onclick={closeDialog} role="presentation">
  <div
    class="picker-container"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    aria-label="Issue Picker"
  >
    <div class="picker-header">
      <h3 class="picker-title">Select Issue</h3>
      <div class="header-actions">
        <button
          class="filter-btn"
          class:active={showFilters}
          onclick={() => (showFilters = !showFilters)}
          title="Filters"
        >
          <Filter size={14} />
        </button>
        <button class="close-btn" onclick={closeDialog} aria-label="Close">
          <X size={16} />
        </button>
      </div>
    </div>

    {#if boards.length > 1}
      <div class="board-row">
        <select class="board-select" bind:value={selectedBoardId} onchange={onBoardChange}>
          {#each boards as board (board.id)}
            <option value={board.id}>{board.name}</option>
          {/each}
        </select>
      </div>
    {/if}

    {#if showFilters}
      <div class="filters-panel">
        <label class="filter-check">
          <input type="checkbox" checked={assignedToMe} onchange={toggleAssignedToMe} />
          <span>Only assigned to me</span>
        </label>
        {#if availableStatuses.length > 0}
          <div class="status-filters">
            {#each availableStatuses as status (status)}
              <button
                class="status-chip"
                class:active={!excludedStatuses.has(status)}
                class:excluded={excludedStatuses.has(status)}
                onclick={() => toggleStatus(status)}
              >
                {status}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <div class="search-row">
      <Search size={14} />
      <input
        class="search-input"
        bind:value={searchQuery}
        placeholder="Search by key or title..."
        oninput={() => (selectedIndex = 0)}
      />
    </div>

    <div class="issue-list">
      {#if loading}
        <div class="state-msg">
          <Loader2 size={16} class="spin" />
          <span>Loading issues...</span>
        </div>
      {:else if error}
        <div class="state-msg error">
          <span>{error}</span>
          <button class="retry-btn" onclick={fetchIssues}>Retry</button>
        </div>
      {:else if filteredIssues.length === 0}
        <div class="state-msg">No issues found</div>
      {:else}
        {#each filteredIssues as issue, i (issue.key)}
          <div
            class="issue-row"
            class:selected={i === selectedIndex}
            role="button"
            tabindex="0"
            onclick={() => selectIssue(issue)}
            onkeydown={(e) => {
              if (e.key === 'Enter') selectIssue(issue)
            }}
            onmouseenter={() => (selectedIndex = i)}
          >
            <span class="issue-key">{issue.key}</span>
            <span class="issue-summary">{issue.summary}</span>
            <span class="status-badge">{issue.status}</span>
            <span
              class="priority-dot"
              style="color: {priorityColor(issue.priority)}"
              title={issue.priority}
            >
              ●
            </span>
            <button
              class="send-btn"
              onclick={(e) => {
                e.stopPropagation()
                sendToTerminal(issue, e)
              }}
              title="Send to active terminal"
            >
              <Send size={12} />
            </button>
          </div>
        {/each}
      {/if}
    </div>

    <div class="picker-footer">
      <span class="hint">↑↓ navigate · Enter select · Esc close</span>
      <span class="count"
        >{filteredIssues.length} issue{filteredIssues.length !== 1 ? 's' : ''}</span
      >
    </div>
  </div>
</div>

<style>
  .dialog-overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 80px;
    background: rgba(0, 0, 0, 0.5);
  }

  .picker-container {
    width: 600px;
    max-height: 500px;
    display: flex;
    flex-direction: column;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 10px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
    overflow: hidden;
  }

  .picker-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px 10px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .picker-title {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.9);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .filter-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 6px;
    background: none;
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
  }

  .filter-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .filter-btn.active {
    background: rgba(116, 192, 252, 0.15);
    color: rgba(116, 192, 252, 0.9);
  }

  .board-row {
    padding: 6px 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .board-select {
    width: 100%;
    padding: 4px 8px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.3);
    color: #e0e0e0;
    font-size: 12px;
    font-family: inherit;
    outline: none;
    cursor: pointer;
  }

  .board-select:focus {
    border-color: rgba(116, 192, 252, 0.5);
  }

  .filters-panel {
    padding: 8px 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .filter-check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
  }

  .status-filters {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .status-chip {
    padding: 2px 8px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 12px;
    background: none;
    color: rgba(255, 255, 255, 0.4);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .status-chip:hover {
    border-color: rgba(255, 255, 255, 0.2);
    color: rgba(255, 255, 255, 0.6);
  }

  .status-chip.active {
    background: rgba(116, 192, 252, 0.15);
    border-color: rgba(116, 192, 252, 0.3);
    color: rgba(116, 192, 252, 0.9);
  }

  .status-chip.excluded {
    opacity: 0.4;
    text-decoration: line-through;
  }

  .close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 6px;
    background: none;
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
  }

  .close-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.8);
  }

  .search-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.4);
  }

  .search-input {
    flex: 1;
    border: none;
    background: none;
    color: rgba(255, 255, 255, 0.9);
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .search-input::placeholder {
    color: rgba(255, 255, 255, 0.3);
  }

  .issue-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }

  .state-msg {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 24px 16px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.5);
  }

  .state-msg.error {
    color: rgba(255, 120, 120, 0.8);
    flex-direction: column;
    gap: 12px;
  }

  .retry-btn {
    padding: 4px 12px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    background: none;
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .retry-btn:hover {
    background: rgba(255, 255, 255, 0.06);
  }

  .issue-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 16px;
    border: none;
    background: none;
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.05s;
  }

  .issue-row:hover,
  .issue-row.selected {
    background: rgba(255, 255, 255, 0.06);
  }

  .issue-key {
    flex-shrink: 0;
    font-weight: 600;
    color: rgba(116, 192, 252, 0.9);
    min-width: 80px;
  }

  .issue-summary {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-badge {
    flex-shrink: 0;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.08);
    font-size: 10px;
    color: rgba(255, 255, 255, 0.5);
  }

  .priority-dot {
    flex-shrink: 0;
    font-size: 8px;
    line-height: 1;
  }

  .send-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 4px;
    background: none;
    color: rgba(255, 255, 255, 0.3);
    cursor: pointer;
    flex-shrink: 0;
    opacity: 0;
    transition: opacity 0.1s;
  }

  .issue-row:hover .send-btn,
  .issue-row.selected .send-btn {
    opacity: 1;
  }

  .send-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(168, 130, 255, 0.9);
  }

  .picker-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .hint {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.3);
  }

  .count {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
  }

  :global(.spin) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
</style>
