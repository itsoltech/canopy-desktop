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
  import { Shield } from '@lucide/svelte'
  import AttentionBanner from './AttentionBanner.svelte'
  import InlineCode from '../atoms/InlineCode.svelte'
  import TypingDots from '../atoms/TypingDots.svelte'

  export type PermissionDecision = 'allow-once' | 'allow-session' | 'deny'
  type Status = 'waiting' | 'submitting' | 'granted' | 'denied'

  interface Props {
    /** Tool name (e.g., "Edit", "Bash", "Read"). */
    tool: string
    /** Structured tool input. Preferred over `args` — yields readable summary for known tools. */
    input?: Record<string, unknown>
    /** Raw args JSON string — fallback for unknown tools. */
    args?: string
    /** Why does the assistant need this tool — 1 short sentence is ideal. */
    reason?: string
    status?: Status
    /** Prior decision to surface in the resolved view. */
    priorDecision?: PermissionDecision
    onrespond?: (decision: PermissionDecision) => void
  }

  let { tool, input, args, reason, status = 'waiting', priorDecision, onrespond }: Props = $props()
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

  // Lightweight inline registry — mirrors ToolCallBlock but kept local to avoid
  // a shared dependency for this first presentational pass.
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

  function str(obj: Record<string, unknown> | undefined, ...keys: string[]): string | undefined {
    if (!obj) return undefined
    for (const k of keys) {
      const v = obj[k]
      if (typeof v === 'string' && v.length > 0) return v
    }
    return undefined
  }

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
      redacted = redacted.replace(new RegExp(`${escapeRegExp(variant)}(?=$|[/\\\\])`, 'g'), '~')
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

  let kind = $derived.by<Kind>(() => {
    if (EDIT_TOOLS.has(tool)) return 'edit'
    if (READ_TOOLS.has(tool)) return 'read'
    if (BASH_TOOLS.has(tool)) return 'bash'
    if (GREP_TOOLS.has(tool)) return 'grep'
    if (GLOB_TOOLS.has(tool)) return 'glob'
    if (WEB_SEARCH_TOOLS.has(tool)) return 'web_search'
    return null
  })

  let summary = $derived.by((): string | null => {
    if (!displayInput) return null
    if (kind === 'edit' || kind === 'read')
      return str(displayInput, 'file_path', 'filePath', 'path') ?? null
    if (kind === 'bash') return str(displayInput, 'command', 'cmd', 'script') ?? null
    if (kind === 'grep') {
      const pattern = str(displayInput, 'pattern', 'query', 'q', 'search')
      const path = str(displayInput, 'path', 'dir', 'directory')
      if (!pattern) return null
      return path ? `"${pattern}" · ${path}` : `"${pattern}"`
    }
    if (kind === 'glob') return str(displayInput, 'pattern', 'query') ?? null
    if (kind === 'web_search') {
      const q = str(displayInput, 'query', 'q', 'search')
      return q ? `"${q}"` : null
    }
    return null
  })

  let fallbackArgs = $derived.by<string | null>(() => {
    if (displayArgs !== undefined) return displayArgs
    if (displayInput !== undefined) {
      try {
        return JSON.stringify(displayInput, null, 2)
      } catch {
        return String(displayInput)
      }
    }
    return null
  })

  let bannerStatus = $derived.by<'waiting' | 'submitting' | 'resolved' | 'rejected'>(() => {
    if (status === 'granted') return 'resolved'
    if (status === 'denied') return 'rejected'
    return status
  })

  let readonly = $derived(status !== 'waiting')

  let displayTitle = $derived.by(() => {
    if (status === 'granted') return 'Permission granted'
    if (status === 'denied') return 'Permission denied'
    if (status === 'submitting') return 'Applying…'
    return 'Tool permission required'
  })

  let tone = $derived<'warning' | 'accent' | 'danger'>(
    // Destructive-ish tools get a stronger warning hue.
    EDIT_TOOLS.has(tool) || BASH_TOOLS.has(tool) ? 'warning' : 'accent',
  )

  let priorLabel = $derived.by(() => {
    if (!priorDecision) return null
    if (priorDecision === 'allow-once') return 'Allowed once'
    if (priorDecision === 'allow-session') return 'Allowed for session'
    return 'Denied'
  })

  function respond(decision: PermissionDecision): void {
    if (readonly) return
    onrespond?.(decision)
  }
