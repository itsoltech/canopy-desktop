<script lang="ts">
  import { ChevronRight } from '@lucide/svelte'
  import StatusDot from '../atoms/StatusDot.svelte'
  import TypingDots from '../atoms/TypingDots.svelte'
  import EditToolView from './EditToolView.svelte'
  import ReadToolView from './ReadToolView.svelte'
  import BashToolView from './BashToolView.svelte'
  import SearchToolView from './SearchToolView.svelte'

  type ToolStatus = 'running' | 'success' | 'error'

  interface Props {
    name: string
    status?: ToolStatus
    /** Structured input object — preferred for known tools. */
    input?: Record<string, unknown>
    /** Raw args string — fallback for unknown tools. */
    args?: string
    result?: string
    defaultOpen?: boolean
  }

  let { name, status = 'success', input, args, result, defaultOpen = false }: Props = $props()

  let open = $state(defaultOpen)

  let dotStatus = $derived.by(() => {
    if (status === 'running') return 'thinking' as const
    if (status === 'error') return 'error' as const
    return 'success' as const
  })

  // Normalize tool name → renderer kind. Covers common naming variations.
  const EDIT_TOOLS = new Set([
    'edit',
    'Edit',
    'write',
    'Write',
    'str_replace',
    'str_replace_based_edit_tool',
    'MultiEdit',
    'multi_edit',
  ])
  const READ_TOOLS = new Set(['read', 'Read', 'read_file'])
  const BASH_TOOLS = new Set(['bash', 'Bash', 'run_command', 'shell'])
  const GREP_TOOLS = new Set(['grep', 'Grep'])
  const GLOB_TOOLS = new Set(['glob', 'Glob'])
  const WEB_SEARCH_TOOLS = new Set(['web_search', 'WebSearch', 'websearch'])

  type Kind = 'edit' | 'read' | 'bash' | 'grep' | 'glob' | 'web_search' | null

  let kind = $derived.by((): Kind => {
    if (EDIT_TOOLS.has(name)) return 'edit'
    if (READ_TOOLS.has(name)) return 'read'
    if (BASH_TOOLS.has(name)) return 'bash'
    if (GREP_TOOLS.has(name)) return 'grep'
    if (GLOB_TOOLS.has(name)) return 'glob'
    if (WEB_SEARCH_TOOLS.has(name)) return 'web_search'
    return null
  })

  let fallbackArgs = $derived.by(() => {
    if (args !== undefined) return args
    if (input !== undefined) {
      try {
        return JSON.stringify(input, null, 2)
      } catch {
        return String(input)
      }
    }
    return undefined
  })

  function str(obj: Record<string, unknown> | undefined, ...keys: string[]): string | undefined {
    if (!obj) return undefined
    for (const k of keys) {
      const v = obj[k]
      if (typeof v === 'string' && v.length > 0) return v
    }
    return undefined
  }

  function num(obj: Record<string, unknown> | undefined, ...keys: string[]): number | undefined {
    if (!obj) return undefined
    for (const k of keys) {
      const v = obj[k]
      if (typeof v === 'number' && Number.isFinite(v)) return v
    }
    return undefined
  }

  // Short inline summary shown next to the tool name in the header — lets the
  // user know what this call is about without opening the accordion.
  let headerSummary = $derived.by((): string | null => {
    if (!kind || !input) return null

    if (kind === 'edit') {
      return str(input, 'file_path', 'filePath', 'path') ?? null
    }

    if (kind === 'read') {
      const path = str(input, 'file_path', 'filePath', 'path')
      if (!path) return null
      const start = num(input, 'offset', 'startLine', 'start_line')
      const limit = num(input, 'limit', 'numLines', 'num_lines')
      const end =
        start !== undefined && limit !== undefined
          ? start + limit - 1
          : num(input, 'endLine', 'end_line')
      if (start === undefined) return path
      if (end === undefined || end === start) return `${path}:${start}`
      return `${path}:${start}-${end}`
    }

    if (kind === 'bash') {
      return str(input, 'command', 'cmd', 'script') ?? null
    }

    if (kind === 'grep') {
      const pattern = str(input, 'pattern', 'query', 'q', 'search')
      const path = str(input, 'path', 'dir', 'directory')
      if (!pattern) return null
      return path ? `"${pattern}" · ${path}` : `"${pattern}"`
    }

    if (kind === 'glob') {
      return str(input, 'pattern', 'query') ?? null
    }

    if (kind === 'web_search') {
      const q = str(input, 'query', 'q', 'search')
      return q ? `"${q}"` : null
    }

    return null
  })
