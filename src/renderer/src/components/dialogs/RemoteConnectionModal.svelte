<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { match } from 'ts-pattern'
  import QRCodeStyling from 'qr-code-styling'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { remoteSession } from '../../lib/stores/remoteSession.svelte'
  import appIconUrl from '../../assets/app-icon.png'

  let qrEl: HTMLDivElement | undefined = $state()
  let qrInstance: QRCodeStyling | null = null
  let errorMsg: string | null = $state(null)
  let copied = $state(false)
  let now = $state(Date.now())
  let tick: ReturnType<typeof setInterval> | null = null

  let status = $derived(remoteSession.status)
  let pendingDevice = $derived(status.kind === 'peerArrived' ? status.device : null)
  let rememberDevice = $state(false)
  let actionBusy = $state(false)
  let actionError: string | null = $state(null)
  let pairingUrl = $derived(
    status.kind === 'waiting' || status.kind === 'peerArrived' ? status.pairingUrl : null,
  )
  let pairingUrlMasked = $derived.by(() => {
    if (!pairingUrl) return null
    return pairingUrl.replace(/([?&#]t=)[^&]+/, '$1●●●●●●●●')
  })
  let showFullUrl = $state(false)
  let hostname = $derived(
    status.kind === 'listening' ||
      status.kind === 'waiting' ||
      status.kind === 'peerArrived' ||
      status.kind === 'paired' ||
      status.kind === 'reconnecting'
      ? status.hostname
      : null,
  )
  let expiresAt = $derived(status.kind === 'waiting' ? status.expiresAt : null)
  let connectedDeviceName = $derived(
    status.kind === 'paired' || status.kind === 'reconnecting' ? status.deviceName : null,
  )

  let secondsLeft = $derived.by(() => {
    if (expiresAt === null) return null
    const ms = expiresAt - now
    return ms > 0 ? Math.ceil(ms / 1000) : 0
  })

  let statusLabel = $derived.by(() =>
    match(status)
      .with({ kind: 'idle' }, () => 'Ready')
      .with({ kind: 'starting' }, () => 'Starting signaling server…')
      .with({ kind: 'listening' }, () => 'Listening for trusted devices')
      .with({ kind: 'waiting' }, () => 'Waiting for device to scan')
      .with({ kind: 'peerArrived' }, (s) => `Device requesting pairing: ${s.device.deviceName}`)
      .with({ kind: 'paired' }, (s) => `Connected to ${s.deviceName}`)
      .with({ kind: 'reconnecting' }, () => 'Reconnecting…')
      .with({ kind: 'error' }, (s) => s.message)
      .exhaustive(),
  )

  let statusKind = $derived(status.kind)

  onMount(async () => {
    try {
      const existing = await window.api.remote.getStatus()
      if (existing.kind === 'idle' || existing.kind === 'error' || existing.kind === 'listening') {
        await window.api.remote.start()
      }
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e)
    }
    tick = setInterval(() => (now = Date.now()), 1000)
  })

  onDestroy(() => {
    if (tick) clearInterval(tick)
  })

  $effect(() => {
    if (!pairingUrl || !qrEl) return
    if (qrInstance) {
      qrInstance.update({ data: pairingUrl })
      return
    }
    const style = getComputedStyle(document.documentElement)
    const dotColor = style.getPropertyValue('--c-text').trim() || 'oklch(0.907 0 0)'
    qrInstance = new QRCodeStyling({
      width: 260,
      height: 260,
      type: 'svg',
      data: pairingUrl,
      image: appIconUrl,
      dotsOptions: { color: dotColor, type: 'rounded' },
      backgroundOptions: { color: 'transparent' },
      cornersSquareOptions: { color: 'oklch(0.689 0.149 137.53)', type: 'extra-rounded' },
      cornersDotOptions: { color: 'oklch(0.689 0.149 137.53)' },
      qrOptions: { errorCorrectionLevel: 'H' },
      imageOptions: {
        crossOrigin: 'anonymous',
        imageSize: 0.2,
        margin: 4,
        hideBackgroundDots: true,
      },
    })
    qrInstance.append(qrEl)
  })

  async function copyUrl(): Promise<void> {
    if (!pairingUrl) return
    try {
      await navigator.clipboard.writeText(pairingUrl)
      copied = true
      setTimeout(() => (copied = false), 1500)
    } catch {
      copied = false
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      closeDialog()
    }
  }

  async function acceptInline(): Promise<void> {
    if (actionBusy) return
    actionBusy = true
    actionError = null
    try {
      await window.api.remote.acceptDevice(rememberDevice)
    } catch (e) {
      actionError = e instanceof Error ? e.message : String(e)
    } finally {
      actionBusy = false
    }
  }

  async function rejectInline(): Promise<void> {
    if (actionBusy) return
    actionBusy = true
    actionError = null
    try {
      await window.api.remote.rejectDevice()
    } catch (e) {
      actionError = e instanceof Error ? e.message : String(e)
    } finally {
      actionBusy = false
    }
  }

  async function stopAndClose(): Promise<void> {
    try {
      await window.api.remote.stop()
    } catch {
      /* ignore */
    }
    closeDialog()
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-[1001] flex justify-center items-start pt-20 bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={closeDialog}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[420px] max-w-[90vw] bg-bg-overlay border border-border rounded-[10px] shadow-modal p-6"
    role="dialog"
    aria-modal="true"
    aria-labelledby="remote-connection-title"
    onmousedown={(e) => e.stopPropagation()}
  >
    <h2
      id="remote-connection-title"
      class="m-0 mb-1 text-xl font-semibold text-text flex items-center gap-2"
    >
      Remote Connection
      <span
        class="inline-block px-2 py-0.5 text-[9px] font-bold uppercase tracking-[0.6px] text-warning bg-experimental-bg border border-experimental-border rounded-[10px] align-middle"
        title="This feature is in beta — expect rough edges">Beta</span
      >
    </h2>

    {#if errorMsg}
      <div class="p-4 bg-danger-bg border border-danger rounded-md mb-4">
        <p class="m-0 mb-1 text-md text-danger-text">Failed to start remote control:</p>
        <code class="text-xs text-danger-text font-mono break-all">{errorMsg}</code>
      </div>
    {:else if statusKind === 'starting'}
      <div class="px-4 py-12 text-center text-text-secondary text-md">
        <p>Starting signaling server…</p>
      </div>
    {:else if statusKind === 'listening'}
      <div
        class="px-4 py-5 bg-bg-input border border-border-subtle rounded-lg mb-3 flex flex-col gap-2"
      >
        <p class="m-0 text-md font-semibold text-text">Listening for trusted devices</p>
        {#if hostname}
          <div class="flex justify-between text-sm py-1">
            <span class="text-text-muted uppercase font-semibold tracking-[0.3px] text-2xs"
              >Host</span
            >
            <span class="text-text tabular-nums">{hostname}</span>
          </div>
        {/if}
        <p class="m-0 mb-4 text-md text-text-secondary">Generating a new pairing code…</p>
      </div>
    {:else if statusKind === 'paired' || statusKind === 'reconnecting'}
      <div
        class="px-4 py-5 bg-bg-input border border-border-subtle rounded-lg mb-3 flex flex-col gap-2"
      >
        <p class="m-0 text-md font-semibold text-text">
          {statusKind === 'paired' ? 'Device connected' : 'Reconnecting…'}
        </p>
        {#if connectedDeviceName}
          <p class="m-0 text-lg text-accent-text font-medium">{connectedDeviceName}</p>
        {/if}
        {#if hostname}
          <div class="flex justify-between text-sm py-1">
            <span class="text-text-muted uppercase font-semibold tracking-[0.3px] text-2xs"
              >Host</span
            >
            <span class="text-text tabular-nums">{hostname}</span>
          </div>
        {/if}
      </div>
    {:else if pairingUrl}
      <p class="m-0 mb-4 text-md text-text-secondary">
        Scan this code from your phone, tablet or another laptop to pair.
      </p>

      <div class="flex justify-center p-4 bg-bg-input border border-border-subtle rounded-xl mb-4">
        <div class="qr [&_svg]:block" bind:this={qrEl}></div>
      </div>

      {#if hostname}
        <div class="flex justify-between text-sm py-1">
          <span class="text-text-muted uppercase font-semibold tracking-[0.3px] text-2xs">Host</span
          >
          <span class="text-text tabular-nums">{hostname}</span>
        </div>
      {/if}

      {#if secondsLeft !== null}
        <div class="flex justify-between text-sm py-1">
          <span class="text-text-muted uppercase font-semibold tracking-[0.3px] text-2xs"
            >Expires in</span
          >
          <span class="text-text tabular-nums">
            {Math.floor(secondsLeft / 60)}:{(secondsLeft % 60).toString().padStart(2, '0')}
          </span>
        </div>
      {/if}

      <div
        class="flex gap-2 items-center mt-3 px-2.5 py-2 bg-bg-input border border-border-subtle rounded-lg"
      >
        <code
          class="flex-1 text-xs font-mono text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap"
          >{showFullUrl ? pairingUrl : pairingUrlMasked}</code
        >
        <button
          type="button"
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-accent-bg text-accent-text hover:bg-accent-bg-hover focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
          onclick={() => (showFullUrl = !showFullUrl)}
          title={showFullUrl
            ? 'Hide token'
            : 'Reveal token (visible to anyone looking at your screen)'}
        >
          {showFullUrl ? 'Hide' : 'Show'}
        </button>
        <button
          type="button"
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-accent-bg text-accent-text hover:bg-accent-bg-hover focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
          onclick={copyUrl}
        >
          {copied ? 'Copied' : 'Copy'}
        </button>
      </div>
    {:else}
      <div class="px-4 py-12 text-center text-text-secondary text-md">
        <p>Loading…</p>
      </div>
    {/if}

    <div
      class="flex items-center gap-2 px-3 py-2.5 mt-4 bg-bg-input rounded-lg text-sm"
      data-kind={statusKind}
    >
      <span class="status-dot w-2 h-2 rounded-full bg-text-muted flex-shrink-0"></span>
      <span class="text-text">{statusLabel}</span>
    </div>

    {#if pendingDevice}
      <div class="mt-4 px-4 py-3.5 bg-accent-bg border border-accent-muted rounded-xl">
        <p class="m-0 mb-2 text-md font-semibold text-text">A device wants to pair</p>
        <div class="flex flex-col gap-1 mb-2.5">
          <span class="text-lg text-text">{pendingDevice.deviceName}</span>
          <code class="text-xs text-accent-text font-mono">{pendingDevice.fingerprint}</code>
        </div>
        <label class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer mb-2.5">
          <input type="checkbox" bind:checked={rememberDevice} />
          <span>Remember this device</span>
        </label>
        {#if actionError}
          <p class="my-1 mb-2 text-xs text-danger-text">{actionError}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-active text-text enabled:hover:bg-border disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
            disabled={actionBusy}
            onclick={rejectInline}
          >
            Reject
          </button>
          <button
            type="button"
            class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
            disabled={actionBusy}
            onclick={acceptInline}
          >
            Accept
          </button>
        </div>
      </div>
    {/if}

    <div class="flex justify-end gap-2 mt-5">
      {#if statusKind === 'idle' || statusKind === 'error'}
        <button
          type="button"
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-active text-text hover:bg-border focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
          onclick={closeDialog}>Close</button
        >
      {:else}
        <button
          type="button"
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-active text-text hover:bg-border focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
          onclick={closeDialog}>Hide</button
        >
        <button
          type="button"
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-danger-bg text-danger-text hover:bg-[color-mix(in_srgb,var(--color-danger)_30%,transparent)] focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
          onclick={stopAndClose}>Stop session</button
        >
      {/if}
    </div>
  </div>
</div>

<!-- Status dot animations: data-kind drives the animation/glow combinations.
     prefers-reduced-motion disables the pulse for accessibility. Keyframes
     and per-state mappings are easier to express here than as utility classes. -->