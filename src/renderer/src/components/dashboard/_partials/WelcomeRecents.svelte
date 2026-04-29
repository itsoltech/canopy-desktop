<script lang="ts">
  import { Search, FolderOpen, FolderPlus } from '@lucide/svelte'
  import RecentRow from './RecentRow.svelte'

  interface WorkspaceRow {
    id: string
    path: string
    name: string
    is_git_repo: number
    last_opened: string | null
    cached_branch: string | null
    cached_dirty: number | null
    cached_ahead_behind: string | null
    cached_worktree_count: number | null
  }

  interface Props {
    workspaces: WorkspaceRow[]
    filteredWorkspaces: WorkspaceRow[]
    filter: string
    showFilter: boolean
    selectedIndex: number
    modKey: string
    onOpen: (ws: WorkspaceRow) => void
    onSelect: (i: number) => void
    onContextMenu: (e: MouseEvent, ws: WorkspaceRow) => void
    onOpenFolder: () => void
    onOpenFromPath: () => void
    filterInputEl?: HTMLInputElement
    listEl?: HTMLDivElement
  }

  let {
    workspaces,
    filteredWorkspaces,
    filter = $bindable(),
    showFilter,
    selectedIndex,
    modKey,
    onOpen,
    onSelect,
    onContextMenu,
    onOpenFolder,
    onOpenFromPath,
    filterInputEl = $bindable(),
    listEl = $bindable(),
  }: Props = $props()
</script>

<div class="flex items-center justify-between h-7">
  <span class="text-xs font-semibold tracking-caps uppercase text-text-faint select-none">
    Canopy
  </span>
  <span class="flex items-center gap-3 text-2xs text-text-faint select-none">
    <span class="inline-flex items-center gap-1">
      <kbd class="font-sans">{modKey}O</kbd>
      <span>open</span>
    </span>
    {#if showFilter}
      <span class="inline-flex items-center gap-1">
        <kbd class="font-sans">/</kbd>
        <span>filter</span>
      </span>
    {/if}
  </span>
</div>

{#if showFilter}
  <div
    class="relative flex items-center h-8 rounded-md bg-bg-input border border-transparent focus-within:border-focus-ring transition-colors duration-fast"
  >
    <Search size={13} class="absolute left-2.5 text-text-faint pointer-events-none shrink-0" />
    <input
      bind:this={filterInputEl}
      type="text"
      placeholder="Filter recents…"
      aria-label="Filter recents"
      spellcheck="false"
      autocomplete="off"
      bind:value={filter}
      class="w-full h-full pl-7 pr-3 bg-transparent border-0 outline-none text-md text-text placeholder:text-text-faint font-inherit"
    />
  </div>
{/if}

<section class="flex flex-col gap-2">
  <div class="flex items-baseline justify-between">
    <h2
      class="text-2xs font-semibold uppercase tracking-caps-looser text-text-faint m-0 leading-tight"
    >
      Recent
    </h2>
    <span class="text-2xs text-text-faint tabular-nums">
      {filteredWorkspaces.length}{filter ? ` / ${workspaces.length}` : ''}
    </span>
  </div>

  {#if filteredWorkspaces.length === 0}
    <p class="text-sm text-text-muted py-8 text-center m-0">
      No matches for <span class="font-mono text-text">{filter}</span>
    </p>
  {:else}
    <div bind:this={listEl} role="listbox" aria-label="Recent workspaces" class="flex flex-col">
      {#each filteredWorkspaces as ws, i (ws.id)}
        <RecentRow
          {ws}
          selected={i === selectedIndex}
          onopen={() => onOpen(ws)}
          onfocus={() => onSelect(i)}
          onmouseenter={() => onSelect(i)}
          oncontextmenu={(e) => onContextMenu(e, ws)}
        />
      {/each}
    </div>
  {/if}
</section>

<div class="flex items-center gap-2 pt-2 border-t border-border-subtle">
  <button
    type="button"
    class="inline-flex items-center gap-2 px-3 h-8 rounded-md text-md font-inherit cursor-pointer border-0 outline-none bg-hover text-text-secondary transition-colors duration-fast hover:bg-hover-strong hover:text-text focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-2"
    onclick={onOpenFolder}
  >
    <FolderOpen size={13} />
    Open Folder
  </button>
  <button
    type="button"
    class="inline-flex items-center gap-2 px-3 h-8 rounded-md text-md font-inherit cursor-pointer border-0 outline-none bg-hover text-text-secondary transition-colors duration-fast hover:bg-hover-strong hover:text-text focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-2"
    onclick={onOpenFromPath}
  >
    <FolderPlus size={13} />
    Open from Path…
  </button>
  <span class="text-2xs text-text-faint ml-auto select-none flex items-center gap-3">
    <span class="inline-flex items-center gap-1">
      <kbd class="font-sans">↑↓</kbd>
      <span>navigate</span>
    </span>
    <span class="inline-flex items-center gap-1">
      <kbd class="font-sans">↵</kbd>
      <span>open</span>
    </span>
    <span class="inline-flex items-center gap-1">
      <kbd class="font-sans">{modKey}⌫</kbd>
      <span>remove</span>
    </span>
  </span>
</div>
