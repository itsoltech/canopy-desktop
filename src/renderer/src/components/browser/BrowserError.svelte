<script lang="ts">
  import { RotateCw } from 'lucide-svelte'

  let {
    errorDescription,
    validatedURL,
    onRetry,
    onDismiss,
  }: {
    errorDescription: string
    validatedURL: string
    onRetry: () => void
    onDismiss: () => void
  } = $props()
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="error-overlay" onclick={onDismiss}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="error-content" onclick={(e) => e.stopPropagation()}>
    <div class="error-title">Page failed to load</div>
    <div class="error-url">{validatedURL}</div>
    <div class="error-desc">{errorDescription}</div>
    <button class="retry-btn" onclick={onRetry}>
      <RotateCw size={14} />
      Retry
    </button>
  </div>
</div>

<style>
  .error-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--c-scrim);
    cursor: pointer;
    z-index: 1;
  }

  .error-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    max-width: 400px;
    padding: 24px;
    text-align: center;
    cursor: default;
  }

  .error-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--c-text);
  }

  .error-url {
    font-size: 12px;
    color: var(--c-text-secondary);
    word-break: break-all;
  }

  .error-desc {
    font-size: 12px;
    color: var(--c-danger-text);
  }

  .retry-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    padding: 6px 16px;
    border: 1px solid var(--c-text-faint);
    border-radius: 4px;
    background: var(--c-hover);
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .retry-btn:hover {
    background: var(--c-hover-strong);
  }
</style>
