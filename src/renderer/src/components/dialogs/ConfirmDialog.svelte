<script lang="ts">
  import { onMount } from 'svelte'

  let {
    title,
    message,
    details,
    confirmLabel = 'Confirm',
    destructive = false,
    onConfirm,
    onCancel,
  }: {
    title: string
    message: string
    details?: string
    confirmLabel?: string
    destructive?: boolean
    onConfirm: () => void
    onCancel: () => void
  } = $props()

  let cancelBtn: HTMLButtonElement | undefined = $state()
  let confirmBtn: HTMLButtonElement | undefined = $state()

  onMount(() => {
    cancelBtn?.focus()
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onCancel()
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      e.stopPropagation()
      // When destructive, only confirm if the confirm button is explicitly focused
      if (destructive && document.activeElement !== confirmBtn) {
        if (document.activeElement === cancelBtn) onCancel()
        return
      }
      onConfirm()
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="dialog-overlay" onkeydown={handleKeydown} onmousedown={onCancel}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="dialog-container"
    role="dialog"
    aria-modal="true"
    aria-labelledby="confirm-dialog-title"
    onmousedown={(e) => e.stopPropagation()}
  >
    <h3 id="confirm-dialog-title" class="dialog-title">{title}</h3>
    <p class="dialog-message">{message}</p>
    {#if details}
      <p class="dialog-details">{details}</p>
    {/if}
    <div class="dialog-actions">
      <button bind:this={cancelBtn} class="btn btn-cancel" onclick={onCancel}>Cancel</button>
      <button bind:this={confirmBtn} class="btn btn-confirm" class:destructive onclick={onConfirm}>
        {confirmLabel}
      </button>
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
    width: 420px;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 10px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
    padding: 20px;
  }

  .dialog-title {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
  }

  .dialog-message {
    margin: 0 0 4px;
    font-size: 13px;
    color: var(--c-text);
    line-height: 1.5;
    white-space: pre-wrap;
  }

  .dialog-details {
    margin: 0 0 4px;
    font-size: 11px;
    color: var(--c-text-muted);
    font-family: monospace;
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

  .btn-confirm.destructive {
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
  }

  .btn-confirm.destructive:hover {
    background: color-mix(in srgb, var(--c-danger) 30%, transparent);
  }
</style>
