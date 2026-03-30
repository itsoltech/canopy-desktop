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
      onConfirm()
    }
    if (e.key === 'Tab') {
      const container = e.currentTarget as HTMLElement
      const focusable = container.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault()
        first.focus()
      }
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="dialog-overlay" onkeydown={handleKeydown} onclick={onCancel}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="dialog-container"
    role="dialog"
    aria-modal="true"
    aria-labelledby="confirm-dialog-title"
    onclick={(e) => e.stopPropagation()}
  >
    <h3 id="confirm-dialog-title" class="dialog-title">{title}</h3>
    <p class="dialog-message">{message}</p>
    {#if details}
      <p class="dialog-details">{details}</p>
    {/if}
    <div class="dialog-actions">
      <button bind:this={cancelBtn} class="btn btn-cancel" onclick={onCancel}>Cancel</button>
      <button class="btn btn-confirm" class:destructive onclick={onConfirm}>
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
    background: rgba(0, 0, 0, 0.5);
  }

  .dialog-container {
    width: 420px;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 10px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
    padding: 20px;
  }

  .dialog-title {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 600;
    color: #e0e0e0;
  }

  .dialog-message {
    margin: 0 0 4px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.7);
    line-height: 1.5;
    white-space: pre-wrap;
  }

  .dialog-details {
    margin: 0 0 4px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.35);
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
    outline: 2px solid rgba(116, 192, 252, 0.6);
    outline-offset: 1px;
  }

  .btn-cancel {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .btn-cancel:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .btn-confirm {
    background: rgba(116, 192, 252, 0.2);
    color: rgba(116, 192, 252, 0.9);
  }

  .btn-confirm:hover {
    background: rgba(116, 192, 252, 0.3);
  }

  .btn-confirm.destructive {
    background: rgba(255, 100, 100, 0.2);
    color: rgba(255, 120, 120, 0.9);
  }

  .btn-confirm.destructive:hover {
    background: rgba(255, 100, 100, 0.3);
  }
</style>
