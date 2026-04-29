<script lang="ts">
  import { onMount } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { match } from 'ts-pattern'
  import { Search, RotateCw, ChevronRight, Copy } from 'lucide-svelte'
  import { getAiSessions, focusSessionByPtyId } from '../../lib/stores/tabs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { wrapAsBracketedPaste } from '../../lib/pty/paste'
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
    return match(status)
      .with('added', () => 'Added')
      .with('modified', () => 'Modified')
      .with('deleted', () => 'Deleted')
      .with('renamed', () => 'Renamed')
      .exhaustive()
  }

  function statusClass(status: DiffFile['status']): string {
    return match(status)
      .with('added', () => 'bg-success-bg text-diff-add-fg')
      .with('deleted', () => 'bg-diff-delete-bg text-diff-delete-fg')
      .with('modified', 'renamed', () => 'bg-accent-bg text-accent')
      .exhaustive()
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

  function sendComment(): void {
    if (!commentText.trim()) return
    const sessions = getAiSessions(worktreePath)
    if (sessions.length === 0) return

    const context = gatherContext(commentFilePath, commentLineNum)
    const contextLines = context ? context.split('\n') : []

    const message = [
      '---',
      `[Code Review] ${commentFilePath}:${commentLineNum}`,
      '',
      ...contextLines,
      '',
      `Comment: ${commentText.trim()}`,
      '---',
    ].join('\n')

    window.api.writePty(sessions[0].sessionId, wrapAsBracketedPaste(message) + '\r')
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
  class="flex flex-col w-full h-full overflow-hidden bg-bg outline-none"
  class:hidden={!active}
  bind:this={paneEl}
  tabindex="-1"
  onkeydown={handleKeydown}
>
  <div
    class="flex items-center gap-2.5 px-4 py-2 bg-bg-elevated border-b border-border-subtle flex-shrink-0 transition-shadow duration-slow motion-reduce:transition-none"
    class:animate-pulse-glow={justRefreshed}
    class:motion-reduce:animate-none={justRefreshed}
  >
    <span class="text-md font-semibold text-text">Diff</span>
    <span class="text-sm text-text-muted flex-1">
      {files.length} file{files.length !== 1 ? 's' : ''}
      <span class="text-diff-add-fg">+{totalAdditions}</span>
      <span class="text-diff-delete-fg">&minus;{totalDeletions}</span>
    </span>
    {#if showSearch}
      <div class="flex items-center gap-1.5">
        <input
          class="bg-bg border border-border rounded-md text-text text-sm px-2 py-0.5 w-45 outline-none font-mono focus:border-accent"
          type="text"
          placeholder="Search in diff..."
          bind:value={searchQuery}
          onkeydown={(e) => {
            if (e.key === 'Escape') toggleSearch()
          }}
        />
        {#if searchQuery}
          <span class="text-xs text-text-muted whitespace-nowrap"
            >{matchCount} match{matchCount !== 1 ? 'es' : ''}</span
          >
        {/if}
      </div>
    {/if}
    <button
      class="bg-transparent border-0 text-text-muted cursor-pointer px-1.5 py-0.5 rounded-md flex items-center justify-center hover:text-text hover:bg-active"
      onclick={toggleSearch}
      title="Search"
      aria-label="Search in diff"
    >
      <Search size={14} />
    </button>
    <button
      class="bg-transparent border-0 text-text-muted cursor-pointer px-1.5 py-0.5 rounded-md flex items-center justify-center enabled:hover:text-text enabled:hover:bg-active disabled:opacity-40 disabled:cursor-default"
      onclick={refresh}
      title="Refresh"
      aria-label="Refresh diff"
      disabled={loading}
    >
      <RotateCw size={14} />
    </button>
  </div>

  <div
    class="flex-1 overflow-auto min-h-0 font-mono text-sm leading-normal container-inline"
    bind:this={bodyEl}
  >
    {#if loading && files.length === 0}
      <div class="flex items-center justify-center h-full text-md text-text-muted">Loading...</div>
    {:else if files.length === 0}
      <div class="flex items-center justify-center h-full text-md text-text-muted">
        No uncommitted changes
      </div>
    {:else}
      <div class="w-fit min-w-full">
        {#each files as file, fileIndex (file.path)}
          <div
            class="border-b border-border-subtle"
            class:bg-file-focused={focusedFileIndex === fileIndex}
            id="diff-file-{file.path}"
            data-filepath={file.path}
            onpointerenter={() => (hoveredFilePath = file.path)}
            onpointerleave={() => (hoveredFilePath = null)}
          >
            <div
              class="file-header flex items-center gap-2 h-9 px-4 bg-bg-elevated border-b border-border-subtle sticky top-0 z-10 cursor-pointer select-none hover:bg-file-header-hover"
              role="button"
              tabindex="0"
              aria-expanded={!collapsedFiles.has(file.path)}
              onclick={() => toggleCollapse(file.path)}
              onkeydown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault()
                  toggleCollapse(file.path)
                }
              }}
            >
              <span
                class="text-text-muted transition-transform duration-base ease-std flex-shrink-0 flex items-center motion-reduce:transition-none"
                class:rotate-90={!collapsedFiles.has(file.path)}
              >
                <ChevronRight size={12} />
              </span>
              <span
                class="text-2xs font-semibold py-px px-1.5 rounded-sm uppercase tracking-caps-tight flex-shrink-0 {statusClass(
                  file.status,
                )}">{statusLabel(file.status)}</span
              >
              <span
                class="text-sm text-text flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono"
                title={file.path}>{file.path}</span
              >
              <span class="flex items-center h-2 rounded-xs overflow-hidden flex-shrink-0">
                {#if file.additions > 0}
                  <span
                    class="block h-full bg-diff-add-fg"
                    style="width: {statsBarAddWidth(file)}px"
                  ></span>
                {/if}
                {#if file.deletions > 0}
                  <span
                    class="block h-full bg-diff-delete-fg"
                    style="width: {statsBarDelWidth(file)}px"
                  ></span>
                {/if}
              </span>
              <span class="flex gap-1.5 text-xs flex-shrink-0">
                <span class="text-diff-add-fg">+{file.additions}</span>
                <span class="text-diff-delete-fg">&minus;{file.deletions}</span>
              </span>
              {#if hoveredFilePath === file.path}
                <button
                  class="bg-transparent border border-border text-text-muted cursor-pointer px-1.5 py-0.5 rounded-md flex-shrink-0 flex items-center justify-center hover:text-text hover:bg-active"
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
                <div class="px-4 py-3 text-text-muted italic">Binary file or empty diff</div>
              {:else}
                {#each file.hunks as hunk (hunk.header)}
                  <div>
                    <div class="flex bg-diff-hunk-header-bg text-text-muted py-0.5 text-xs">
                      <span
                        class="inline-block w-4ch text-right pr-1 text-text-faint flex-shrink-0 select-none"
                      ></span>
                      <span
                        class="inline-block w-4ch text-right pr-1 text-text-faint flex-shrink-0 select-none"
                      ></span>
                      <span class="px-2 flex-1">{hunk.header}</span>
                    </div>
                    {#each hunk.changes as change, i (`${i}-${change.type}`)}
                      <div
                        class="diff-line {change.type} flex select-text relative"
                        class:bg-diff-add-bg={change.type === 'add'}
                        class:bg-diff-delete-bg={change.type === 'delete'}
                        class:outline-1={lineMatchesSearch(change.content)}
                        class:outline-warning={lineMatchesSearch(change.content)}
                        class:-outline-offset-1={lineMatchesSearch(change.content)}
                      >
                        {#if hasAgent}
                          <button
                            class="comment-trigger absolute left-px top-1/2 -translate-y-1/2 w-5.5 h-5.5 rounded-lg border-0 bg-accent text-bg text-base font-bold leading-none cursor-pointer opacity-0 z-10 flex items-center justify-center transition-opacity duration-fast motion-reduce:transition-none hover:brightness-110 active:brightness-85"
                            title="Add review comment"
                            aria-label="Add review comment"
                            onclick={() =>
                              openComment(file.path, `${hunk.header}:${i}`, getLineNum(change))}
                            >+</button
                          >
                        {/if}
                        <span
                          class="inline-block w-4ch text-right pr-1 text-text-faint flex-shrink-0 select-none"
                          >{change.type !== 'add' && change.oldLine != null
                            ? change.oldLine
                            : ''}</span
                        >
                        <span
                          class="inline-block w-4ch text-right pr-1 text-text-faint flex-shrink-0 select-none"
                          >{change.type !== 'delete' && change.newLine != null
                            ? change.newLine
                            : ''}</span
                        >
                        <span class="line-prefix w-1ch flex-shrink-0 select-none text-text-faint"
                          >{change.type === 'add'
                            ? '+'
                            : change.type === 'delete'
                              ? '-'
                              : ' '}</span
                        >
                        {#if searchQuery && lineMatchesSearch(change.content)}
                          <span
                            class="line-content flex-1 px-2 pl-1 whitespace-pre min-w-0 text-text"
                          >
                            <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                            {@html highlightMatch(change.content)}
                          </span>
                        {:else}
                          <span
                            class="line-content flex-1 px-2 pl-1 whitespace-pre min-w-0 text-text"
                            >{change.content}</span
                          >
                        {/if}
                      </div>
                      {#if commentKey === `${file.path}:${hunk.header}:${i}`}
                        <div
                          class="sticky left-0 w-cqi-comment my-1 mx-2 border border-border rounded-lg bg-bg-elevated overflow-hidden box-border animate-slide-down-in motion-reduce:animate-none"
                        >
                          <textarea
                            class="w-full min-h-16 px-4 py-2.5 border-0 bg-bg-elevated text-text font-inherit text-sm leading-normal resize-none outline-none box-border placeholder:text-text-muted"
                            placeholder="Comment for agent — {file.path}:{getLineNum(change)}"
                            bind:value={commentText}
                            onkeydown={(e) => {
                              if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) sendComment()
                              if (e.key === 'Escape') closeComment()
                            }}
                          ></textarea>
                          <div
                            class="flex items-center px-4 py-1.5 bg-bg-elevated border-t border-border-subtle"
                          >
                            <span class="text-xs text-text-secondary flex-1">
                              {navigator.userAgent.includes('Mac') ? '⌘' : 'Ctrl'}+Enter to send
                            </span>
                            <div class="flex gap-1">
                              <button
                                class="px-2.5 py-0.5 border border-border-subtle bg-transparent text-text-muted rounded-md text-xs cursor-pointer font-inherit hover:text-text hover:bg-hover"
                                onclick={closeComment}>Cancel</button
                              >
                              <button
                                class="px-2.5 py-0.5 border-0 bg-accent text-bg rounded-md text-xs font-semibold cursor-pointer font-inherit enabled:hover:brightness-110 disabled:opacity-35 disabled:cursor-default disabled:filter-none"
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
