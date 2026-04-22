<script lang="ts">
  import { Square } from '@lucide/svelte'
  import {
    cancel,
    sdkSessions,
    contextPercentFor,
    contextTokensUsedFor,
  } from '../../lib/stores/sdkAgentSessions.svelte'

  interface Props {
    conversationId: string
  }

  let { conversationId }: Props = $props()

  let state = $derived(sdkSessions[conversationId])
  let isStreaming = $derived(state?.status === 'streaming')
  let tokensIn = $derived(state?.tokensIn ?? 0)
  let tokensOut = $derived(state?.tokensOut ?? 0)
  let costUsd = $derived(state?.costUsd ?? 0)
  let contextPercent = $derived(contextPercentFor(state))
  let contextUsed = $derived(contextTokensUsedFor(state))
  let contextWindow = $derived(state?.contextWindow ?? null)
  let pendingCount = $derived(
    state?.pendingAttention.filter((a) => a.status === 'waiting').length ?? 0,
  )

  function formatTokens(n: number): string {
    if (n < 1000) return `${n}`
    if (n < 10_000) return `${(n / 1000).toFixed(1)}k`
    return `${Math.round(n / 1000)}k`
  }

  function formatCost(usd: number): string {
    if (usd === 0) return '—'
    if (usd < 0.01) return `$${usd.toFixed(4)}`
    return `$${usd.toFixed(2)}`
  }
</script>

{#if state}
  <section class="sdk-extras" aria-label="SDK session details">
    <dl class="stats">
      <div class="stat">
        <dt>Status</dt>
        <dd class="status {state.status}">{state.status}</dd>
      </div>
      <div class="stat">
        <dt>Model</dt>
        <dd class="tabular">{state.conversation?.model ?? 'unknown'}</dd>
      </div>
      <div class="stat">
        <dt>Context</dt>
        <dd class="tabular">
          {#if contextPercent != null && contextWindow}
            {formatTokens(contextUsed)} / {formatTokens(contextWindow)} ({contextPercent}%)
          {:else}
            —
          {/if}
        </dd>
      </div>
      <div class="stat">
        <dt>Tokens in</dt>
        <dd class="tabular">{formatTokens(tokensIn)}</dd>
      </div>
      <div class="stat">
        <dt>Tokens out</dt>
        <dd class="tabular">{formatTokens(tokensOut)}</dd>
      </div>
      <div class="stat">
        <dt>Cost (est.)</dt>
        <dd class="tabular">{formatCost(costUsd)}</dd>
      </div>
      {#if pendingCount > 0}
        <div class="stat attention">
          <dt>Waiting on you</dt>
          <dd class="tabular">{pendingCount}</dd>
        </div>
      {/if}
    </dl>

    {#if isStreaming}
      <button
        type="button"
        class="stop-btn"
        onclick={() => void cancel(conversationId)}
        title="Stop generation (⌘.)"
      >
        <Square size={12} />
        <span>Stop</span>
      </button>
    {/if}
  </section>
{:else}
  <section class="sdk-extras placeholder">
    <p>Conversation not yet open.</p>
  </section>
{/if}

<style>
  .sdk-extras {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--c-border-subtle);
    border-radius: 6px;
    background: var(--c-bg-elevated);
  }

  .sdk-extras.placeholder {
    color: var(--c-text-muted);
    font-style: italic;
  }

  .sdk-extras.placeholder p {
    margin: 0;
    font-size: 12px;
  }

  .stats {
    margin: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px 12px;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stat dt {
    margin: 0;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--c-text-muted);
  }

  .stat dd {
    margin: 0;
    font-size: 12.5px;
    color: var(--c-text);
  }

  .tabular {
    font-variant-numeric: tabular-nums;
  }

  .status.streaming {
    color: var(--c-accent-text);
    font-weight: 600;
  }

  .status.ended {
    color: var(--c-text-muted);
  }

  .status.error,
  .status.cancelled {
    color: var(--c-danger);
  }

  .stat.attention dd {
    color: var(--c-warning);
    font-weight: 600;
  }

  .stop-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    align-self: flex-start;
    padding: 4px 10px;
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
    border: 1px solid color-mix(in srgb, var(--c-danger) 40%, transparent);
    border-radius: 4px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .stop-btn:hover {
    background: var(--c-danger);
    color: var(--c-bg);
    border-color: var(--c-danger);
  }
</style>
