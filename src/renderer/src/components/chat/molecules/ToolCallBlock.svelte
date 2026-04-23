<script module lang="ts">
  let cachedHomeDir: string | null = null
  let homeDirPromise: Promise<string> | null = null

  function loadHomeDir(): Promise<string> {
    if (cachedHomeDir !== null) return Promise.resolve(cachedHomeDir)
    if (!homeDirPromise) {
      homeDirPromise = window.api
        .getHomedir()
        .then((home) => {
          cachedHomeDir = home
          return home
        })
        .catch(() => '')
    }
    return homeDirPromise
  }
</script>

<script lang="ts">
  import { slide } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import { ChevronRight } from '@lucide/svelte'
  import StatusDot from '../atoms/StatusDot.svelte'
  import TypingDots from '../atoms/TypingDots.svelte'
  import EditToolView from './EditToolView.svelte'
  import ReadToolView from './ReadToolView.svelte'
  import BashToolView from './BashToolView.svelte'
  import SearchToolView from './SearchToolView.svelte'
  import SkillToolView from './SkillToolView.svelte'

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
  let homeDir = $state(cachedHomeDir ?? '')

  $effect(() => {
    let cancelled = false
    loadHomeDir().then((home) => {
      if (!cancelled) homeDir = home
    })
    return () => {
      cancelled = true
    }
  })

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
  const SKILL_TOOLS = new Set(['skill', 'Skill'])

  type Kind = 'edit' | 'read' | 'bash' | 'grep' | 'glob' | 'web_search' | 'skill' | null

  let kind = $derived.by((): Kind => {
    if (EDIT_TOOLS.has(name)) return 'edit'
    if (READ_TOOLS.has(name)) return 'read'
    if (BASH_TOOLS.has(name)) return 'bash'
    if (GREP_TOOLS.has(name)) return 'grep'
    if (GLOB_TOOLS.has(name)) return 'glob'
    if (WEB_SEARCH_TOOLS.has(name)) return 'web_search'
    if (SKILL_TOOLS.has(name)) return 'skill'
    return null
  })

  function escapeRegExp(value: string): string {
    return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  }

  function redactHome(value: string): string {
    if (!homeDir) return value
    const normalizedHome = homeDir.replace(/\\/g, '/').replace(/\/+$/, '')
    const variants = new Set([homeDir.replace(/[/\\]+$/, ''), normalizedHome])
    let redacted = value
    for (const variant of variants) {
      if (!variant) continue
      const pattern = new RegExp(`${escapeRegExp(variant)}(?=$|[/\\\\])`, 'g')
      redacted = redacted.replace(pattern, '~')
    }
    return redacted
  }

  function redactUnknown(value: unknown): unknown {
    if (typeof value === 'string') return redactHome(value)
    if (Array.isArray(value)) return value.map(redactUnknown)
    if (!value || typeof value !== 'object') return value
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>).map(([key, child]) => [
        key,
        redactUnknown(child),
      ]),
    )
  }

  let displayInput = $derived(input ? (redactUnknown(input) as Record<string, unknown>) : undefined)
  let displayArgs = $derived(args !== undefined ? redactHome(args) : undefined)
  let displayResult = $derived(result !== undefined ? redactHome(result) : undefined)

  let fallbackArgs = $derived.by(() => {
    if (displayArgs !== undefined) return displayArgs
    if (displayInput !== undefined) {
      try {
        return JSON.stringify(displayInput, null, 2)
      } catch {
        return String(displayInput)
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
    if (!kind || !displayInput) return null

    if (kind === 'edit') {
      return str(displayInput, 'file_path', 'filePath', 'path') ?? null
    }

    if (kind === 'read') {
      const path = str(displayInput, 'file_path', 'filePath', 'path')
      if (!path) return null
      const start = num(displayInput, 'offset', 'startLine', 'start_line')
      const limit = num(displayInput, 'limit', 'numLines', 'num_lines')
      const end =
        start !== undefined && limit !== undefined
          ? start + limit - 1
          : num(displayInput, 'endLine', 'end_line')
      if (start === undefined) return path
      if (end === undefined || end === start) return `${path}:${start}`
      return `${path}:${start}-${end}`
    }

    if (kind === 'bash') {
      return str(displayInput, 'description', 'summary', 'command', 'cmd', 'script') ?? null
    }

    if (kind === 'grep') {
      const pattern = str(displayInput, 'pattern', 'query', 'q', 'search')
      const path = str(displayInput, 'path', 'dir', 'directory')
      if (!pattern) return null
      return path ? `"${pattern}" · ${path}` : `"${pattern}"`
    }

    if (kind === 'glob') {
      return str(displayInput, 'pattern', 'query') ?? null
    }

    if (kind === 'web_search') {
      const q = str(displayInput, 'query', 'q', 'search')
      return q ? `"${q}"` : null
    }

    if (kind === 'skill') {
      return str(displayInput, 'skill', 'name') ?? null
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
    <div class="tool-body" transition:slide={{ duration: 160, easing: cubicOut }}>
      {#if kind === 'edit'}
        <EditToolView input={displayInput} result={displayResult} />
      {:else if kind === 'read'}
        <ReadToolView input={displayInput} result={displayResult} />
      {:else if kind === 'bash'}
        <BashToolView input={displayInput} result={displayResult} />
      {:else if kind === 'grep' || kind === 'glob' || kind === 'web_search'}
        <SearchToolView {kind} input={displayInput} result={displayResult} />
      {:else if kind === 'skill'}
        <SkillToolView input={displayInput} result={displayResult} />
      {:else}
        {#if fallbackArgs}
          <div class="section">
            <div class="section-label">Arguments</div>
            <pre class="section-code">{fallbackArgs}</pre>
          </div>
        {/if}
        {#if displayResult}
          <div class="section">
            <div class="section-label">Result</div>
            <pre class="section-code">{displayResult}</pre>
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
    margin: 1px 0;
    border: 0;
    border-left: 2px solid transparent;
    border-radius: 0;
    background: transparent;
    overflow: hidden;
    font-family: inherit;
    font-size: 0.95em;
    transition:
      border-color 0.14s ease,
      background-color 0.14s ease,
      opacity 0.14s ease;
  }

  .tool-call:not(.open) {
    opacity: 0.55;
  }

  .tool-call:hover,
  .tool-call:focus-within {
    border-left-color: var(--c-generate);
    background: color-mix(in srgb, var(--c-generate) 4%, transparent);
    opacity: 1;
  }

  .tool-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px 5px 0;
    background: transparent;
    border: none;
    color: var(--c-text);
    font-size: 1em;
    cursor: pointer;
    text-align: left;
    width: 100%;
    color: color-mix(in srgb, var(--c-text) 70%, transparent);
    transition:
      color 0.14s ease,
      padding-left 0.14s ease;
  }

  .tool-call:hover .tool-head,
  .tool-call:focus-within .tool-head,
  .tool-call.open .tool-head {
    color: var(--c-text);
    padding-left: 8px;
  }

  .tool-head:hover {
    background: color-mix(in srgb, var(--c-hover) 70%, transparent);
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
    font-family: inherit;
    font-weight: 500;
    color: var(--c-text);
    flex-shrink: 0;
    white-space: nowrap;
  }

  .tool-summary {
    flex: 1;
    min-width: 0;
    font-family: inherit;
    font-size: 0.92em;
    color: var(--c-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tool-status {
    font-size: 0.8em;
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
    padding: 7px 8px 8px;
    border-top: 1px solid var(--c-border-subtle);
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .section-label {
    font-size: 0.8em;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--c-text-muted);
  }

  .section-code {
    margin: 0;
    padding: 8px 10px;
    font-family: inherit;
    font-size: 0.92em;
    line-height: 1.5;
    color: var(--c-text);
    background: var(--c-bg);
    border: 1px solid var(--c-border-subtle);
    border-radius: 0;
    overflow-x: auto;
    white-space: pre;
    -webkit-user-select: text;
    user-select: text;
  }
</style>
