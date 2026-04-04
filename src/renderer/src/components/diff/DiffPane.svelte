<script lang="ts">
  import { onMount } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { getAiSessions, focusSessionByPtyId } from '../../lib/stores/tabs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'

  interface DiffChange {
    type: 'add' | 'delete' | 'context'
    content: string
    oldLine?: number
    newLine?: number
  }

  interface DiffHunk {
    oldStart: number
    oldLines: number
    newStart: number
    newLines: number
    header: string
    changes: DiffChange[]
  }

  interface DiffFile {
    path: string
    oldPath?: string
    status: 'added' | 'modified' | 'deleted' | 'renamed'
    hunks: DiffHunk[]
    additions: number
    deletions: number
  }

  let {
    worktreePath,
    active,
  }: {
    worktreePath: string
    active: boolean
  } = $props()

  let files = $state<DiffFile[]>([])
  let loading = $state(false)
  let bodyEl: HTMLDivElement | undefined = $state()
  let paneEl: HTMLDivElement | undefined = $state()

  // Collapse state
  let collapsedFiles = new SvelteSet<string>()

  // Search state
  let searchQuery = $state('')
  let showSearch = $state(false)

  // Keyboard navigation state
  let focusedFileIndex = $state(-1)

  // Auto-refresh pulse state
  let justRefreshed = $state(false)

  // Hover state for copy button
  let hoveredFilePath = $state<string | null>(null)

  // Comment state
  let commentKey = $state<string | null>(null)
  let commentText = $state('')
  let commentFilePath = $state('')
  let commentLineNum = $state(0)

  async function refresh(): Promise<void> {
    loading = true
    try {
      const result = await window.api.gitDiff(worktreePath)
      files = result.files
    } catch {
      files = []
    } finally {
      loading = false
    }
  }

  function triggerPulse(): void {
    justRefreshed = true
    setTimeout(() => {
      justRefreshed = false
    }, 1000)
  }

  // Re-fetch when worktreePath changes (branch/worktree switch)
  $effect(() => {
    void worktreePath
    refresh()
  })

  // Scroll to file when diffScrollTarget changes
  $effect(() => {
    const target = workspaceState.diffScrollTarget
    if (!target) return
    const filePath = target.path
    // Delay to ensure tab switch + DOM layout completes
    setTimeout(() => {
      const el = document.getElementById(`diff-file-${filePath}`)
      if (el) {
        el.scrollIntoView({ behavior: 'smooth', block: 'start' })
      }
    }, 150)
  })

  onMount(() => {
    const unsubGit = window.api.onGitChanged(() => {
      refresh().then(() => triggerPulse())
    })

    // Track which file is currently visible via IntersectionObserver
    let observer: IntersectionObserver | null = null

    function setupObserver(): void {
      observer?.disconnect()
      if (!bodyEl) return
      observer = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            if (entry.isIntersecting) {
              // IntersectionObserverEntry.target is always Element, safe to cast
              const path = (entry.target as HTMLElement).dataset.filepath
              if (path) {
                workspaceState.diffScrollTarget = null
                window.dispatchEvent(
                  new CustomEvent('canopy:diff-visible-file', { detail: { filePath: path } }),
                )
              }
            }
          }
        },
        { root: bodyEl, rootMargin: '0px 0px -70% 0px', threshold: 0 },
      )
      bodyEl.querySelectorAll('.file-section').forEach((el) => observer!.observe(el))
    }

    // Re-setup observer when files change
    const unsubFiles = $effect.root(() => {
      $effect(() => {
        void files.length
        setTimeout(setupObserver, 100)
      })
      return () => observer?.disconnect()
    })

    return () => {
      unsubGit()
      unsubFiles()
      observer?.disconnect()
    }
  })

  let totalAdditions = $derived(files.reduce((sum, f) => sum + f.additions, 0))
  let totalDeletions = $derived(files.reduce((sum, f) => sum + f.deletions, 0))

  // Search matching
  let searchLower = $derived(searchQuery.toLowerCase())

  let matchCount = $derived.by(() => {
    if (!searchLower) return 0
    let count = 0
    for (const file of files) {
      for (const hunk of file.hunks) {
        for (const change of hunk.changes) {
          if (change.content.toLowerCase().includes(searchLower)) {
            count++
          }
        }
      }
    }
    return count
  })

  function lineMatchesSearch(content: string): boolean {
    if (!searchLower) return false
    return content.toLowerCase().includes(searchLower)
  }

  function highlightMatch(text: string): string {
    if (!searchLower) return escapeHtml(text)
    const escaped = escapeHtml(text)
    const queryEscaped = escapeHtml(searchLower)
    const regex = new RegExp(`(${escapeRegex(queryEscaped)})`, 'gi')
    return escaped.replace(regex, '<mark class="search-highlight">$1</mark>')
  }

  function escapeHtml(str: string): string {
    return str
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
  }

  function escapeRegex(str: string): string {
    return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  }

  function statusLabel(status: DiffFile['status']): string {
    if (status === 'added') return 'Added'
    if (status === 'modified') return 'Modified'
    if (status === 'deleted') return 'Deleted'
    return 'Renamed'
  }

  function statusClass(status: DiffFile['status']): string {
    if (status === 'added') return 'badge-added'
    if (status === 'deleted') return 'badge-deleted'
    return 'badge-modified'
  }

  function toggleCollapse(path: string): void {
    if (collapsedFiles.has(path)) {
      collapsedFiles.delete(path)
    } else {
      collapsedFiles.add(path)
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return

    if (e.key === 'j' || e.key === 'J') {
      e.preventDefault()
      if (files.length > 0) {
        focusedFileIndex = Math.min(focusedFileIndex + 1, files.length - 1)
        scrollToFocusedFile()
      }
    } else if (e.key === 'k' || e.key === 'K') {
      e.preventDefault()
      if (files.length > 0) {
        focusedFileIndex = Math.max(focusedFileIndex - 1, 0)
        scrollToFocusedFile()
      }
    }
  }

  function scrollToFocusedFile(): void {
    if (focusedFileIndex >= 0 && focusedFileIndex < files.length) {
      const file = files[focusedFileIndex]
      const el = document.getElementById(`diff-file-${CSS.escape(file.path)}`)
      if (el && bodyEl) {
        el.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
      }
    }
  }

  function openComment(filePath: string, line: number): void {
    const key = `${filePath}:${line}`
    if (commentKey === key) {
      closeComment()
      return
    }
    commentKey = key
    commentFilePath = filePath
    commentLineNum = line
    commentText = ''
  }

  function closeComment(): void {
    commentKey = null
    commentText = ''
  }

  function gatherContext(filePath: string, lineNum: number): string {
    const file = files.find((f) => f.path === filePath)
    if (!file) return ''

    for (const hunk of file.hunks) {
      const changeIndex = hunk.changes.findIndex((c) => getLineNum(c) === lineNum)
      if (changeIndex === -1) continue

      const start = Math.max(0, changeIndex - 3)
      const end = Math.min(hunk.changes.length - 1, changeIndex + 3)
      const lines: string[] = []

      for (let i = start; i <= end; i++) {
        const c = hunk.changes[i]
        const ln = getLineNum(c)
        const prefix = i === changeIndex ? '>' : ' '
        const marker = c.type === 'add' ? '+' : c.type === 'delete' ? '-' : ' '
        lines.push(`${prefix} ${String(ln).padStart(4)} | ${marker}${c.content}`)
      }

      return lines.join('\n')
    }
    return ''
  }

  function sendComment(): void {
    if (!commentText.trim()) return
    const sessions = getAiSessions(worktreePath)
    if (sessions.length === 0) return

    const context = gatherContext(commentFilePath, commentLineNum)
    const contextBlock = context ? `\nContext:\n${context}\n` : ''

    const message = [
      `In file ${commentFilePath}, around line ${commentLineNum}:`,
      contextBlock,
      `Comment: ${commentText.trim()}`,
    ].join('\n')

    window.api.writePty(sessions[0].sessionId, message + '\n')
    focusSessionByPtyId(sessions[0].sessionId)
    window.dispatchEvent(
      new CustomEvent('canopy:focus-terminal', {
        detail: { sessionId: sessions[0].sessionId },
      }),
    )
    closeComment()
  }

  function getLineNum(change: DiffChange): number {
    return change.newLine ?? change.oldLine ?? 0
  }

  function statsBarWidth(file: DiffFile): number {
    const total = file.additions + file.deletions
    return Math.min(total, 60)
  }

  function statsBarAddWidth(file: DiffFile): number {
    const total = file.additions + file.deletions
    if (total === 0) return 0
    return Math.round((file.additions / total) * statsBarWidth(file))
  }

  function statsBarDelWidth(file: DiffFile): number {
    return statsBarWidth(file) - statsBarAddWidth(file)
  }

  function buildUnifiedDiff(file: DiffFile): string {
    const lines: string[] = []
    lines.push(`--- a/${file.oldPath ?? file.path}`)
    lines.push(`+++ b/${file.path}`)
    for (const hunk of file.hunks) {
      lines.push(hunk.header)
      for (const change of hunk.changes) {
        const prefix = change.type === 'add' ? '+' : change.type === 'delete' ? '-' : ' '
        lines.push(`${prefix}${change.content}`)
      }
    }
    return lines.join('\n')
  }

  async function copyDiff(file: DiffFile): Promise<void> {
    const text = buildUnifiedDiff(file)
    await navigator.clipboard.writeText(text)
  }

  function toggleSearch(): void {
    showSearch = !showSearch
    if (!showSearch) {
      searchQuery = ''
    }
  }

  let hasAgent = $derived(getAiSessions(worktreePath).length > 0)
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="diff-pane"
  class:hidden={!active}
  bind:this={paneEl}
  tabindex="-1"
  onkeydown={handleKeydown}
>
  <div class="diff-toolbar" class:toolbar-pulse={justRefreshed}>
    <span class="toolbar-title">Diff</span>
    <span class="toolbar-summary">
      {files.length} file{files.length !== 1 ? 's' : ''}
      <span class="stat-add">+{totalAdditions}</span>
      <span class="stat-del">&minus;{totalDeletions}</span>
    </span>
    {#if showSearch}
      <div class="search-bar">
        <input
          class="search-input"
          type="text"
          placeholder="Search in diff..."
          bind:value={searchQuery}
          onkeydown={(e) => {
            if (e.key === 'Escape') toggleSearch()
          }}
        />
        {#if searchQuery}
          <span class="match-count">{matchCount} match{matchCount !== 1 ? 'es' : ''}</span>
        {/if}
      </div>
    {/if}
    <button class="toolbar-btn" onclick={toggleSearch} title="Search" aria-label="Search in diff">
      &#x1F50D;
    </button>
    <button
      class="toolbar-btn"
      onclick={refresh}
      title="Refresh"
      aria-label="Refresh diff"
      disabled={loading}
    >
      &#x21BB;
    </button>
  </div>

  <div class="diff-body" bind:this={bodyEl}>
    {#if loading && files.length === 0}
      <div class="empty-state">Loading...</div>
    {:else if files.length === 0}
      <div class="empty-state">No uncommitted changes</div>
    {:else}
      {#each files as file, fileIndex (file.path)}
        <div
          class="file-section"
          class:file-focused={focusedFileIndex === fileIndex}
          id="diff-file-{file.path}"
          data-filepath={file.path}
          onpointerenter={() => (hoveredFilePath = file.path)}
          onpointerleave={() => (hoveredFilePath = null)}
        >
          <div class="file-header" onclick={() => toggleCollapse(file.path)}>
            <span class="chevron" class:chevron-open={!collapsedFiles.has(file.path)}>&#x25B8;</span
            >
            <span class="file-status {statusClass(file.status)}">{statusLabel(file.status)}</span>
            <span class="file-path">{file.path}</span>
            <span class="stats-bar">
              {#if file.additions > 0}
                <span class="stats-bar-add" style="width: {statsBarAddWidth(file)}px"></span>
              {/if}
              {#if file.deletions > 0}
                <span class="stats-bar-del" style="width: {statsBarDelWidth(file)}px"></span>
              {/if}
            </span>
            <span class="file-stats">
              <span class="stat-add">+{file.additions}</span>
              <span class="stat-del">&minus;{file.deletions}</span>
            </span>
            {#if hoveredFilePath === file.path}
              <button
                class="copy-diff-btn"
                title="Copy diff"
                aria-label="Copy diff to clipboard"
                onclick={(e) => {
                  e.stopPropagation()
                  copyDiff(file)
                }}
              >
                &#x1F4CB;
              </button>
            {/if}
          </div>

          {#if !collapsedFiles.has(file.path)}
            {#if file.hunks.length === 0}
              <div class="no-hunks">Binary file or empty diff</div>
            {:else}
              {#each file.hunks as hunk (hunk.header)}
                <div class="hunk">
                  <div class="hunk-header">
                    <span class="gutter old-gutter"></span>
                    <span class="gutter new-gutter"></span>
                    <span class="hunk-text">{hunk.header}</span>
                  </div>
                  {#each hunk.changes as change, i (i)}
                    <div
                      class="diff-line {change.type}"
                      class:search-match={lineMatchesSearch(change.content)}
                    >
                      {#if hasAgent}
                        <button
                          class="comment-trigger"
                          title="Add review comment"
                          aria-label="Add review comment"
                          onclick={() => openComment(file.path, getLineNum(change))}>+</button
                        >
                      {/if}
                      <span class="gutter old-gutter"
                        >{change.type !== 'add' && change.oldLine != null
                          ? change.oldLine
                          : ''}</span
                      >
                      <span class="gutter new-gutter"
                        >{change.type !== 'delete' && change.newLine != null
                          ? change.newLine
                          : ''}</span
                      >
                      <span class="line-prefix"
                        >{change.type === 'add' ? '+' : change.type === 'delete' ? '-' : ' '}</span
                      >
                      {#if searchQuery && lineMatchesSearch(change.content)}
                        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                        <span class="line-content">{@html highlightMatch(change.content)}</span>
                      {:else}
                        <span class="line-content">{change.content}</span>
                      {/if}
                    </div>
                    {#if commentKey === `${file.path}:${getLineNum(change)}`}
                      <div class="comment-form">
                        <div class="comment-card">
                          <div class="comment-card-header">
                            <svg
                              class="comment-icon"
                              width="14"
                              height="14"
                              viewBox="0 0 16 16"
                              fill="currentColor"
                            >
                              <path
                                d="M1 2.75C1 1.784 1.784 1 2.75 1h10.5c.966 0 1.75.784 1.75 1.75v7.5A1.75 1.75 0 0 1 13.25 12H9.06l-2.573 2.573A1.458 1.458 0 0 1 4 13.543V12H2.75A1.75 1.75 0 0 1 1 10.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h4.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z"
                              />
                            </svg>
                            <span class="comment-label">Review comment</span>
                            <span class="comment-file-ref">{file.path}:{getLineNum(change)}</span>
                          </div>
                          <textarea
                            class="comment-input"
                            placeholder="Describe what should be changed..."
                            bind:value={commentText}
                            onkeydown={(e) => {
                              if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) sendComment()
                              if (e.key === 'Escape') closeComment()
                            }}
                          ></textarea>
                          <div class="comment-footer">
                            <span class="comment-hint">
                              {navigator.userAgent.includes('Mac') ? '\u2318' : 'Ctrl'}+Enter to
                              send
                            </span>
                            <div class="comment-actions">
                              <button class="comment-cancel" onclick={closeComment}>Cancel</button>
                              <button
                                class="comment-send"
                                onclick={sendComment}
                                disabled={!commentText.trim()}
                              >
                                <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                                  <path
                                    d="M.989 8 .064 2.68a1.342 1.342 0 0 1 1.85-1.462l13.402 5.744a1.13 1.13 0 0 1 0 2.076L1.913 14.782a1.343 1.343 0 0 1-1.85-1.463Z"
                                  />
                                </svg>
                                Send to Agent
                              </button>
                            </div>
                          </div>
                        </div>
                      </div>
                    {/if}
                  {/each}
                </div>
              {/each}
            {/if}
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .diff-pane {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--c-bg);
    outline: none;
  }

  .diff-pane.hidden {
    display: none;
  }

  .diff-toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 16px;
    background: var(--c-bg-elevated);
    border-bottom: 1px solid var(--c-border-subtle);
    flex-shrink: 0;
    transition: box-shadow 0.3s ease;
  }

  .toolbar-pulse {
    animation: pulse-glow 1s ease-out;
  }

  @keyframes pulse-glow {
    0% {
      box-shadow: inset 0 0 12px color-mix(in srgb, var(--c-accent) 40%, transparent);
    }
    100% {
      box-shadow: none;
    }
  }

  .toolbar-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--c-text);
  }

  .toolbar-summary {
    font-size: 12px;
    color: var(--c-text-muted);
    flex: 1;
  }

  .stat-add {
    color: var(--diff-add-fg);
  }

  .stat-del {
    color: var(--diff-delete-fg);
  }

  .toolbar-btn {
    background: none;
    border: none;
    color: var(--c-text-muted);
    cursor: pointer;
    font-size: 14px;
    padding: 2px 6px;
    border-radius: 4px;
  }

  .toolbar-btn:hover {
    color: var(--c-text);
    background: var(--c-active);
  }

  .toolbar-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .search-bar {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .search-input {
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    font-size: 12px;
    padding: 3px 8px;
    width: 180px;
    outline: none;
    font-family: var(--font-mono, monospace);
  }

  .search-input:focus {
    border-color: var(--c-accent);
  }

  .match-count {
    font-size: 11px;
    color: var(--c-text-muted);
    white-space: nowrap;
  }

  .diff-body {
    flex: 1;
    overflow: auto;
    min-height: 0;
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    line-height: 1.5;
  }

  .file-section {
    border-bottom: 1px solid var(--c-border-subtle);
    border-left: 2px solid transparent;
  }

  .file-section.file-focused {
    border-left: 2px solid var(--c-accent);
  }

  .file-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    background: var(--c-bg-elevated);
    border-bottom: 1px solid var(--c-border-subtle);
    position: sticky;
    top: 0;
    z-index: 1;
    cursor: pointer;
    user-select: none;
  }

  .file-header:hover {
    background: var(--c-active);
  }

  .chevron {
    font-size: 10px;
    color: var(--c-text-muted);
    transition: transform 0.15s ease;
    flex-shrink: 0;
    display: inline-block;
  }

  .chevron.chevron-open {
    transform: rotate(90deg);
  }

  .file-status {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.3px;
    flex-shrink: 0;
  }

  .badge-added {
    background: color-mix(in srgb, var(--c-success) 20%, transparent);
    color: var(--diff-add-fg);
  }

  .badge-modified {
    background: color-mix(in srgb, var(--c-accent) 20%, transparent);
    color: var(--c-accent);
  }

  .badge-deleted {
    background: var(--diff-delete-bg);
    color: var(--diff-delete-fg);
  }

  .file-path {
    font-size: 12px;
    color: var(--c-text);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono, monospace);
  }

  .stats-bar {
    display: flex;
    align-items: center;
    height: 8px;
    border-radius: 2px;
    overflow: hidden;
    flex-shrink: 0;
  }

  .stats-bar-add {
    height: 100%;
    background: var(--diff-add-fg);
    display: block;
  }

  .stats-bar-del {
    height: 100%;
    background: var(--diff-delete-fg);
    display: block;
  }

  .file-stats {
    display: flex;
    gap: 6px;
    font-size: 11px;
    flex-shrink: 0;
  }

  .copy-diff-btn {
    background: none;
    border: 1px solid var(--c-border);
    color: var(--c-text-muted);
    cursor: pointer;
    font-size: 12px;
    padding: 2px 6px;
    border-radius: 4px;
    flex-shrink: 0;
  }

  .copy-diff-btn:hover {
    color: var(--c-text);
    background: var(--c-active);
  }

  .no-hunks {
    padding: 12px 16px;
    color: var(--c-text-muted);
    font-style: italic;
  }

  .hunk-header {
    display: flex;
    background: var(--diff-hunk-header-bg);
    color: var(--c-text-muted);
    padding: 2px 0;
    font-size: 11px;
  }

  .hunk-text {
    padding: 0 8px;
    flex: 1;
  }

  .diff-line {
    display: flex;
    -webkit-user-select: text;
    user-select: text;
    position: relative;
  }

  .diff-line.search-match {
    outline: 1px solid var(--c-warning);
    outline-offset: -1px;
  }

  :global(.search-highlight) {
    background: rgba(var(--c-warning), 0.3);
    background: color-mix(in srgb, var(--c-warning) 30%, transparent);
    border-radius: 2px;
  }

  .comment-trigger {
    position: absolute;
    left: 1px;
    top: 50%;
    transform: translateY(-50%);
    width: 22px;
    height: 22px;
    border-radius: 6px;
    border: none;
    background: var(--c-accent);
    color: var(--c-bg);
    font-size: 15px;
    font-weight: 700;
    line-height: 1;
    cursor: pointer;
    opacity: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: opacity 0.1s ease;
  }

  .diff-line:hover .comment-trigger {
    opacity: 1;
  }

  .comment-trigger:hover {
    filter: brightness(1.1);
  }

  .comment-trigger:active {
    filter: brightness(0.85);
  }

  .comment-form {
    padding: 10px 16px 10px 24px;
    animation: comment-slide-in 0.25s cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  @keyframes comment-slide-in {
    0% {
      opacity: 0;
      transform: translateY(-4px);
    }
    100% {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .comment-card {
    background: var(--c-bg-elevated);
    border: 1px solid var(--c-border);
    border-radius: 10px;
    overflow: hidden;
    box-shadow:
      0 2px 8px rgba(0, 0, 0, 0.15),
      0 1px 2px rgba(0, 0, 0, 0.1);
  }

  .comment-card-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    background: var(--c-border-subtle);
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .comment-icon {
    color: var(--c-accent);
    flex-shrink: 0;
  }

  .comment-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--c-text-secondary);
    letter-spacing: 0.2px;
  }

  .comment-file-ref {
    font-size: 10px;
    color: var(--c-text-faint);
    margin-left: auto;
    font-family: var(--font-mono, monospace);
  }

  .comment-input {
    width: 100%;
    min-height: 72px;
    padding: 10px 12px;
    border: none;
    background: transparent;
    color: var(--c-text);
    font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', system-ui, sans-serif;
    font-size: 13px;
    line-height: 1.5;
    resize: vertical;
    outline: none;
  }

  .comment-input::placeholder {
    color: var(--c-text-faint);
  }

  .comment-footer {
    display: flex;
    align-items: center;
    padding: 6px 12px 8px;
    border-top: 1px solid var(--c-border-subtle);
    background: var(--c-border-subtle);
  }

  .comment-hint {
    font-size: 10px;
    color: var(--c-text-faint);
    flex: 1;
  }

  .comment-actions {
    display: flex;
    gap: 6px;
  }

  .comment-cancel {
    padding: 5px 12px;
    border: none;
    background: var(--c-active);
    color: var(--c-text-muted);
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    font-family: inherit;
    transition:
      background 0.15s,
      color 0.15s;
  }

  .comment-cancel:hover {
    color: var(--c-text);
    background: var(--c-active);
  }

  .comment-send {
    padding: 5px 14px;
    border: none;
    background: var(--c-accent);
    color: var(--c-bg);
    border-radius: 6px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    font-family: inherit;
    display: flex;
    align-items: center;
    gap: 5px;
    transition:
      filter 0.15s,
      transform 0.1s;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  }

  .comment-send:hover {
    filter: brightness(1.15);
  }

  .comment-send:active {
    transform: scale(0.97);
  }

  .comment-send:disabled {
    opacity: 0.35;
    cursor: default;
    transform: none;
    filter: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .comment-trigger {
      transition: none;
    }

    .comment-form {
      animation: none;
    }

    .comment-send {
      transition: none;
    }

    .toolbar-pulse {
      animation: none;
    }

    .chevron {
      transition: none;
    }
  }

  .diff-line.add {
    background: var(--diff-add-bg);
  }

  .diff-line.delete {
    background: var(--diff-delete-bg);
  }

  .gutter {
    display: inline-block;
    width: 4ch;
    text-align: right;
    padding-right: 4px;
    color: var(--c-text-faint);
    flex-shrink: 0;
    -webkit-user-select: none;
    user-select: none;
  }

  .line-prefix {
    width: 1ch;
    flex-shrink: 0;
    -webkit-user-select: none;
    user-select: none;
    color: var(--c-text-faint);
  }

  .diff-line.add .line-prefix {
    color: var(--diff-add-fg);
  }

  .diff-line.delete .line-prefix {
    color: var(--diff-delete-fg);
  }

  .line-content {
    flex: 1;
    padding: 0 8px 0 4px;
    white-space: pre;
    min-width: 0;
    color: var(--c-text);
  }

  .diff-line.add .line-content {
    color: var(--diff-add-fg);
  }

  .diff-line.delete .line-content {
    color: var(--diff-delete-fg);
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    font-size: 13px;
    color: var(--c-text-muted);
    font-family: inherit;
  }
</style>
