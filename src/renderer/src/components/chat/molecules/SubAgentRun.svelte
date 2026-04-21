<script lang="ts">
  import type { Snippet } from 'svelte'
  import { ChevronRight } from '@lucide/svelte'
  import StatusDot from '../atoms/StatusDot.svelte'
  import SubAgentBadge from '../atoms/SubAgentBadge.svelte'
  import TokenCount from '../atoms/TokenCount.svelte'

  type Status = 'running' | 'success' | 'error'

  interface Props {
    agentType: string
    task?: string
    status?: Status
    model?: string
    tokens?: number
    elapsedMs?: number
    defaultOpen?: boolean
    maxBodyHeight?: number
    body: Snippet
    summary?: Snippet
  }

  let {
    agentType,
    task,
    status = 'success',
    model,
    tokens,
    elapsedMs,
    defaultOpen = false,
    maxBodyHeight = 320,
    body,
    summary,
  }: Props = $props()

  // Default-open when running so users see live progress; callers can override.
  let open = $state(defaultOpen || status === 'running')

  let dotStatus = $derived.by(() => {
    if (status === 'running') return 'thinking' as const
    if (status === 'error') return 'error' as const
    return 'success' as const
  })

  let elapsedText = $derived.by(() => {
    if (elapsedMs === undefined) return null
    if (elapsedMs < 1000) return `${elapsedMs}ms`
    const s = elapsedMs / 1000
    if (s < 60) return `${s.toFixed(s < 10 ? 1 : 0)}s`
    const m = Math.floor(s / 60)
    const rem = Math.floor(s % 60)
    return `${m}m ${rem}s`
  })
</script>

<section class="sub-agent" class:open data-status={status}>
  <button class="head" type="button" aria-expanded={open} onclick={() => (open = !open)}>
    <ChevronRight class="chevron" size={14} />
    <SubAgentBadge type={agentType} />
    <StatusDot status={dotStatus} pulse={status === 'running'} size={7} />
    {#if task}
      <span class="task">{task}</span>
    {:else}
      <span class="task task-empty">sub-agent run</span>
    {/if}
    <span class="meta">
      {#if model}
        <span class="model">{model}</span>
      {/if}
      {#if elapsedText}
        <span class="elapsed">{elapsedText}</span>
      {/if}
      {#if tokens !== undefined}
        <TokenCount {tokens} />
      {/if}
    </span>
  </button>

  {#if open}
    <div class="body" style="max-height: {maxBodyHeight}px;" aria-label="Sub-agent activity">
      {@render body()}
    </div>

    {#if summary}
      <div class="summary-section">
        <div class="summary-label">Result</div>
        <div class="summary-content">{@render summary()}</div>
      </div>
    {/if}
  {/if}
</section>

<style>
  .sub-agent {
    position: relative;
    margin: 8px 0;
    border-radius: 8px;
    border: 1px solid color-mix(in srgb, var(--c-generate) 28%, transparent);
    background: color-mix(in srgb, var(--c-generate) 5%, transparent);
    overflow: hidden;
  }

  /* Left rail makes the nested context unmistakable. */
  .sub-agent::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: 3px;
    background: var(--c-generate);
    opacity: 0.55;
  }

  .sub-agent[data-status='error']::before {
    background: var(--c-danger);
    opacity: 0.7;
  }

  .sub-agent[data-status='running']::before {
    animation: rail-pulse 1.4s ease-in-out infinite;
  }

  @keyframes rail-pulse {
    0%,
    100% {
      opacity: 0.55;
    }
    50% {
      opacity: 1;
    }
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px 8px 14px;
    background: transparent;
    border: none;
    color: var(--c-text);
    text-align: left;
    cursor: pointer;
    font-size: 12.5px;
    line-height: 1.4;
  }

  .head:hover {
    background: var(--c-hover);
  }

  .head:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 2px var(--c-focus-ring);
  }

  .head :global(.chevron) {
    transition: transform 0.15s ease;
    color: var(--c-text-muted);
    flex-shrink: 0;
  }

  .sub-agent.open .head :global(.chevron) {
    transform: rotate(90deg);
  }

  .task {
    flex: 1;
    min-width: 0;
    color: var(--c-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }

  .task-empty {
    color: var(--c-text-muted);
    font-style: italic;
  }

  .meta {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    color: var(--c-text-muted);
    font-size: 11px;
  }

  .model {
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 10.5px;
    color: var(--c-text-muted);
  }

  .elapsed {
    font-variant-numeric: tabular-nums;
    color: var(--c-text-muted);
  }

  .body {
    padding: 8px 12px 10px 14px;
    border-top: 1px solid color-mix(in srgb, var(--c-generate) 20%, transparent);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: transparent;
  }

  .summary-section {
    padding: 8px 12px 10px 14px;
    border-top: 1px solid color-mix(in srgb, var(--c-generate) 20%, transparent);
    background: color-mix(in srgb, var(--c-bg-elevated) 50%, transparent);
  }

  .summary-label {
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--c-generate);
    margin-bottom: 6px;
  }

  .summary-content {
    font-size: 12.5px;
    color: var(--c-text);
    line-height: 1.55;
  }

  .summary-content :global(p) {
    margin: 0 0 6px;
  }

  .summary-content :global(p:last-child) {
    margin-bottom: 0;
  }
</style>
