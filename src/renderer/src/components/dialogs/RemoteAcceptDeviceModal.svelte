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

  const devLogId = deviceId

  onMount(() => {
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
      // Even if reject fails, modal must close so the user isn't stuck.
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
<div
  class="fixed inset-0 z-[1002] flex justify-center items-start pt-[120px] bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={handleReject}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[420px] bg-bg-overlay border border-border rounded-[10px] shadow-modal p-5"
    role="dialog"
    aria-modal="true"
    aria-labelledby="remote-accept-title"
    onmousedown={(e) => e.stopPropagation()}
  >
    <h3 id="remote-accept-title" class="m-0 mb-2 text-[15px] font-semibold text-text">
      Accept remote device?
    </h3>
    <p class="m-0 mb-4 text-md text-text leading-normal">
      A device on your local network is requesting remote access to this Canopy window.
    </p>

    <div
      class="px-3 py-2.5 bg-bg-input border border-border-subtle rounded-lg flex flex-col gap-1.5 mb-3"
    >
      <div class="flex justify-between text-sm">
        <span class="text-text-muted uppercase tracking-[0.3px] text-2xs font-semibold">Device</span
        >
        <span class="text-text">{deviceName}</span>
      </div>
      <div class="flex justify-between text-sm">
        <span class="text-text-muted uppercase tracking-[0.3px] text-2xs font-semibold"
          >Fingerprint</span
        >
        <span class="font-mono text-accent-text">{fingerprint}</span>
      </div>
    </div>

    <label class="flex items-center gap-2 text-sm text-text-secondary py-2 cursor-pointer">
      <CustomCheckbox checked={remember} onchange={() => (remember = !remember)} />
      <span>Remember this device</span>
    </label>

    {#if actionError}
      <p
        class="m-0 mt-2 px-2.5 py-2 bg-danger-bg border border-danger rounded-md text-sm text-danger-text"
        role="alert"
      >
        Failed to accept: {actionError}
      </p>
    {/if}

    <p class="m-0 mt-2 text-xs text-text-muted text-right">Auto-reject in {secondsLeft}s</p>

    <div class="flex justify-end gap-2 mt-4">
      <button
        bind:this={rejectBtn}
        type="button"
        class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-active text-text transition-colors duration-fast enabled:hover:bg-border focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1 disabled:opacity-50 disabled:cursor-wait"
        disabled={busy}
        onclick={handleReject}
      >
        Reject
      </button>
      <button
        type="button"
        class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-danger-bg text-danger-text transition-colors duration-fast enabled:hover:bg-[color-mix(in_srgb,var(--color-danger)_30%,transparent)] focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1 disabled:opacity-50 disabled:cursor-wait"
        disabled={busy}
        onclick={handleAccept}
      >
        Accept
      </button>
    </div>
  </div>
</div>
