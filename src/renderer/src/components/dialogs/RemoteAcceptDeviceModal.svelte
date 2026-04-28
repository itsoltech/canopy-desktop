<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

  let {
    deviceId,
    deviceName,
    fingerprint,
  }: {
    deviceId: string
    deviceName: string
    fingerprint: string
  } = $props()

  const AUTO_REJECT_SECONDS = 30

  let remember = $state(false)
  let secondsLeft = $state(AUTO_REJECT_SECONDS)
  let rejectBtn: HTMLButtonElement | undefined = $state()
  let busy = $state(false)
  let actionError: string | null = $state(null)
  let countdown: ReturnType<typeof setInterval> | null = null

  // Stable reference to the device id so the dev-mode console log lines up
  // with the main-process logs (`[remote] peer signal:` etc).
  const devLogId = deviceId

  onMount(() => {
    // Default focus on Reject — this is destructive territory, we prefer the
    // user to actively confirm the safer action if they press Enter blindly.
    rejectBtn?.focus()
    countdown = setInterval(() => {
      secondsLeft -= 1
      if (secondsLeft <= 0) {
        void handleReject()
      }
    }, 1000)
  })

  onDestroy(() => {
    if (countdown) clearInterval(countdown)
  })

  async function handleAccept(): Promise<void> {
    if (busy) return
    busy = true
    actionError = null
    try {
      await window.api.remote.acceptDevice(remember)
      closeDialog()
    } catch (e) {
      console.error('[remote] accept failed:', devLogId, e)
      actionError = e instanceof Error ? e.message : String(e)
      busy = false
    }
  }

  async function handleReject(): Promise<void> {
    if (busy) return
    busy = true
    try {
      await window.api.remote.rejectDevice()
    } catch {
      // Even if reject fails in the main process (e.g. the peer already left),
      // the modal must still close so the user isn't stuck.
    }
    closeDialog()
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      void handleReject()
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="dialog-overlay" onkeydown={handleKeydown} onmousedown={handleReject}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="dialog-container"
    role="dialog"
    aria-modal="true"
    aria-labelledby="remote-accept-title"
    onmousedown={(e) => e.stopPropagation()}
  >
    <h3 id="remote-accept-title" class="dialog-title">Accept remote device?</h3>
    <p class="dialog-message">
      A device on your local network is requesting remote access to this Canopy window.
    </p>

    <div class="device-block">
      <div class="device-row">
        <span class="device-label">Device</span>
        <span class="device-value">{deviceName}</span>
      </div>
      <div class="device-row">
        <span class="device-label">Fingerprint</span>
        <span class="device-value fingerprint">{fingerprint}</span>
      </div>
    </div>

    <label class="remember-row">
      <CustomCheckbox checked={remember} onchange={() => (remember = !remember)} />
      <span>Remember this device</span>
    </label>

    {#if actionError}
      <p class="action-error" role="alert">
        Failed to accept: {actionError}
      </p>
    {/if}

    <p class="countdown">Auto-reject in {secondsLeft}s</p>

    <div class="dialog-actions">
      <button
        bind:this={rejectBtn}
        type="button"
        class="btn btn-cancel"
        disabled={busy}
        onclick={handleReject}
      >
        Reject
      </button>
      <button
        type="button"
        class="btn btn-confirm destructive"
        disabled={busy}
        onclick={handleAccept}
      >
        Accept
      </button>
    </div>
  </div>
</div>

<style>
  .dialog-overlay {
    position: fixed;
    inset: 0;
    z-index: 1002;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 120px;
    background: var(--color-scrim);
  }

  .dialog-container {
    width: 420px;
    background: var(--color-bg-overlay);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    box-shadow: var(--color-shadow-dialog, 0 16px 48px oklch(0 0 0 / 0.6));
    padding: 20px;
  }

  .dialog-title {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
  }

  .dialog-message {
    margin: 0 0 16px;
    font-size: 13px;
    color: var(--color-text);
    line-height: 1.5;
  }

  .device-block {
    padding: 10px 12px;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border-subtle);
    border-radius: 6px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 12px;
  }

  .device-row {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
  }

  .device-label {
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.3px;
    font-size: 10px;
    font-weight: 600;
  }

  .device-value {
    color: var(--color-text);
  }

  .device-value.fingerprint {
    font-family: ui-monospace, 'SF Mono', Menlo, monospace;
    color: var(--color-accent-text);
  }

  .remember-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--color-text-secondary);
    padding: 8px 0;
    cursor: pointer;
  }

  .countdown {
    margin: 8px 0 0;
    font-size: 11px;
    color: var(--color-text-muted);
    text-align: right;
  }

  .action-error {
    margin: 8px 0 0;
    padding: 8px 10px;
    background: var(--color-danger-bg);
    border: 1px solid var(--color-danger);
    border-radius: 4px;
    font-size: 12px;
    color: var(--color-danger-text);
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
    outline: 2px solid var(--color-focus-ring);
    outline-offset: 1px;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .btn-cancel {
    background: var(--color-active);
    color: var(--color-text);
  }

  .btn-cancel:hover:not(:disabled) {
    background: var(--color-border);
  }

  .btn-confirm {
    background: var(--color-accent-bg);
    color: var(--color-accent-text);
  }

  .btn-confirm:hover:not(:disabled) {
    background: var(--color-accent-bg-hover);
  }

  .btn-confirm.destructive {
    background: var(--color-danger-bg);
    color: var(--color-danger-text);
  }

  .btn-confirm.destructive:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-danger) 30%, transparent);
  }
</style>
