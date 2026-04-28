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
    void forceReload(worktreePath)
  })

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

  const markCls = 'bg-transparent text-accent font-semibold'
</script>

<div
  class="fixed inset-0 bg-scrim flex items-start justify-center pt-[10vh] z-[9999]"
  onclick={onClose}
  onkeydown={(e) => {
    if (e.key === 'Escape') onClose()
  }}
  role="presentation"
>
  <div
    class="w-[min(620px,90vw)] max-h-[70vh] flex flex-col bg-bg-elevated border border-border rounded-xl shadow-[0_20px_60px_oklch(0_0_0/0.4)] overflow-hidden"
    onclick={(e) => e.stopPropagation()}
    onkeydown={handleKeydown}
    role="dialog"
    aria-label="Quick Open"
  >
    <div class="flex items-center gap-2.5 px-3.5 py-2.5 border-b border-border-subtle">
      <input
        bind:this={inputEl}
        bind:value={query}
        class="flex-1 bg-transparent border-0 outline-none text-text text-lg font-inherit"
        type="text"
        placeholder={loading && files.length === 0 ? 'Indexing files…' : 'Search files by name'}
        spellcheck="false"
        autocomplete="off"
      />
      <span class="text-xs text-text-faint whitespace-nowrap">
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

    <div class="flex-1 overflow-y-auto p-1" bind:this={listEl} role="listbox">
      {#each results as r, idx (r.path)}
        {@const selected = idx === selectedIndex}
        {@const dir = dirname(r.path)}
        {@const iconInfo = getFileIcon(r.path)}
        <button
          type="button"
          class="flex items-center gap-2.5 w-full px-2.5 py-1.5 bg-transparent border-0 rounded-md text-text cursor-pointer text-left"
          class:!bg-accent-bg={selected}
          data-idx={idx}
          onclick={() => openAt(idx)}
          onmouseenter={() => (selectedIndex = idx)}
          role="option"
          aria-selected={selected}
          title={r.path}
        >
          <FileIconDisplay info={iconInfo} size={14} />
          <div class="flex flex-col min-w-0 flex-1">
            <!-- prettier-ignore -->
            <span class="text-md text-text overflow-hidden text-ellipsis whitespace-nowrap">{#each basenameSegments(r.path, r.indexes) as seg, sidx (sidx)}{#if seg.match}<mark class={markCls}>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span>
            {#if dir}
              <span
                class="flex items-center gap-1 text-xs text-text-muted overflow-hidden text-ellipsis whitespace-nowrap"
              >
                <FolderOpen size={11} />
                <!-- prettier-ignore -->
                <span class="overflow-hidden text-ellipsis whitespace-nowrap">{#each toSegments(r.path, r.indexes) as seg, sidx (sidx)}{#if seg.match}<mark class={markCls}>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span>
              </span>
            {/if}
          </div>
          {#if r.fromMru && query.length === 0}
            <span
              class="px-1.5 py-px text-2xs rounded-sm bg-hover text-text-muted uppercase tracking-[0.5px]"
              >recent</span
            >
          {/if}
        </button>
      {/each}
      {#if !loading && results.length === 0 && query.length > 0}
        <div class="p-5 text-center text-text-muted text-sm">No files match "{query}"</div>
      {/if}
    </div>

    <div class="flex gap-3.5 px-3.5 py-1.5 border-t border-border-subtle text-2xs text-text-faint">
      <span>↑↓ navigate</span>
      <span>↵ open</span>
      <span>Esc close</span>
      <span>⇧⌘R refresh</span>
    </div>
  </div>
</div>
