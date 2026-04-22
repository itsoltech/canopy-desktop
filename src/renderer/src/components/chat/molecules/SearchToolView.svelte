<script lang="ts">
  import { Search, Folder, Globe } from '@lucide/svelte'
  import FilePath from '../atoms/FilePath.svelte'
  import InlineCode from '../atoms/InlineCode.svelte'

  type SearchKind = 'grep' | 'glob' | 'web_search'

  interface Props {
    kind: SearchKind
    input?: Record<string, unknown>
    result?: string
    previewMaxLines?: number
  }

  let { kind, input, result, previewMaxLines = 8 }: Props = $props()

  function str(...keys: string[]): string | undefined {
    if (!input) return undefined
    for (const k of keys) {
      const v = input[k]
      if (typeof v === 'string') return v
    }
    return undefined
  }

  let query = $derived(str('pattern', 'query', 'q', 'search') ?? '')
  let path = $derived(str('path', 'dir', 'directory'))
  let globFilter = $derived(str('glob', 'include', 'filePattern'))

  let matchCount = $derived.by(() => {
    if (!result) return null
    const lines = result.split('\n').filter((l) => l.trim().length > 0)
    return lines.length
  })

  let previewText = $derived.by(() => {
    if (!result) return ''
    const lines = result.split('\n')
    const shown = lines.slice(0, previewMaxLines).join('\n')
    const hidden = Math.max(0, lines.length - previewMaxLines)
    if (hidden === 0) return shown
    return `${shown}\n… ${hidden} more line${hidden === 1 ? '' : 's'}`
  })

  let kindLabel = $derived.by(() => {
    if (kind === 'grep') return 'grep'
    if (kind === 'glob') return 'glob'
    return 'web search'
  })
</script>

<div class="search-view">
  <div class="query-row">
    <div class="query-meta">
      <span class="kind">
        {#if kind === 'web_search'}
          <Globe size={11} />
        {:else if kind === 'glob'}
          <Folder size={11} />
        {:else}
          <Search size={11} />
        {/if}
        {kindLabel}
      </span>
      <InlineCode>{query}</InlineCode>
      {#if path}
        <FilePath {path} showIcon={false} />
      {/if}
      {#if globFilter}
        <span class="filter">{globFilter}</span>
      {/if}
    </div>
    {#if matchCount !== null}
      <span class="match-count" class:empty={matchCount === 0}>
        {matchCount}
        {matchCount === 1 ? 'match' : 'matches'}
      </span>
    {/if}
  </div>

  {#if result}
    <pre class="hits">{previewText}</pre>
  {/if}
</div>

<style>
  .search-view {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .query-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .query-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .kind {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--c-text-muted);
    font-weight: 600;
  }

  .filter {
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11px;
    padding: 1px 5px;
    border-radius: 3px;
    background: var(--c-bg-elevated);
    color: var(--c-text-muted);
  }

  .match-count {
    flex-shrink: 0;
    margin-left: auto;
    font-size: 10.5px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 10px;
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .match-count.empty {
    background: var(--c-bg-elevated);
    color: var(--c-text-muted);
  }

  .hits {
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--c-border-subtle);
    border-radius: 4px;
    background: var(--c-bg);
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--c-text);
    overflow-x: auto;
    white-space: pre;
    -webkit-user-select: text;
    user-select: text;
  }
</style>
