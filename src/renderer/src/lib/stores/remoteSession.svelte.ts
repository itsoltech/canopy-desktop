// `RemoteSessionStatus` is declared globally in `src/preload/index.d.ts`
// as an ambient type (the file has no top-level import/export, so all its
// declarations land in global scope and are visible here without an import).

import { showRemoteAcceptDevice, dialogState, closeDialog } from './dialogs.svelte'
import { RemoteHostController } from '../remote/RemoteHostController'
import type { OutboundSignalFromRenderer } from '../../../../renderer-shared/remote/signalingProtocol'

/**
 * Reactive mirror of the main-process `RemoteSessionService` state.
 *
 * The desktop renderer never holds canonical state for the remote session —
 * it only observes pushes from `remote:statusChange` and dispatches actions
 * via `window.api.remote.*`. The UI (modal, status indicators, command
 * palette conditional) reads from this store so each component doesn't have
 * to subscribe to IPC events individually.
 *
 * Listeners are installed once at app mount (see `MainLayout.svelte`) and
 * torn down on unmount.
 */
export const remoteSession: { status: RemoteSessionStatus; lastError: string | null } = $state({
  status: { kind: 'idle' },
  lastError: null,
})

/**
 * The live WebRTC host controller, if any. Created lazily when the peer
 * arrives and disposed when the session returns to `idle` / `error`. Kept as
 * a module-level variable (not reactive state) because Svelte's reactivity
 * doesn't need to track it — only the lifecycle callbacks do.
 */
let hostController: RemoteHostController | null = null

function sendOutbound(msg: OutboundSignalFromRenderer): void {
  window.api.remote.sendSignal(msg).catch((e) => {
    console.warn('[remote] sendSignal failed:', e)
  })
}

function ensureHostController(): RemoteHostController {
  if (!hostController) {
    hostController = new RemoteHostController(sendOutbound)
    hostController.initialize()
  }
  return hostController
}

function teardownHostController(): void {
  if (hostController) {
    hostController.dispose()
    hostController = null
  }
}

function applyStatus(status: RemoteSessionStatus): void {
  remoteSession.status = status

  // Lifecycle: ensure the WebRTC host controller exists as soon as the
  // peer arrives so it's ready to answer an offer the moment the user
  // accepts the device. The remote peer waits on the `accepted` signal
  // before building its offer so there's ample time.
  //
  // IMPORTANT: we must NOT teardown+rebuild the controller here. The
  // statusChange and signal IPC channels are independent — if the
  // `remote:signal` (offer) event is dispatched before the renderer
  // processes the `remote:statusChange` (peerArrived) event, the offer
  // arrives at the signal handler first and triggers a controller
  // rebuild there. If we then also rebuild in the peerArrived handler,
  // we tear down the very controller that just called
  // `setRemoteDescription(offer)` and any subsequent ICE candidates
  // land on a fresh controller with no remote description — they get
  // stuck in `pendingIce` forever and the connection silently hangs.
  //
  // The signal handler (`onSignal` below) is the authoritative place
  // to guarantee a fresh `RTCPeerConnection` per new SDP offer. Here
  // we only create-if-null, which covers the first-ever pair flow.
  if ((status.kind === 'peerArrived' || status.kind === 'paired') && hostController === null) {
    ensureHostController()
  }

  // Tear down the controller on return to idle or an error state. The main
  // process already closed the peer WebSocket by this point. `listening`
  // also means the peer is gone (we reach it from the reaper / idle path
  // via returnToListening); leaving a dangling RTCPeerConnection alive
  // would leak resources and run stale ICE timers into the void. A fresh
  // pair attempt later will rebuild the controller via the
  // offer-arrived-first branch in `onSignal` below.
  if (status.kind === 'idle' || status.kind === 'error' || status.kind === 'listening') {
    teardownHostController()
  }

  // The accept UI is now rendered *inline* by `RemoteConnectionModal` while
  // the QR modal is open. We only fall back to the standalone accept dialog
  // if the connection modal is closed when a peer arrives — that lets the
  // user still respond to a peer-arrival even after dismissing the QR.
  if (
    status.kind === 'peerArrived' &&
    dialogState.current.type !== 'remoteConnection' &&
    dialogState.current.type !== 'remoteAcceptDevice'
  ) {
    showRemoteAcceptDevice({
      deviceId: status.device.deviceId,
      deviceName: status.device.deviceName,
      fingerprint: status.device.fingerprint,
    })
  }

  // Close the standalone accept dialog whenever the session is no longer
  // in the `peerArrived` state. This covers every way the accept prompt
  // can become irrelevant:
  //   - `paired`: user clicked Accept (or the peer was auto-trusted and
  //     this applyStatus is running for the first time after bypassing
  //     peerArrived — in which case there's nothing to close anyway).
  //   - `waiting`: peer aborted before the user responded (WS closed
  //     mid-prompt, refresh loop, etc.).
  //   - `reconnecting`: the old peer is in the reaper window — the
  //     prompt was for an outdated pair attempt.
  //   - `idle` / `error`: session torn down entirely.
  // Without this, the prompt would linger with a stale device (or a
  // countdown that auto-rejects a connection that's already gone) and
  // the user would see the modal repeatedly re-opening from each new
  // peerArrived event.
  if (status.kind !== 'peerArrived' && dialogState.current.type === 'remoteAcceptDevice') {
    closeDialog()
  }
}

export function initRemoteSessionListeners(): () => void {
  // Seed the store with the current main-process state so first render isn't
  // stuck on 'idle' if the user opened a window while a session was active.
  window.api.remote
    .getStatus()
    .then((status) => applyStatus(status))
    .catch(() => {
      // Ignore — status change listener will update on the next event.
    })

  const unsubStatus = window.api.remote.onStatusChange((status) => applyStatus(status))

  // Best-effort: ask the main process to bring the signaling server up in
  // passive listen mode so a previously trusted phone can reconnect without
  // the user re-opening the Remote Connection modal after a desktop
  // restart. The main process silently no-ops unless the user has opted in
  // and has ≥1 trusted device; on success we'll receive a `statusChange`
  // event transitioning to `listening` through the listener above.
  window.api.remote.ensureListening().catch(() => {
    // Never surfaces errors — listen mode is opportunistic.
  })
  const unsubSignal = window.api.remote.onSignal((msg) => {
    // Signals from the main process only reach this renderer if it's the
    // designated host window (see `RemoteSessionService.handlePeerSignal`), so
    // we can safely hand them to the controller without any cross-window
    // dispatch.
    //
    // A fresh `offer` message means the peer just created a brand-new
    // `RTCPeerConnection` (which happens on every page load / reconnect).
    // IPC events from the main process arrive on different channels with
    // no cross-channel ordering guarantee — we may receive the `offer`
    // BEFORE our `applyStatus` handler has processed the `peerArrived`
    // state change that was supposed to tear down the old controller and
    // create a fresh one. So we rebuild the controller eagerly right here
    // whenever an offer arrives, guaranteeing a pristine pc for every
    // new SDP exchange regardless of IPC delivery order.
    const isOffer =
      typeof msg === 'object' && msg !== null && (msg as { type?: unknown }).type === 'offer'
    if (isOffer) {
      teardownHostController()
      ensureHostController()
    }
    const ctl = ensureHostController()
    void ctl.handleSignal(msg)
  })

  return () => {
    unsubStatus()
    unsubSignal()
    teardownHostController()
  }
}
