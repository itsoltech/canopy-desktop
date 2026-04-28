<script lang="ts">
  import { onMount, tick } from 'svelte'
  import fuzzysort from 'fuzzysort'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { openFile } from '../../lib/stores/tabs.svelte'
  import { getMru } from '../../lib/stores/quickOpenMru.svelte'
  import { forceReload, getFiles, isLoading } from '../../lib/stores/quickOpenStore.svelte'
  import { FolderOpen } from '@lucide/svelte'
  import { getFileIcon } from '../../lib/fileIcons'
  import FileIconDisplay from '../shared/FileIconDisplay.svelte'

  let { onClose }: { onClose: () => void } = $props()

  let query = $state('')
  let selectedIndex = $state(0)
  let inputEl: HTMLInputElement | undefined = $state()
  let listEl: HTMLDivElement | undefined = $state()

  const MAX_RESULTS = 50

  const worktreePath = $derived(workspaceState.selectedWorktreePath ?? '')
  const files = $derived(getFiles(worktreePath))
  const loading = $derived(isLoading(worktreePath))

  type Result = {
    target: string
    path: string
    score: number
    indexes: number[]
    fromMru: boolean
  }

  let matchedResults: Result[] = $state([])

  onMount(() => {
    inputEl?.focus()
    // Kick off a refresh every time the picker opens so newly created files
    // (via shell, an agent, or an external editor) show up without the user
    // having to remember to hit ⇧⌘R. We render cached results immediately —
    // the list updates reactively when the refresh completes.
    void forceReload(worktreePath)
  })

  // Synchronous matching on every keystroke (fuzzysort is ~5-10ms on 50k files)
  $effect(() => {
    void query
    void files
    selectedIndex = 0
    if (query.length === 0 || files.length === 0) {
      matchedResults = []
      return
    }
    const mru = getMru(worktreePath)
    const mruBoost: Record<string, number> = {}
    mru.forEach((p, idx) => (mruBoost[p] = (mru.length - idx) * 0.5))
    const matches = fuzzysort.go(query, files, { limit: MAX_RESULTS, threshold: -10000 })
    const scored = matches.map((m) => {
      const rank = mruBoost[m.target] ?? 0
      return {
        target: m.target,
        path: m.target,
        score: m.score + rank,
        indexes: [...m.indexes],
        fromMru: rank > 0,
      }
    })
    scored.sort((a, b) => b.score - a.score)
    matchedResults = scored
  })

  // Empty-query view: MRU first, then the rest of the file list
  const emptyResults: Result[] = $derived.by(() => {
    if (query.length > 0) return []
    const mru = getMru(worktreePath)
    const fileSet = new Set(files)
    const recent: Result[] = mru
      .filter((p) => fileSet.has(p))
      .slice(0, MAX_RESULTS)
      .map((path) => ({ target: path, score: 0, indexes: [], fromMru: true, path }))
    if (recent.length >= MAX_RESULTS) return recent
    const mruSet = new Set(mru)
    const rest: Result[] = []
    for (const f of files) {
      if (rest.length + recent.length >= MAX_RESULTS) break
      if (mruSet.has(f)) continue
      rest.push({ target: f, score: 0, indexes: [], fromMru: false, path: f })
    }
    return [...recent, ...rest]
  })

  const results: Result[] = $derived(query.length === 0 ? emptyResults : matchedResults)

  $effect(() => {
    void selectedIndex
    void tick().then(() => scrollSelectedIntoView())
  })

  function scrollSelectedIntoView(): void {
    if (!listEl) return
    const el = listEl.querySelector<HTMLElement>(`[data-idx="${selectedIndex}"]`)
    el?.scrollIntoView({ block: 'nearest' })
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      onClose()
      return
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      if (results.length > 0) {
        selectedIndex = (selectedIndex + 1) % results.length
      }
      return
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      if (results.length > 0) {
        selectedIndex = (selectedIndex - 1 + results.length) % results.length
      }
      return
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      openSelected()
      return
    }
    const isMac = navigator.userAgent.includes('Mac')
    const mod = isMac ? e.metaKey : e.ctrlKey
    if (mod && e.shiftKey && (e.key === 'r' || e.key === 'R')) {
      e.preventDefault()
      void forceReload(worktreePath)
    }
  }

  function openSelected(): void {
    const item = results[selectedIndex]
    if (!item || !worktreePath) return
    openFile(`${worktreePath}/${item.path}`, worktreePath)
    onClose()
  }

  function openAt(idx: number): void {
    selectedIndex = idx
    openSelected()
  }

  function dirname(p: string): string {
    const i = p.lastIndexOf('/')
    return i >= 0 ? p.slice(0, i) : ''
  }

  type Segment = { text: string; match: boolean }

  function toSegments(path: string, indexes: readonly number[] | null): Segment[] {
    if (!indexes || indexes.length === 0) return [{ text: path, match: false }]
    const sorted = [...indexes].sort((a, b) => a - b)
    const segs: Segment[] = []
    let cursor = 0
    for (const i of sorted) {
      if (i < cursor) continue
      if (i > cursor) segs.push({ text: path.slice(cursor, i), match: false })
      segs.push({ text: path[i] ?? '', match: true })
      cursor = i + 1
    }
    if (cursor < path.length) segs.push({ text: path.slice(cursor), match: false })
    return segs
  }

  function basenameSegments(path: string, indexes: readonly number[] | null): Segment[] {
    const sep = path.lastIndexOf('/')
    const base = sep >= 0 ? path.slice(sep + 1) : path
    if (!indexes) return [{ text: base, match: false }]
    const offset = sep + 1
    const rel = indexes.filter((i) => i >= offset).map((i) => i - offset)
    return toSegments(base, rel)
  }