</script>

<AttentionBanner title={displayTitle} icon={Shield} status={bannerStatus} {tone}>
  {#snippet description()}
    {#if status === 'granted' || status === 'denied'}
      {#if priorLabel}
        {priorLabel}.
      {/if}
    {:else}
      The assistant wants to run <InlineCode>{tool}</InlineCode>.
      {#if reason}<span class="reason"> {reason}</span>{/if}
    {/if}
  {/snippet}

  {#snippet body()}
    <div class="call-card">
      <div class="call-head">
        <span class="call-label">Call</span>
        <InlineCode>{tool}</InlineCode>
      </div>
      {#if summary}
        <div class="call-summary" title={summary}>{summary}</div>
      {:else if fallbackArgs}
        <pre class="call-args">{fallbackArgs}</pre>
      {:else}
        <div class="call-summary muted">No arguments.</div>
      {/if}
    </div>
  {/snippet}

  {#snippet actions()}
    {#if status === 'waiting'}
      <button type="button" class="btn primary" onclick={() => respond('allow-once')}>
        Allow once
      </button>
      <button type="button" class="btn ghost-accent" onclick={() => respond('allow-session')}>
        Allow for session
      </button>
      <button type="button" class="btn danger" onclick={() => respond('deny')}>Deny</button>
    {:else if status === 'submitting'}
      <span class="status-inline">
        <TypingDots label="Applying" />
        <span>Applying…</span>
      </span>
    {:else if status === 'granted'}
      <span class="status-inline status-success">Granted</span>
    {:else if status === 'denied'}
      <span class="status-inline status-error">Denied</span>
    {/if}
  {/snippet}
</AttentionBanner>

<style>
  .reason {
    color: var(--c-text-muted);
  }

  .call-card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--c-border-subtle);
    border-radius: 6px;
    background: var(--c-bg);
  }

  .call-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .call-label {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--c-text-muted);
    font-weight: 600;
  }

  .call-summary {
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px;
    color: var(--c-text);
    line-height: 1.45;
    word-break: break-all;
    -webkit-user-select: text;
    user-select: text;
  }

  .call-summary.muted {
    color: var(--c-text-muted);
    font-style: italic;
  }

  .call-args {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11px;
    line-height: 1.5;
    color: var(--c-text);
    white-space: pre;
    overflow-x: auto;
    max-height: 160px;
    overflow-y: auto;
  }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    font-size: 12.5px;
    font-family: inherit;
    cursor: pointer;
    outline: none;
    transition:
      background 0.1s,
      border-color 0.1s,
      color 0.1s;
  }

  .btn:focus-visible {
    box-shadow: 0 0 0 2px var(--c-focus-ring);
  }

  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .btn.primary {
    background: var(--c-accent);
    color: var(--c-bg);
    font-weight: 600;
  }

  .btn.primary:hover:not(:disabled) {
    background: var(--c-accent-text);
  }

  .btn.ghost-accent {
    color: var(--c-accent-text);
    border-color: color-mix(in srgb, var(--c-accent) 40%, transparent);
  }

  .btn.ghost-accent:hover:not(:disabled) {
    background: var(--c-accent-bg);
    color: var(--c-accent);
    border-color: color-mix(in srgb, var(--c-accent) 55%, transparent);
  }

  .btn.danger {
    color: var(--c-danger-text);
    border-color: color-mix(in srgb, var(--c-danger) 35%, transparent);
  }

  .btn.danger:hover:not(:disabled) {
    background: var(--c-danger-bg);
    color: var(--c-danger);
    border-color: var(--c-danger);
  }

  .status-inline {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--c-text-muted);
  }

  .status-inline.status-success {
    color: var(--c-success);
  }

  .status-inline.status-error {
    color: var(--c-danger);
  }
</style>
