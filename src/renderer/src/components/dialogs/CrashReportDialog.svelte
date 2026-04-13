<script lang="ts">
  import { onMount } from 'svelte'
  import { TriangleAlert } from 'lucide-svelte'
  import type { CrashReportData } from '../../lib/stores/dialogs.svelte'

  let {
    data,
    onCreateIssue,
    onDismiss,
  }: {
    data: CrashReportData
    onCreateIssue: () => void
    onDismiss: () => void
  } = $props()

  let dismissBtn: HTMLButtonElement | undefined = $state()

  onMount(() => {
    dismissBtn?.focus()
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onDismiss()
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      e.stopPropagation()
      if (document.activeElement === dismissBtn) {
        onDismiss()
        return
      }
      onCreateIssue()
    }
  }

  const formattedTime = $derived(
    new Date(data.timestamp).toLocaleString(undefined, {
      dateStyle: 'medium',
      timeStyle: 'medium',
    }),
  )
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="dialog-overlay" onkeydown={handleKeydown} onmousedown={onDismiss}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="dialog-container"
    role="dialog"
    aria-modal="true"
    aria-labelledby="crash-dialog-title"
    onmousedown={(e) => e.stopPropagation()}
  >
    <h3 id="crash-dialog-title" class="dialog-title">
      <TriangleAlert size={16} />
      Canopy crashed
    </h3>
    <p class="dialog-message">
      The app did not shut down cleanly last time. You can report this to help us fix it.
    </p>

    <div class="crash-details">
      <div class="detail-row">
        <span class="detail-label">Time</span>
        <span>{formattedTime}</span>
      </div>
      <div class="detail-row">
        <span class="detail-label">Type</span>
        <span>{data.type}</span>
      </div>
      <div class="detail-row">
        <span class="detail-label">Version</span>
        <span>{data.appVersion}</span>
      </div>
      <div class="detail-row">
        <span class="detail-label">Electron</span>
        <span>{data.electronVersion}</span>
      </div>
      <div class="detail-row">
        <span class="detail-label">OS</span>
        <span>{data.os}</span>
      </div>
    </div>

    {#if data.errorMessage || data.stack}
      <pre class="crash-stack">{data.errorMessage}{#if data.stack}
          {data.stack}{/if}</pre>
    {/if}

    <div class="dialog-actions">
      <button bind:this={dismissBtn} class="btn btn-cancel" onclick={onDismiss}>Dismiss</button>
      <button class="btn btn-confirm" onclick={onCreateIssue}>Create issue</button>
    </div>
  </div>
</div>

<style>
  .dialog-overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 120px;
    background: var(--c-scrim);
  }

  .dialog-container {
    width: 480px;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 10px;
    box-shadow: var(--c-shadow-dialog, 0 16px 48px rgba(0, 0, 0, 0.6));
    padding: 20px;
  }

  .dialog-title {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .dialog-title :global(svg) {
    color: var(--c-warning);
    flex-shrink: 0;
  }

  .dialog-message {
    margin: 0 0 12px;
    font-size: 13px;
    color: var(--c-text-secondary);
    line-height: 1.5;
  }

  .crash-details {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 12px;
    font-size: 12px;
    color: var(--c-text-muted);
  }

  .detail-row {
    display: flex;
    gap: 8px;
  }

  .detail-label {
    min-width: 64px;
    color: var(--c-text-faint);
  }

  .crash-stack {
    margin: 0 0 12px;
    padding: 8px;
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 6px;
    font-family: monospace;
    font-size: 11px;
    line-height: 1.4;
    color: var(--c-text-muted);
    max-height: 200px;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }

  .btn {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    border: none;
    outline: none;
    transition: background 0.1s;
  }

  .btn:focus-visible {
    outline: 2px solid var(--c-focus-ring);
    outline-offset: 1px;
  }

  .btn-cancel {
    background: var(--c-active);
    color: var(--c-text);
  }

  .btn-cancel:hover {
    background: var(--c-border);
  }

  .btn-confirm {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn-confirm:hover {
    background: var(--c-accent-muted);
  }
</style>
