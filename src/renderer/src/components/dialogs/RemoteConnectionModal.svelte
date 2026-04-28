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

  // Pull the reactive status out of the store. When the store transitions
  // to `peerArrived`, this modal renders inline accept/reject buttons rather
  // than handing off to a separate dialog (which used to unmount this one
  // and accidentally tear the session down through onDestroy).
  let status = $derived(remoteSession.status)
  let pendingDevice = $derived(status.kind === 'peerArrived' ? status.device : null)
  let rememberDevice = $state(false)
  let actionBusy = $state(false)
  let actionError: string | null = $state(null)
  let pairingUrl = $derived(
    status.kind === 'waiting' || status.kind === 'peerArrived' ? status.pairingUrl : null,
  )
  /**
   * Display form of the pairing URL with the token fragment masked.
   * The raw URL is still used for the QR code and the Copy button (so
   * users can re-paste it into another browser if needed), but it
   * never appears unmasked on screen — the QR is the intended
   * pairing surface and the visible URL is informational only.
   * Bystanders glancing at the screen can't memorise/photograph the
   * token this way.
   */
  let pairingUrlMasked = $derived.by(() => {
    if (!pairingUrl) return null
    // Fragment is `#t=<token>&h=<hostname>`. Mask only the `t=...`
    // segment. Keep the `h=...` piece so the user can verify the
    // hostname matches their own machine.
    return pairingUrl.replace(/([?&#]t=)[^&]+/, '$1●●●●●●●●')
  })
  /** Toggle for revealing the full URL with its token. Hidden by default. */
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
    // Check the current session state first so the modal can be opened
    // repeatedly without tripping the "already running" guard on the
    // main process. The common flow is:
    //   1. User opens modal → `start()` kicks off the session
    //   2. User hides the modal (not Stop) → session keeps running
    //   3. User reopens the modal → we land HERE with an active session
    // In that case we just adopt whatever state the store already has;
    // calling `remote.start()` would throw `AlreadyRunning`.
    try {
      const existing = await window.api.remote.getStatus()
      // `listening` means the server is already up (from auto-listen) but
      // there's no QR token yet — calling start() here upgrades it to
      // `waiting` and generates the token on the already-bound port.
      if (existing.kind === 'idle' || existing.kind === 'error' || existing.kind === 'listening') {
        await window.api.remote.start()
      }
      // Otherwise the store is already subscribed to status changes via
      // `initRemoteSessionListeners`, so the modal renders whatever is
      // live without re-starting anything.
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e)
    }
    tick = setInterval(() => (now = Date.now()), 1000)
  })

  onDestroy(() => {
    if (tick) clearInterval(tick)
    // Note: we deliberately do NOT tear the session down here. The modal can
    // be unmounted for many reasons (Svelte reactive cascade, parent
    // re-render, navigation) and tearing the session down on every unmount
    // led to a race where pairing was killed mid-flight. The user must
    // explicitly close via the Stop button below to terminate the session.
  })

  // Render the QR code whenever we have a URL and the container is mounted.
  $effect(() => {
    if (!pairingUrl || !qrEl) return
    if (qrInstance) {
      qrInstance.update({ data: pairingUrl })
      return
    }
    // Pull the dot color from the active theme so the QR is still
    // readable in light themes — hardcoded oklch(0.907 0 0) would be almost
    // invisible on a light background. The canopy-green corner
    // markers stay hardcoded because they're brand identity tied to
    // the tree-crown logo in the center of the code.
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
      // Corner markers in the same canopy-green as the tree crown in
      // the app icon, so the QR pulls visual identity from the
      // embedded logo instead of fighting it with a blue accent.
      cornersSquareOptions: { color: 'oklch(0.689 0.149 137.53)', type: 'extra-rounded' },
      cornersDotOptions: { color: 'oklch(0.689 0.149 137.53)' },
      // H-level error correction so the embedded image doesn't make the
      // QR unscannable — H tolerates ~30% of the code being obscured,
      // leaving plenty of redundancy for a centered logo.
      qrOptions: { errorCorrectionLevel: 'H' },
      imageOptions: {
        crossOrigin: 'anonymous',
        // 20% of the QR width ≈ 52px logo inside a 260px code. Anything
        // larger starts to cover too many data modules even at H-level.
        imageSize: 0.2,
        // Small transparent ring around the logo so it visually detaches
        // from the surrounding modules.
        margin: 4,
        hideBackgroundDots: true,
      },
    })
    // qrEl is a plain `bind:this` container that Svelte never writes to,
    // so appending qr-code-styling's SVG into it doesn't conflict with the
    // Svelte reconciler. Subsequent URL changes hit the `update()` fast path
    // above and never re-append.
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

  /** Explicit close button — user wants to fully terminate the session. */
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
<div class="modal-overlay" onkeydown={handleKeydown} onmousedown={closeDialog}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="modal-container"
    role="dialog"
    aria-modal="true"
    aria-labelledby="remote-connection-title"
    onmousedown={(e) => e.stopPropagation()}
  >
    <h2 id="remote-connection-title" class="modal-title">
      Remote Connection
      <span class="beta-badge" title="This feature is in beta — expect rough edges">Beta</span>
    </h2>

    {#if errorMsg}
      <div class="error-block">
        <p>Failed to start remote control:</p>
        <code>{errorMsg}</code>
      </div>
    {:else if statusKind === 'starting'}
      <div class="placeholder">
        <p>Starting signaling server…</p>
      </div>
    {:else if statusKind === 'listening'}
      <!-- Listen mode: server is already bound in the background waiting for
           trusted devices. Opening the modal calls `start()` in onMount to
           upgrade this to `waiting` and generate a fresh QR token, so this
           block is visible only in the brief window between mount and the
           upgrade (or if the upgrade fails). Show a helpful message instead
           of the bare "Loading…" placeholder. -->
      <div class="connected-block">
        <p class="connected-headline">Listening for trusted devices</p>
        {#if hostname}
          <div class="meta">
            <span class="meta-label">Host</span>
            <span class="meta-value">{hostname}</span>
          </div>
        {/if}
        <p class="subtitle">Generating a new pairing code…</p>
      </div>
    {:else if statusKind === 'paired' || statusKind === 'reconnecting'}
      <!-- Connected state: show the device name + host info instead of the
           pairing QR. The modal can still be opened after the peer has
           connected (to check connection status or terminate the session). -->
      <div class="connected-block">
        <p class="connected-headline">
          {statusKind === 'paired' ? 'Device connected' : 'Reconnecting…'}
        </p>
        {#if connectedDeviceName}
          <p class="connected-device">{connectedDeviceName}</p>
        {/if}
        {#if hostname}
          <div class="meta">
            <span class="meta-label">Host</span>
            <span class="meta-value">{hostname}</span>
          </div>
        {/if}
      </div>
    {:else if pairingUrl}
      <p class="subtitle">Scan this code from your phone, tablet or another laptop to pair.</p>

      <div class="qr-wrap">
        <div class="qr" bind:this={qrEl}></div>
      </div>

      {#if hostname}
        <div class="meta">
          <span class="meta-label">Host</span>
          <span class="meta-value">{hostname}</span>
        </div>
      {/if}

      {#if secondsLeft !== null}
        <div class="meta">
          <span class="meta-label">Expires in</span>
          <span class="meta-value">
            {Math.floor(secondsLeft / 60)}:{(secondsLeft % 60).toString().padStart(2, '0')}
          </span>
        </div>
      {/if}

      <div class="url-row">
        <code class="url">{showFullUrl ? pairingUrl : pairingUrlMasked}</code>
        <button
          type="button"
          class="btn btn-secondary"
          onclick={() => (showFullUrl = !showFullUrl)}
          title={showFullUrl
            ? 'Hide token'
            : 'Reveal token (visible to anyone looking at your screen)'}
        >
          {showFullUrl ? 'Hide' : 'Show'}
        </button>
        <button type="button" class="btn btn-secondary" onclick={copyUrl}>
          {copied ? 'Copied' : 'Copy'}
        </button>
      </div>
    {:else}
      <div class="placeholder">
        <p>Loading…</p>
      </div>
    {/if}

    <div class="status-row" data-kind={statusKind}>
      <span class="status-dot"></span>
      <span class="status-label">{statusLabel}</span>
    </div>

    {#if pendingDevice}
      <div class="accept-block">
        <p class="accept-headline">A device wants to pair</p>
        <div class="accept-meta">
          <span class="device-name">{pendingDevice.deviceName}</span>
          <code class="device-fp">{pendingDevice.fingerprint}</code>
        </div>
        <label class="remember-row">
          <input type="checkbox" bind:checked={rememberDevice} />
          <span>Remember this device</span>
        </label>
        {#if actionError}
          <p class="action-error">{actionError}</p>
        {/if}
        <div class="accept-actions">
          <button type="button" class="btn btn-cancel" disabled={actionBusy} onclick={rejectInline}>
            Reject
          </button>
          <button
            type="button"
            class="btn btn-confirm"
            disabled={actionBusy}
            onclick={acceptInline}
          >
            Accept
          </button>
        </div>
      </div>
    {/if}

    <div class="actions">
      {#if statusKind === 'idle' || statusKind === 'error'}
        <button type="button" class="btn btn-cancel" onclick={closeDialog}>Close</button>
      {:else}
        <button type="button" class="btn btn-cancel" onclick={closeDialog}>Hide</button>
        <button type="button" class="btn btn-danger" onclick={stopAndClose}>Stop session</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 80px;
    background: var(--color-scrim);
  }

  .modal-container {
    width: 420px;
    max-width: 90vw;
    background: var(--color-bg-overlay);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    box-shadow: var(--color-shadow-dialog, 0 16px 48px oklch(0 0 0 / 0.6));
    padding: 24px;
  }

  .modal-title {
    margin: 0 0 4px;
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .beta-badge {
    display: inline-block;
    padding: 2px 8px;
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--color-warning);
    background: color-mix(in srgb, var(--color-warning) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-warning) 40%, transparent);
    border-radius: 10px;
    vertical-align: middle;
  }

  .subtitle {
    margin: 0 0 16px;
    font-size: 13px;
    color: var(--color-text-secondary);
  }

  .qr-wrap {
    display: flex;
    justify-content: center;
    padding: 16px;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    margin-bottom: 16px;
  }

  .qr :global(svg) {
    display: block;
  }

  .meta {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    padding: 4px 0;
  }

  .meta-label {
    color: var(--color-text-muted);
    text-transform: uppercase;
    font-weight: 600;
    letter-spacing: 0.3px;
    font-size: 10px;
  }

  .meta-value {
    color: var(--color-text);
    font-variant-numeric: tabular-nums;
  }

  .url-row {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 12px;
    padding: 8px 10px;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border-subtle);
    border-radius: 6px;
  }

  .url {
    flex: 1;
    font-size: 11px;
    font-family: ui-monospace, 'SF Mono', Menlo, monospace;
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    margin-top: 16px;
    background: var(--color-bg-input);
    border-radius: 6px;
    font-size: 12px;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-text-muted);
  }

  .status-row[data-kind='waiting'] .status-dot,
  .status-row[data-kind='starting'] .status-dot {
    background: var(--color-warning);
    box-shadow: 0 0 8px var(--color-warning);
    animation: pulse 1.4s ease-in-out infinite;
  }

  /* Listen mode: server is bound passively, waiting for trusted-device
     reconnects. Visually quieter than `waiting` (no glow, slower pulse)
     to communicate "alive but idle" rather than "actively pairing". */
  .status-row[data-kind='listening'] .status-dot {
    background: var(--color-success);
    animation: pulse 2.4s ease-in-out infinite;
  }

  .status-row[data-kind='peerArrived'] .status-dot {
    background: var(--color-accent);
    box-shadow: 0 0 8px var(--color-accent);
  }

  .status-row[data-kind='paired'] .status-dot {
    background: var(--color-success);
    box-shadow: 0 0 8px var(--color-success);
  }

  .status-row[data-kind='reconnecting'] .status-dot {
    background: var(--color-warning);
    animation: pulse 1s ease-in-out infinite;
  }

  .status-row[data-kind='error'] .status-dot {
    background: var(--color-danger);
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.35;
    }
  }

  /* Respect `prefers-reduced-motion` — users with vestibular disorders
     should not see the continuous pulse on status dots. */
  @media (prefers-reduced-motion: reduce) {
    .status-row[data-kind='waiting'] .status-dot,
    .status-row[data-kind='starting'] .status-dot,
    .status-row[data-kind='listening'] .status-dot,
    .status-row[data-kind='reconnecting'] .status-dot {
      animation: none;
    }
  }

  .status-label {
    color: var(--color-text);
  }

  .placeholder {
    padding: 48px 16px;
    text-align: center;
    color: var(--color-text-secondary);
    font-size: 13px;
  }

  .connected-block {
    padding: 20px 18px;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    margin-bottom: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .connected-headline {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text);
  }

  .connected-device {
    margin: 0;
    font-size: 15px;
    color: var(--color-accent-text);
    font-weight: 500;
  }

  .error-block {
    padding: 16px;
    background: var(--color-danger-bg);
    border: 1px solid var(--color-danger);
    border-radius: 6px;
    margin-bottom: 16px;
  }

  .error-block p {
    margin: 0 0 4px;
    font-size: 13px;
    color: var(--color-danger-text);
  }

  .error-block code {
    font-size: 11px;
    color: var(--color-danger-text);
    font-family: ui-monospace, 'SF Mono', Menlo, monospace;
    word-break: break-all;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 20px;
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

  .btn-cancel {
    background: var(--color-active);
    color: var(--color-text);
  }

  .btn-cancel:hover {
    background: var(--color-border);
  }

  .btn-secondary {
    background: var(--color-accent-bg);
    color: var(--color-accent-text);
  }

  .btn-secondary:hover {
    background: var(--color-accent-bg-hover);
  }

  .btn-confirm {
    background: var(--color-accent-bg);
    color: var(--color-accent-text);
  }

  .btn-confirm:hover:not(:disabled) {
    background: var(--color-accent-bg-hover);
  }

  .btn-danger {
    background: var(--color-danger-bg);
    color: var(--color-danger-text);
  }

  .btn-danger:hover {
    background: color-mix(in srgb, var(--color-danger) 30%, transparent);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .accept-block {
    margin-top: 16px;
    padding: 14px 16px;
    background: var(--color-accent-bg);
    border: 1px solid var(--color-accent-muted);
    border-radius: 8px;
  }

  .accept-headline {
    margin: 0 0 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text);
  }

  .accept-meta {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
  }

  .device-name {
    font-size: 14px;
    color: var(--color-text);
  }

  .device-fp {
    font-size: 11px;
    color: var(--color-accent-text);
    font-family: ui-monospace, 'SF Mono', Menlo, monospace;
  }

  .remember-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--color-text-secondary);
    cursor: pointer;
    margin-bottom: 10px;
  }

  .action-error {
    margin: 4px 0 8px;
    font-size: 11px;
    color: var(--color-danger-text);
  }

  .accept-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
