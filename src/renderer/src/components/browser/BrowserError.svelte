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
    background: rgba(0, 0, 0, 0.85);
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
    color: rgba(255, 255, 255, 0.9);
  }

  .error-url {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    word-break: break-all;
  }

  .error-desc {
    font-size: 12px;
    color: rgba(255, 130, 130, 0.8);
  }

  .retry-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    padding: 6px 16px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.8);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .retry-btn:hover {
    background: rgba(255, 255, 255, 0.1);
  }
</style>
