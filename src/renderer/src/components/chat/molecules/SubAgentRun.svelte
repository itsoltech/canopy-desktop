<script lang="ts">
  import type { Snippet } from 'svelte'
  import { slide } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
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
    /** When true, force-opens the block regardless of internal toggle state. */
    forceOpen?: boolean
    maxBodyHeight?: number
    activityKey?: string
    hasSummary?: boolean
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
    forceOpen = false,
    maxBodyHeight = 320,
    activityKey = '',
    hasSummary = false,
    body,
    summary,
  }: Props = $props()

  // Default-open when running so users see live progress; callers can override.
  let internalOpen = $state(defaultOpen)
  let open = $derived(forceOpen || internalOpen)
  let bodyEl: HTMLDivElement | undefined = $state()
  let autoScrollBody = $state(true)
  let scrollFrame: number | null = null

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

  function onBodyScroll(): void {
    if (!bodyEl) return
    const distance = bodyEl.scrollHeight - bodyEl.scrollTop - bodyEl.clientHeight
    autoScrollBody = distance < 48
  }

  function scheduleBodyScroll(): void {
    if (!open || !bodyEl || !autoScrollBody) return
    if (scrollFrame !== null) return
    scrollFrame = requestAnimationFrame(() => {
      scrollFrame = null
      if (!bodyEl) return
      bodyEl.scrollTop = bodyEl.scrollHeight
    })
  }

  $effect(() => {
    void activityKey
    scheduleBodyScroll()
  })

  $effect(() => {
    if (open) scheduleBodyScroll()
  })

  $effect(() => {
    return () => {
      if (scrollFrame !== null) cancelAnimationFrame(scrollFrame)
    }
  })
</script>

<section class="sub-agent" class:open data-status={status}>
  <button class="head" type="button" aria-expanded={open} onclick={() => (internalOpen = !open)}>
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
    <div class="accordion-panel" transition:slide={{ duration: 180, easing: cubicOut }}>
      <div
        class="body"
        style="max-height: {maxBodyHeight}px;"
        aria-label="Sub-agent activity"
        bind:this={bodyEl}
        onscroll={onBodyScroll}
      >
        {@render body()}
      </div>

      {#if summary && hasSummary}
        <div class="summary-section">
          <div class="summary-label">Result</div>
          <div class="summary-content">{@render summary()}</div>
        </div>
      {/if}
    </div>
  {/if}
</section>

<style>
  .sub-agent {
    position: relative;
    margin: 0;
    border-radius: 0;
    border: 1px solid transparent;
    border-left: 2px solid transparent;
    background: transparent;
    overflow: hidden;
    font-family: inherit;
    font-size: 0.95em;
    transition:
      border-color 0.14s ease,
      background-color 0.14s ease,
      opacity 0.14s ease;
  }

  .sub-agent:not(.open) {
    opacity: 0.55;
  }

  .sub-agent:hover,
  .sub-agent:focus-within {
    border-color: color-mix(in srgb, var(--c-generate) 28%, transparent);
    border-left-color: var(--c-generate);
    background: color-mix(in srgb, var(--c-bg) 88%, black);
    opacity: 1;
  }

  /* Left rail makes the nested context unmistakable. */
  .sub-agent::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: 0;
    background: var(--c-generate);
    opacity: 0;
    transition: opacity 0.14s ease;
  }

  .sub-agent[data-status='error']::before {
    background: var(--c-danger);
  }

  .sub-agent:hover::before,
  .sub-agent:focus-within::before {
    opacity: 0.55;
  }

  .sub-agent[data-status='error']:hover::before,
  .sub-agent[data-status='error']:focus-within::before {
    opacity: 0.7;
  }

  .sub-agent[data-status='running']:hover::before,
  .sub-agent[data-status='running']:focus-within::before {
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
    display: inline-flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 9px 6px 0;
    background: transparent;
    border: none;
    color: var(--c-text);
    text-align: left;
    cursor: pointer;
    font-size: 1em;
    line-height: 1.4;
    color: color-mix(in srgb, var(--c-text) 72%, transparent);
    transition:
      color 0.14s ease,
      padding-left 0.14s ease;
  }

  .sub-agent:hover .head,
  .sub-agent:focus-within .head,
  .sub-agent.open .head {
    color: var(--c-text);
    padding-left: 9px;
  }

  .head:hover {
    background: color-mix(in srgb, var(--c-hover) 70%, transparent);
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
    font-size: 0.95em;
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
    font-size: 0.84em;
  }

  .model {
    font-family: inherit;
    font-size: 1em;
    color: var(--c-text-muted);
  }

  .elapsed {
    font-variant-numeric: tabular-nums;
    color: var(--c-text-muted);
  }

  .body {
    padding: 7px 9px 8px;
    border-top: 1px solid color-mix(in srgb, var(--c-generate) 20%, transparent);
    overflow-y: auto;
    display: inline-flex;
    flex-direction: column;
    width: 100%;
    gap: 6px;
    background: transparent;
  }

  .summary-section {
    padding: 7px 9px 8px;
    border-top: 1px solid color-mix(in srgb, var(--c-generate) 20%, transparent);
    background: color-mix(in srgb, var(--c-bg) 82%, black);
  }

  .summary-label {
    font-size: 0.8em;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--c-generate);
    margin-bottom: 6px;
  }

  .summary-content {
    font-size: 0.95em;
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
