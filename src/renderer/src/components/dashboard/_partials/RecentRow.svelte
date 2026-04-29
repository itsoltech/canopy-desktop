<script lang="ts">
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
    ws: WorkspaceRow
    selected: boolean
    onopen: () => void
    onfocus: () => void
    onmouseenter: () => void
    oncontextmenu: (e: MouseEvent) => void
  }

  let { ws, selected, onopen, onfocus, onmouseenter, oncontextmenu }: Props = $props()

  function relativeTime(dateStr: string | null): string {
    if (!dateStr) return ''
    const diff = Date.now() - Date.parse(dateStr)
    const seconds = Math.floor(diff / 1000)
    if (seconds < 60) return 'just now'
    const minutes = Math.floor(seconds / 60)
    if (minutes < 60) return `${minutes}m ago`
    const hours = Math.floor(minutes / 60)
    if (hours < 24) return `${hours}h ago`
    const days = Math.floor(hours / 24)
    if (days === 1) return 'yesterday'
    if (days < 30) return `${days}d ago`
    const months = Math.floor(days / 30)
    return `${months}mo ago`
  }

  function parseAheadBehind(raw: string | null): { ahead: number; behind: number } | null {
    if (!raw) return null
    try {
      const parsed = JSON.parse(raw)
      if (parsed && typeof parsed.ahead === 'number') return parsed
      const parts = raw.split('/')
      if (parts.length === 2) {
        return { ahead: parseInt(parts[0], 10), behind: parseInt(parts[1], 10) }
      }
    } catch {
      const parts = raw.split('/')
      if (parts.length === 2) {
        const ahead = parseInt(parts[0], 10)
        const behind = parseInt(parts[1], 10)
        if (!isNaN(ahead) && !isNaN(behind)) return { ahead, behind }
      }
    }
    return null
  }

  const ab = $derived(parseAheadBehind(ws.cached_ahead_behind))
</script>

<button
  data-row
  role="option"
  tabindex={selected ? 0 : -1}
  aria-selected={selected}
  aria-label={`Open ${ws.name}`}
  class="block w-full px-3 py-2 border-0 border-t border-border-subtle first:border-t-0 rounded-none bg-transparent text-inherit font-inherit text-left cursor-pointer transition-colors duration-fast outline-none hover:bg-hover focus:bg-hover focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:-outline-offset-2 aria-selected:bg-hover"
  onclick={onopen}
  {onfocus}
  {onmouseenter}
  {oncontextmenu}
>
  <div class="flex items-baseline justify-between gap-3">
    <span
      class="text-md font-medium text-text whitespace-nowrap overflow-hidden text-ellipsis"
      title={ws.name}
    >
      {ws.name}
    </span>
    <span class="flex items-baseline gap-1.5 flex-shrink-0 text-xs tabular-nums">
      {#if ws.cached_branch}
        <span class="text-text-muted font-mono">{ws.cached_branch}</span>
      {/if}
      {#if ws.cached_dirty === 1}
        <span class="text-warning-text font-bold" title="Uncommitted changes">*</span>
      {/if}
      {#if ab && ab.ahead > 0}
        <span class="text-success-text text-2xs">{ab.ahead}↑</span>
      {/if}
      {#if ab && ab.behind > 0}
        <span class="text-warning-text text-2xs">{ab.behind}↓</span>
      {/if}
    </span>
  </div>
  <div class="flex items-baseline justify-between gap-3 mt-0.5">
    <span
      class="text-xs font-mono text-text-faint whitespace-nowrap overflow-hidden text-ellipsis"
      title={ws.path}
    >
      {ws.path}
    </span>
    <span class="flex items-baseline gap-2 flex-shrink-0 text-2xs text-text-faint">
      {#if ws.is_git_repo && ws.cached_worktree_count && ws.cached_worktree_count > 1}
        <span>{ws.cached_worktree_count} worktrees</span>
        <span aria-hidden="true">·</span>
      {/if}
      {#if ws.last_opened}
        <span>{relativeTime(ws.last_opened)}</span>
      {/if}
    </span>
  </div>
</button>
