<script lang="ts">
  import { match } from 'ts-pattern'
  import { AlertTriangle, OctagonAlert, RotateCcw } from '@lucide/svelte'
  import AttentionBanner from './AttentionBanner.svelte'

  /**
   * Inline error card rendered in the message stream when the SDK session
   * emits an `error` event. `errorTag` is the typed `_tag` from
   * SdkAgentError; any free-form message is treated as unknown.
   */
  interface Props {
    errorTag: string
    detail?: string
    canRetry?: boolean
    retrying?: boolean
    onretry?: () => void
  }

  let { errorTag, detail, canRetry = true, retrying = false, onretry }: Props = $props()

  let friendlyMessage = $derived(
    match(errorTag)
      .with('auth_missing', () => 'No API key configured for this profile.')
      .with('auth_invalid', () => 'The API rejected the configured credentials.')
      .with('rate_limited', () => 'Rate limited by the API. Retrying in a moment.')
      .with('network', () => (detail ? `Network error: ${detail}` : 'Network error.'))
      .with('aborted', () => 'Request was aborted.')
      .with('sdk_internal', () => detail ?? 'The agent SDK reported an internal error.')
      .with('profile_not_found', () => 'The referenced agent profile could not be loaded.')
      .otherwise(() => detail ?? 'Something went wrong running the agent.'),
  )

  let title = $derived(
    match(errorTag)
      .with('auth_missing', 'auth_invalid', () => 'Authentication required')
      .with('rate_limited', () => 'Rate limited')
      .with('network', () => 'Network problem')
      .with('aborted', () => 'Request aborted')
      .with('profile_not_found', () => 'Profile unavailable')
      .otherwise(() => 'Assistant error'),
  )

  let isAbort = $derived(errorTag === 'aborted')
</script>

<AttentionBanner
  {title}
  icon={isAbort ? AlertTriangle : OctagonAlert}
  tone={isAbort ? 'warning' : 'danger'}
  status={isAbort ? 'resolved' : 'rejected'}
>
  {#snippet description()}
    {friendlyMessage}
  {/snippet}

  {#snippet actions()}
    {#if canRetry && onretry && !isAbort}
      <button type="button" class="btn danger-outline" onclick={onretry} disabled={retrying}>
        <RotateCcw size={14} />
        <span>{retrying ? 'Retrying…' : 'Retry'}</span>
      </button>
    {/if}
  {/snippet}
</AttentionBanner>

<style>
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
    color: var(--c-danger-text);
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

  .btn.danger-outline {
    border-color: color-mix(in srgb, var(--c-danger) 40%, transparent);
  }

  .btn.danger-outline:hover:not(:disabled) {
    background: var(--c-danger-bg);
    color: var(--c-danger);
    border-color: var(--c-danger);
  }
</style>
