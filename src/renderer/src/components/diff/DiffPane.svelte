<script lang="ts">
  import { onMount } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { Search, RotateCw, ChevronRight, Copy } from 'lucide-svelte'
  import { getAiSessions, focusSessionByPtyId } from '../../lib/stores/tabs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import type { DiffChange, DiffFile } from '../../lib/types/diff'

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
  let pulseTimer: ReturnType<typeof setTimeout> | null = null

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
    if (pulseTimer != null) clearTimeout(pulseTimer)
    justRefreshed = true
    pulseTimer = setTimeout(() => {
      justRefreshed = false
      pulseTimer = null
    }, 1000)
  }

  // Re-fetch when worktreePath changes (branch/worktree switch)
  $effect(() => {
    void worktreePath
    refresh()
  })

  // Flag to suppress observer during programmatic scrolls
  let suppressObserver = false
  let suppressTimer: ReturnType<typeof setTimeout> | null = null

  // Scroll to file when diffScrollTarget changes
  $effect(() => {
    const target = workspaceState.diffScrollTarget
    if (!target) return
    const filePath = target.path
    // Delay to ensure tab switch + DOM layout completes
    const timer = setTimeout(() => {
      const el = document.getElementById(`diff-file-${filePath}`)
      if (el && bodyEl) {
        suppressObserver = true
        workspaceState.diffScrollTarget = null
        workspaceState.diffVisibleFile = filePath
        const top = el.offsetTop - bodyEl.offsetTop
        bodyEl.scrollTo({ top, left: 0, behavior: 'smooth' })
        // Fallback in case scrollTo is a no-op (already at position)
        if (suppressTimer != null) clearTimeout(suppressTimer)
        suppressTimer = setTimeout(() => {
          suppressObserver = false
        }, 300)
      }
    }, 150)
    return () => clearTimeout(timer)
  })

  onMount(() => {
    const unsubGit = window.api.onGitChanged(() => {
      refresh().then(() => triggerPulse())
    })

    // React to actual filesystem changes (file create/modify/delete) so the
    // diff stays in sync without waiting for git metadata events. Debounced
    // 200ms to coalesce bursts (e.g. an agent dropping multiple files).
    let fileRefreshTimer: ReturnType<typeof setTimeout> | null = null
    const unsubFileWatcher = window.api.onFilesChanged((payload) => {
      if (payload.repoRoot !== worktreePath) return
      if (fileRefreshTimer) clearTimeout(fileRefreshTimer)
      fileRefreshTimer = setTimeout(() => {
        fileRefreshTimer = null
        refresh().then(() => triggerPulse())
      }, 200)
    })

    // Re-enable observer when scroll settles (user or programmatic)
    const onScrollEnd = (): void => {
      suppressObserver = false
      if (suppressTimer != null) {
        clearTimeout(suppressTimer)
        suppressTimer = null
      }
    }
    bodyEl?.addEventListener('scrollend', onScrollEnd)

    // Track which file is currently visible via IntersectionObserver
    let observer: IntersectionObserver | null = null

    function setupObserver(): void {
      observer?.disconnect()
      if (!bodyEl) return
      observer = new IntersectionObserver(
        (entries) => {
          if (suppressObserver) return
          for (const entry of entries) {
            if (entry.isIntersecting) {
              // IntersectionObserverEntry.target is always Element, safe to cast
              const path = (entry.target as HTMLElement).dataset.filepath
              if (path) {
                workspaceState.diffVisibleFile = path
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
        const timerId = setTimeout(setupObserver, 100)
        return () => clearTimeout(timerId)
      })
      return () => observer?.disconnect()
    })

    return () => {
      unsubGit()
      unsubFileWatcher()
      unsubFiles()
      observer?.disconnect()
      bodyEl?.removeEventListener('scrollend', onScrollEnd)
      if (fileRefreshTimer != null) clearTimeout(fileRefreshTimer)
      if (pulseTimer != null) clearTimeout(pulseTimer)
      if (suppressTimer != null) clearTimeout(suppressTimer)
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
        suppressObserver = true
        workspaceState.diffVisibleFile = file.path
        const top = el.offsetTop - bodyEl.offsetTop
        bodyEl.scrollTo({ top, left: 0, behavior: 'smooth' })
        // Fallback in case scrollTo is a no-op (already at position)
        if (suppressTimer != null) clearTimeout(suppressTimer)
        suppressTimer = setTimeout(() => {
          suppressObserver = false
        }, 300)
      }
    }
  }

  function openComment(filePath: string, changeId: string, line: number): void {
    const key = `${filePath}:${changeId}`
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

  function sanitizePtyInput(text: string): string {
    // Strip control characters (0x00-0x1F except \n \t) and ANSI escape sequences
    let result = ''
    for (let i = 0; i < text.length; i++) {
      const code = text.charCodeAt(i)
      if (code === 0x1b) {
        // Skip ANSI escape sequence: ESC[ ... letter
        if (text[i + 1] === '[') {
          let j = i + 2
          while (j < text.length && !/[a-zA-Z]/.test(text[j])) j++
          i = j
          continue
        }
      }
      // Keep \n (0x0A) and \t (0x09), strip other control chars
      if (code < 0x20 && code !== 0x0a && code !== 0x09) continue
      if (code === 0x7f) continue
      result += text[i]
    }
    return result
  }

  function sendComment(): void {
    if (!commentText.trim()) return
    const sessions = getAiSessions(worktreePath)
    if (sessions.length === 0) return

    const safeComment = sanitizePtyInput(commentText.trim())
    const context = gatherContext(commentFilePath, commentLineNum)
    const contextLines = context ? context.split('\n') : []

    const message = [
      '---',
      `[Code Review] ${commentFilePath}:${commentLineNum}`,
      '',
      ...contextLines,
      '',
      `Comment: ${safeComment}`,
      '---',
    ].join('\n')

    window.api.writePty(sessions[0].sessionId, sanitizePtyInput(message) + '\n')
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
      <Search size={14} />
    </button>
    <button
      class="toolbar-btn"
      onclick={refresh}
      title="Refresh"
      aria-label="Refresh diff"
      disabled={loading}
    >
      <RotateCw size={14} />
    </button>
  </div>

  <div class="diff-body" bind:this={bodyEl}>
    {#if loading && files.length === 0}
      <div class="empty-state">Loading...</div>
    {:else if files.length === 0}
      <div class="empty-state">No uncommitted changes</div>
    {:else}
      <div class="diff-scroll-content">
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
              <span class="chevron" class:chevron-open={!collapsedFiles.has(file.path)}>
                <ChevronRight size={12} />
              </span>
              <span class="file-status {statusClass(file.status)}">{statusLabel(file.status)}</span>
              <span class="file-path" title={file.path}>{file.path}</span>
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
                  <Copy size={12} />
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
                    {#each hunk.changes as change, i (`${i}-${change.type}`)}
                      <div
                        class="diff-line {change.type}"
                        class:search-match={lineMatchesSearch(change.content)}
                      >
                        {#if hasAgent}
                          <button
                            class="comment-trigger"
                            title="Add review comment"
                            aria-label="Add review comment"
                            onclick={() =>
                              openComment(file.path, `${hunk.header}:${i}`, getLineNum(change))}
                            >+</button
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
                          >{change.type === 'add'
                            ? '+'
                            : change.type === 'delete'
                              ? '-'
                              : ' '}</span
                        >
                        {#if searchQuery && lineMatchesSearch(change.content)}
                          <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                          <span class="line-content">{@html highlightMatch(change.content)}</span>
                        {:else}
                          <span class="line-content">{change.content}</span>
                        {/if}
                      </div>
                      {#if commentKey === `${file.path}:${hunk.header}:${i}`}
                        <div class="comment-form">
                          <textarea
                            class="comment-input"
                            placeholder="Comment for agent — {file.path}:{getLineNum(change)}"
                            bind:value={commentText}
                            onkeydown={(e) => {
                              if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) sendComment()
                              if (e.key === 'Escape') closeComment()
                            }}
                          ></textarea>
                          <div class="comment-footer">
                            <span class="comment-hint">
                              {navigator.userAgent.includes('Mac') ? '⌘' : 'Ctrl'}+Enter to send
                            </span>
                            <div class="comment-actions">
                              <button class="comment-cancel" onclick={closeComment}>Cancel</button>
                              <button
                                class="comment-send"
                                onclick={sendComment}
                                disabled={!commentText.trim()}
                              >
                                Send
                              </button>
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
      </div>
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
    padding: 2px 6px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
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
    container-type: inline-size;
  }

  .diff-scroll-content {
    width: fit-content;
    min-width: 100%;
  }

  .file-section {
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .file-section.file-focused {
    background: color-mix(in srgb, var(--c-accent) 5%, transparent);
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
    background: color-mix(in srgb, var(--c-text) 8%, var(--c-bg-elevated));
  }

  .chevron {
    color: var(--c-text-muted);
    transition: transform 0.15s ease;
    flex-shrink: 0;
    display: flex;
    align-items: center;
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
    padding: 2px 6px;
    border-radius: 4px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
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
    position: sticky;
    left: 0;
    width: calc(100cqi - 16px);
    margin: 4px 8px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-elevated);
    overflow: hidden;
    animation: comment-slide-in 0.15s ease both;
    box-sizing: border-box;
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

  .comment-input {
    width: 100%;
    min-height: 64px;
    padding: 10px 16px;
    border: none;
    background: var(--c-bg-elevated);
    color: var(--c-text);
    font-family: inherit;
    font-size: 12px;
    line-height: 1.5;
    resize: none;
    outline: none;
    box-sizing: border-box;
  }

  .comment-input::placeholder {
    color: var(--c-text-muted);
  }

  .comment-footer {
    display: flex;
    align-items: center;
    padding: 6px 16px;
    background: var(--c-bg-elevated);
    border-top: 1px solid var(--c-border-subtle);
  }

  .comment-hint {
    font-size: 11px;
    color: var(--c-text-secondary);
    flex: 1;
  }

  .comment-actions {
    display: flex;
    gap: 4px;
  }

  .comment-cancel {
    padding: 3px 10px;
    border: 1px solid var(--c-border-subtle);
    background: none;
    color: var(--c-text-muted);
    border-radius: 4px;
    font-size: 11px;
    cursor: pointer;
    font-family: inherit;
  }

  .comment-cancel:hover {
    color: var(--c-text);
    background: var(--c-hover);
  }

  .comment-send {
    padding: 3px 10px;
    border: none;
    background: var(--c-accent);
    color: var(--c-bg);
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    font-family: inherit;
  }

  .comment-send:hover {
    filter: brightness(1.1);
  }

  .comment-send:disabled {
    opacity: 0.35;
    cursor: default;
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