</script>

<div
  class="quick-open-backdrop"
  onclick={onClose}
  onkeydown={(e) => {
    if (e.key === 'Escape') onClose()
  }}
  role="presentation"
>
  <div
    class="quick-open"
    onclick={(e) => e.stopPropagation()}
    onkeydown={handleKeydown}
    role="dialog"
    aria-label="Quick Open"
  >
    <div class="quick-open-header">
      <input
        bind:this={inputEl}
        bind:value={query}
        type="text"
        placeholder={loading && files.length === 0 ? 'Indexing files…' : 'Search files by name'}
        spellcheck="false"
        autocomplete="off"
      />
      <span class="quick-open-hint">
        {#if loading && files.length === 0}
          Loading…
        {:else if query.length > 0}
          {results.length}
          {results.length === 1 ? 'match' : 'matches'}
        {:else}
          {files.length.toLocaleString()} files
        {/if}
      </span>
    </div>

    <div class="quick-open-results" bind:this={listEl} role="listbox">
      {#each results as r, idx (r.path)}
        {@const selected = idx === selectedIndex}
        {@const dir = dirname(r.path)}
        {@const iconInfo = getFileIcon(r.path)}
        <button
          type="button"
          class="quick-open-item"
          class:selected
          data-idx={idx}
          onclick={() => openAt(idx)}
          onmouseenter={() => (selectedIndex = idx)}
          role="option"
          aria-selected={selected}
          title={r.path}
        >
          <FileIconDisplay info={iconInfo} size={14} />
          <div class="quick-open-item-text">
            <!-- prettier-ignore -->
            <span class="quick-open-name">{#each basenameSegments(r.path, r.indexes) as seg, sidx (sidx)}{#if seg.match}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span>
            {#if dir}
              <span class="quick-open-dir">
                <FolderOpen size={11} />
                <!-- prettier-ignore -->
                <span class="quick-open-dir-path">{#each toSegments(r.path, r.indexes) as seg, sidx (sidx)}{#if seg.match}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span>
              </span>
            {/if}
          </div>
          {#if r.fromMru && query.length === 0}
            <span class="quick-open-badge">recent</span>
          {/if}
        </button>
      {/each}
      {#if !loading && results.length === 0 && query.length > 0}
        <div class="quick-open-empty">No files match "{query}"</div>
      {/if}
    </div>

    <div class="quick-open-footer">
      <span>↑↓ navigate</span>
      <span>↵ open</span>
      <span>Esc close</span>
      <span>⇧⌘R refresh</span>
    </div>
  </div>
</div>

<style>
  .quick-open-backdrop {
    position: fixed;
    inset: 0;
    background: var(--color-scrim, oklch(0 0 0 / 0.5));
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 10vh;
    z-index: 9999;
  }

  .quick-open {
    width: min(620px, 90vw);
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    box-shadow: 0 20px 60px oklch(0 0 0 / 0.4);
    overflow: hidden;
  }

  .quick-open-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .quick-open-header input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-text);
    font-size: 14px;
    font-family: inherit;
  }

  .quick-open-hint {
    font-size: 11px;
    color: var(--color-text-faint);
    white-space: nowrap;
  }

  .quick-open-results {
    flex: 1;
    overflow-y: auto;
    padding: 4px;
  }

  .quick-open-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: 4px;
    color: var(--color-text);
    cursor: pointer;
    text-align: left;
  }

  .quick-open-item.selected {
    background: var(--color-accent-bg);
  }

  .quick-open-item-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }

  .quick-open-name {
    font-size: 13px;
    color: var(--color-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .quick-open-dir {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .quick-open-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }

  :global(.quick-open-name mark),
  :global(.quick-open-dir mark) {
    background: transparent;
    color: var(--color-accent);
    font-weight: 600;
  }

  .quick-open-badge {
    padding: 1px 6px;
    font-size: 10px;
    border-radius: 3px;
    background: var(--color-hover);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .quick-open-empty {
    padding: 20px;
    text-align: center;
    color: var(--color-text-muted);
    font-size: 12px;
  }

  .quick-open-footer {
    display: flex;
    gap: 14px;
    padding: 6px 14px;
    border-top: 1px solid var(--color-border-subtle);
    font-size: 10px;
    color: var(--color-text-faint);
  }
</style>