</script>

<section class="tool-call" class:open>
  <button class="tool-head" type="button" aria-expanded={open} onclick={() => (open = !open)}>
    <ChevronRight class="chevron" size={14} />
    <StatusDot status={dotStatus} pulse={status === 'running'} size={7} />
    <span class="tool-main">
      <span class="tool-name">{name}</span>
      {#if headerSummary}
        <span class="tool-summary" title={headerSummary}>{headerSummary}</span>
      {/if}
    </span>
    {#if status === 'running'}
      <span class="tool-spinner" aria-label="running"><TypingDots label="Running" /></span>
    {:else}
      <span class="tool-status">{status}</span>
    {/if}
  </button>

  {#if open}
    <div class="tool-body">
      {#if kind === 'edit'}
        <EditToolView {input} {result} />
      {:else if kind === 'read'}
        <ReadToolView {input} {result} />
      {:else if kind === 'bash'}
        <BashToolView {input} {result} />
      {:else if kind === 'grep' || kind === 'glob' || kind === 'web_search'}
        <SearchToolView {kind} {input} {result} />
      {:else}
        {#if fallbackArgs}
          <div class="section">
            <div class="section-label">Arguments</div>
            <pre class="section-code">{fallbackArgs}</pre>
          </div>
        {/if}
        {#if result}
          <div class="section">
            <div class="section-label">Result</div>
            <pre class="section-code">{result}</pre>
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</section>

<style>
  .tool-call {
    display: flex;
    flex-direction: column;
    margin: 6px 0;
    border: 1px solid var(--c-border-subtle);
    border-radius: 6px;
    background: color-mix(in srgb, var(--c-generate) 6%, transparent);
    overflow: hidden;
  }

  .tool-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    background: transparent;
    border: none;
    color: var(--c-text);
    font-size: 12.5px;
    cursor: pointer;
    text-align: left;
    width: 100%;
  }

  .tool-head:hover {
    background: var(--c-hover);
  }

  .tool-head:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 2px var(--c-focus-ring);
  }

  .tool-head :global(.chevron) {
    transition: transform 0.15s ease;
    color: var(--c-text-muted);
    flex-shrink: 0;
  }

  .tool-call.open .tool-head :global(.chevron) {
    transform: rotate(90deg);
  }

  .tool-main {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .tool-name {
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-weight: 500;
    color: var(--c-text);
    flex-shrink: 0;
    white-space: nowrap;
  }

  .tool-summary {
    flex: 1;
    min-width: 0;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px;
    color: var(--c-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tool-status {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--c-text-muted);
    flex-shrink: 0;
  }

  .tool-spinner {
    display: inline-flex;
    align-items: center;
    color: var(--c-accent);
    font-size: 13px;
    flex-shrink: 0;
  }

  .tool-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px 10px;
    border-top: 1px solid var(--c-border-subtle);
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .section-label {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--c-text-muted);
  }

  .section-code {
    margin: 0;
    padding: 8px 10px;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--c-text);
    background: var(--c-bg);
    border: 1px solid var(--c-border-subtle);
    border-radius: 4px;
    overflow-x: auto;
    white-space: pre;
    -webkit-user-select: text;
    user-select: text;
  }
</style>
