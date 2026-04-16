import { app, BrowserWindow, webContents as webContentsNs } from 'electron'
import { hostname as osHostname } from 'node:os'
import path from 'node:path'
import { randomBytes } from 'node:crypto'
import { ResultAsync, errAsync, okAsync } from 'neverthrow'
import type { PreferencesStore } from '../db/PreferencesStore'
import {
  SignalingServer,
  tokensMatch,
  type PairResponse,
  type PairMessage,
} from './SignalingServer'
import { selectPrimaryInterface } from './discovery'
import type { RemoteServerError } from './errors'
import type { PendingDevice, RemoteSessionStatus, PairingUrlInfo } from './types'
import { TrustedDeviceStore } from './TrustedDeviceStore'

/** How long the QR code (and its one-shot token) remains valid before auto-stop. */
const PAIRING_TTL_MS = 10 * 60 * 1000 // 10 minutes

/** Length of the random hex token embedded in the QR fragment (`#t=...`). */
const TOKEN_BYTES = 32

/** How long the desktop keeps the session alive after the peer disconnects. */
const REAPER_WINDOW_MS = 30 * 1000 // 30 seconds

/** Auto-close idle sessions after this many ms of no RPC activity. */
const IDLE_TIMEOUT_MS = 15 * 60 * 1000 // 15 minutes

const REMOTE_STATUS_CHANNEL = 'remote:statusChange'
const REMOTE_SIGNAL_CHANNEL = 'remote:signal'

/**
 * Preferences key for persisting the last successfully-bound signaling port.
 * Reusing the same port across sessions keeps the peer-client origin
 * (`http://<lanip>:<port>`) stable, which is what lets the peer's
 * localStorage (device ID, "remember this device" flag) survive a Canopy
 * restart or a session reset — localStorage is scoped per origin, and a
 * new port means a new origin means a fresh, empty storage.
 */
const LAST_PORT_PREF_KEY = 'remote.lastPort'

/**
 * State machine + lifecycle owner for the WebRTC remote-control feature.
 *
 * Responsibilities:
 *   - Generate one-shot pairing tokens.
 *   - Spin the {@link SignalingServer} up/down on demand (lazy start, never
 *     listens until the user actually opens the modal).
 *   - Resolve a usable LAN IP via {@link selectPrimaryInterface} so the QR URL
 *     points at a host the phone can reach.
 *   - Validate pairing tokens with constant-time comparison.
 *   - Hold and surface the current {@link RemoteSessionStatus} to the renderer
 *     by broadcasting `remote:statusChange` to every BrowserWindow.
 *   - Schedule pairing TTL expiry.
 *
 * Phase 2 scope: pairing handshake + status broadcast. The full peer flow
 * (accept modal, signaling routing into renderer, idle/reaper timers, trusted
 * devices) lands in Phases 3, 4, 10, 11.
 */
export class RemoteSessionService {
  private status: RemoteSessionStatus = { kind: 'idle' }
  private signalingServer = new SignalingServer()
  private pendingToken: string | null = null
  private currentPairing: PairingUrlInfo | null = null
  private pendingDevice: PendingDevice | null = null
  private pairingExpiryTimer: ReturnType<typeof setTimeout> | null = null
  private reaperTimer: ReturnType<typeof setTimeout> | null = null
  private idleTimer: ReturnType<typeof setTimeout> | null = null
  private lastPairedDeviceName: string | null = null
  /**
   * Device ID of the most recently accepted peer in this session. Used to
   * recognize a refresh/reconnect from the same device — when a peer
   * reloads its page the host goes through `paired → reconnecting`, and
   * the peer then reconnects with the same persistent deviceId (loaded
   * from its localStorage). Matching that ID lets us auto-accept the
   * reconnect inside the reaper window without showing the accept modal.
   */
  private lastPairedDeviceId: string | null = null
  /**
   * `webContents.id` of the renderer that started this session. WebRTC lives
   * entirely in one renderer; peer signaling messages are forwarded only to
   * that window, not broadcast to all. Reset on `stop()` / `dispose()`.
   */
  private hostWcId: number | null = null

  private trustedDevices: TrustedDeviceStore

  constructor(private preferencesStore: PreferencesStore) {
    this.trustedDevices = new TrustedDeviceStore(preferencesStore)
  }

  /** Read-only accessor used by IPC handlers to verify that outbound signals
   * come from the window that owns the session. */
  get currentHostWcId(): number | null {
    return this.hostWcId
  }

  getStatus(): RemoteSessionStatus {
    return this.status
  }

  isEnabledInPreferences(): boolean {
    return this.preferencesStore.get('remote.enabled') === 'true'
  }

  /** Trusted devices listing — used by the settings UI. */
  listTrustedDevices(): ReturnType<TrustedDeviceStore['list']> {
    return this.trustedDevices.list()
  }

  /** Remove a single trusted device by its ID. */
  removeTrustedDevice(deviceId: string): void {
    this.trustedDevices.remove(deviceId)
  }

  /**
   * Start a new pairing session: pick a LAN IP, generate a token, bind the
   * signaling server, build the URL the QR code will encode.
   *
   * `hostWcId` identifies the renderer that initiated the session; peer
   * signaling messages are forwarded only to that window so the WebRTC
   * controller has a single, well-defined owner.
   *
   * Idempotent-on-error: if any step fails, we tear back down to `idle` so the
   * user can retry without restarting the app.
   */
  start(hostWcId: number): ResultAsync<{ pairingUrl: string }, RemoteServerError> {
    // Fast path: signaling server is already bound from an auto-listen
    // pass (see `ensureListening`). Upgrade to `waiting` by generating a
    // fresh one-shot token + QR URL on the existing listener — no rebind,
    // no bundle reload. The caller becomes the host window so peer signals
    // reach the renderer that opened the QR modal.
    if (this.status.kind === 'listening') {
      this.hostWcId = hostWcId
      const { lanIp, port } = this.status
      return okAsync(this.beginPairing(lanIp, port))
    }

    if (this.status.kind !== 'idle' && this.status.kind !== 'error') {
      return errAsync({ _tag: 'AlreadyRunning' })
    }

    const iface = selectPrimaryInterface()
    if (!iface) {
      const err: RemoteServerError = { _tag: 'NoNetworkInterface' }
      this.setStatus({ kind: 'error', message: 'No usable network interface found' })
      return errAsync(err)
    }

    this.hostWcId = hostWcId
    const bundleRoot = this.resolveBundleRoot()
    this.setStatus({ kind: 'starting' })

    // Try the last-used port first so the peer-client origin stays
    // stable across Canopy restarts and session resets. Peers rely on
    // localStorage (device ID, trusted flag), and localStorage is per
    // origin — a new port means a new origin means fresh empty storage.
    const savedPortRaw = this.preferencesStore.get(LAST_PORT_PREF_KEY)
    const savedPort = savedPortRaw && /^\d+$/.test(savedPortRaw) ? parseInt(savedPortRaw, 10) : 0

    return this.signalingServer
      .start({
        bundleRoot,
        preferredPort: savedPort,
        handlers: {
          onPairAttempt: (msg) => this.handlePairAttempt(msg),
          onPeerSignal: (msg) => this.handlePeerSignal(msg),
          onPeerDisconnected: () => this.handlePeerDisconnected(),
        },
      })
      .map(({ port }) => {
        // Persist the actually-bound port for next time. If the preferred
        // port was taken and the server fell back to an ephemeral one, we
        // update the preference to the new port — that one's now free (we
        // just bound to it) so it's the new "preferred" for next start.
        if (String(port) !== savedPortRaw) {
          this.preferencesStore.set(LAST_PORT_PREF_KEY, String(port))
        }
        return this.beginPairing(iface.address, port)
      })
      .mapErr((e) => {
        this.setStatus({ kind: 'error', message: `Failed to start: ${e._tag}` })
        this.cleanupSession()
        return e
      })
  }

  /**
   * Start the signaling server in passive listen mode if the user has a
   * previously trusted device. This is how a paired phone reconnects after
   * the desktop app is closed and reopened without the user needing to open
   * the Remote Connection modal: the server is listening on the same saved
   * port, and `handlePairAttempt` recognizes the trusted `deviceId` and
   * auto-accepts without a one-shot token.
   *
   * Called from the renderer root layout on app mount. Best-effort: any
   * failure leaves the session idle and the user can still start a QR
   * pairing manually. Conditions:
   *   - `remote.enabled === 'true'` (user opted in)
   *   - TrustedDeviceStore has ≥1 entry
   *   - A usable LAN interface is available
   * Missing any of these is a silent no-op.
   *
   * Idempotent: if the server is already running (either from a prior
   * listen-mode start or a manual `start()`), the method only rebinds the
   * host renderer if the previous one is gone (dev reload, window closed).
   */
  ensureListening(hostWcId: number): ResultAsync<void, RemoteServerError> {
    // Already running — keep the server up, just adopt the caller as the
    // host renderer if the previous one is destroyed. We do NOT blindly
    // reassign: a mid-session window claiming listening should not steal
    // signaling from an active host.
    if (this.signalingServer.isRunning) {
      const prev = this.hostWcId
      let prevLive = false
      if (prev !== null) {
        const wc = webContentsNs.fromId(prev)
        prevLive = !!wc && !wc.isDestroyed()
      }
      if (!prevLive) {
        this.hostWcId = hostWcId
      }
      return okAsync(undefined)
    }

    if (!this.isEnabledInPreferences()) return okAsync(undefined)
    if (this.trustedDevices.list().length === 0) return okAsync(undefined)

    const iface = selectPrimaryInterface()
    if (!iface) return okAsync(undefined)

    this.hostWcId = hostWcId
    const bundleRoot = this.resolveBundleRoot()
    const savedPortRaw = this.preferencesStore.get(LAST_PORT_PREF_KEY)
    const savedPort = savedPortRaw && /^\d+$/.test(savedPortRaw) ? parseInt(savedPortRaw, 10) : 0

    return this.signalingServer
      .start({
        bundleRoot,
        preferredPort: savedPort,
        handlers: {
          onPairAttempt: (msg) => this.handlePairAttempt(msg),
          onPeerSignal: (msg) => this.handlePeerSignal(msg),
          onPeerDisconnected: () => this.handlePeerDisconnected(),
        },
      })
      .map(({ port }) => {
        if (String(port) !== savedPortRaw) {
          this.preferencesStore.set(LAST_PORT_PREF_KEY, String(port))
        }
        const hostname = osHostname()
        // Populate `currentPairing` so the trusted auto-accept branch in
        // `handlePairAttempt` can pluck hostname/lanIp/port from it when
        // the trusted peer arrives. `pairingUrl` / `expiresAt` are
        // sentinels — nothing surfaces them to the UI while we're in
        // `listening`, and the next `start()` call will overwrite them
        // with real values via `beginPairing`.
        this.currentPairing = {
          pairingUrl: '',
          hostname,
          lanIp: iface.address,
          port,
          expiresAt: Number.POSITIVE_INFINITY,
        }
        // Deliberately NO pendingToken: untrusted peers cannot pair from
        // listening mode, only trusted devices whose deviceId matches an
        // entry already in the TrustedDeviceStore.
        this.setStatus({ kind: 'listening', hostname, lanIp: iface.address, port })
      })
      .mapErr((e) => {
        // Best-effort: listen mode failing should not block the app. Log
        // and return to idle so the user can still start a session
        // manually from the Remote Connection modal.
        console.warn('[remote] ensureListening failed:', e._tag)
        this.hostWcId = null
        return e
      })
  }

  /**
   * Generate a fresh one-shot pairing token + QR URL and transition to
   * `waiting`. Assumes the signaling server is already listening. Shared by
   * the `start()` cold-start path (after `SignalingServer.start` succeeds)
   * and the `start()` fast path that upgrades an already-listening server
   * out of `listening` into `waiting`.
   */
  private beginPairing(lanIp: string, port: number): { pairingUrl: string } {
    const token = randomBytes(TOKEN_BYTES).toString('hex')
    this.pendingToken = token
    const expiresAt = Date.now() + PAIRING_TTL_MS
    const hostname = osHostname()
    const pairingUrl = buildPairingUrl({
      host: lanIp,
      port,
      token,
      hostname,
    })
    const info: PairingUrlInfo = {
      pairingUrl,
      hostname,
      lanIp,
      port,
      expiresAt,
    }
    this.currentPairing = info
    this.scheduleExpiry(PAIRING_TTL_MS)
    this.setStatus({
      kind: 'waiting',
      pairingUrl,
      hostname,
      lanIp,
      port,
      expiresAt,
    })
    return { pairingUrl }
  }

  /**
   * Tear down everything: close peer, stop server, clear timers, return to
   * `idle`. Safe to call from any state.
   */
  stop(): ResultAsync<void, RemoteServerError> {
    if (!this.signalingServer.isRunning) {
      this.cleanupSession()
      this.hostWcId = null
      this.setStatus({ kind: 'idle' })
      return okAsync(undefined)
    }
    return this.signalingServer.stop().map(() => {
      this.cleanupSession()
      this.hostWcId = null
      this.setStatus({ kind: 'idle' })
    })
  }

  /**
   * Called by the desktop renderer after the user accepts the pending device
   * in the modal. Phase 4 will use this to greenlight the WebRTC offer/answer
   * exchange. For Phase 2 we just record the device and transition state.
   */
  acceptPendingDevice(remember: boolean): ResultAsync<void, RemoteServerError> {
    if (this.status.kind !== 'peerArrived' || !this.pendingDevice || !this.currentPairing) {
      return errAsync({ _tag: 'NoPendingPeer' })
    }
    const device = this.pendingDevice
    const pairing = this.currentPairing
    this.pendingDevice = null
    this.lastPairedDeviceName = device.deviceName
    this.lastPairedDeviceId = device.deviceId
    // Clear the pairing expiry — once paired, only idle timeout applies.
    this.clearExpiryTimer()
    this.clearReaperTimer()
    this.resetIdleTimer()
    if (remember) {
      this.trustedDevices.add({
        deviceId: device.deviceId,
        publicKeyJwk: device.publicKeyJwk ?? null,
        name: device.deviceName,
        addedAt: new Date().toISOString(),
        lastSeen: new Date().toISOString(),
      })
      console.log('[remote] device persisted as trusted:', device.deviceId)
    }
    this.setStatus({
      kind: 'paired',
      hostname: pairing.hostname,
      lanIp: pairing.lanIp,
      port: pairing.port,
      deviceName: device.deviceName,
      connectedAt: Date.now(),
    })
    // Green-light the WebRTC offer/answer exchange on the peer side. Before
    // this, the remote peer is waiting on `accepted` before creating data
    // channels and building its SDP offer.
    this.signalingServer.sendToPeer({ type: 'accepted' })
    return okAsync(undefined)
  }

  /**
   * Called by the desktop renderer when the user rejects the pending device
   * in the modal. Closes the WebSocket and returns to the waiting state so the
   * user can scan from a different phone or close the modal entirely.
   */
  rejectPendingDevice(): ResultAsync<void, RemoteServerError> {
    if (this.status.kind !== 'peerArrived' || !this.currentPairing) {
      return errAsync({ _tag: 'NoPendingPeer' })
    }
    this.pendingDevice = null
    // Tell the peer before closing the socket so the peer UI can render a
    // useful error instead of a generic "connection dropped" message.
    this.signalingServer.sendToPeer({ type: 'rejected', reason: 'user rejected' })
    this.signalingServer.closePeer('rejected by user')
    const pairing = this.currentPairing
    this.setStatus({
      kind: 'waiting',
      pairingUrl: pairing.pairingUrl,
      hostname: pairing.hostname,
      lanIp: pairing.lanIp,
      port: pairing.port,
      expiresAt: pairing.expiresAt,
    })
    return okAsync(undefined)
  }

  /**
   * Forward a signaling message produced by the desktop renderer (offer /
   * answer / ice candidates) to the active peer. The caller (IPC handler)
   * should have already verified that the originating renderer is the owning
   * `hostWcId`; this method just guards on session state.
   */
  forwardSignalToPeer(msg: Record<string, unknown>): ResultAsync<void, RemoteServerError> {
    if (this.status.kind !== 'peerArrived' && this.status.kind !== 'paired') {
      return errAsync({ _tag: 'NoPendingPeer' })
    }
    this.resetIdleTimer()
    this.signalingServer.sendToPeer(msg)
    return okAsync(undefined)
  }

  dispose(): void {
    this.clearExpiryTimer()
    if (this.signalingServer.isRunning) {
      // Best-effort fire-and-forget on shutdown
      this.signalingServer.stop().match(
        () => {},
        () => {},
      )
    }
    this.cleanupSession()
    this.hostWcId = null
    this.status = { kind: 'idle' }
  }

  // ===== SignalingServer callbacks =====

  private handlePairAttempt(msg: PairMessage): PairResponse {
    if (!this.isEnabledInPreferences()) {
      return { ok: false, reason: 'remote control disabled' }
    }
    const ts = new Date().toISOString().slice(11, 23)
    const incomingDeviceId =
      typeof msg.deviceId === 'string' && msg.deviceId.length > 0 ? msg.deviceId : null
    const isTrustedDevice =
      incomingDeviceId !== null && this.trustedDevices.isTrusted(incomingDeviceId)
    console.log(`[remote ${ts}] handlePairAttempt:`, {
      status: this.status.kind,
      incomingDeviceId,
      lastPairedDeviceId: this.lastPairedDeviceId,
      isTrusted: isTrustedDevice,
    })

    // Trusted devices bypass the one-shot token check. This is the whole
    // reason the fix exists: a phone that paired previously reconnects
    // after a desktop restart with its persistent deviceId but no fresh
    // token (the old token from SavedInstance expired with the previous
    // session, and listening mode intentionally has no pendingToken).
    //
    // SECURITY: the MVP trust semantics (Phase 11) authenticate a trusted
    // device by `deviceId` alone. The deviceId is a long random string
    // generated by the mobile app and persisted in SecureStore, but it
    // travels on the wire — any LAN peer that captures a previous pair
    // frame could replay it. This is the same threat model as the
    // one-shot token (which also travels on the wire) for the same
    // LAN-only binding. Cryptographic challenge-response via publicKeyJwk
    // is deferred to Phase 14 hardening (see TrustedDeviceStore.ts:7-12).
    if (!isTrustedDevice) {
      if (!this.pendingToken) {
        return { ok: false, reason: 'no active pairing' }
      }
      if (!tokensMatch(msg.token, this.pendingToken)) {
        return { ok: false, reason: 'invalid token' }
      }
    }

    // Single-device policy: if we're already paired or have a pending peer,
    // reject newcomers — EXCEPT when the "new" pair attempt comes from the
    // same device that's already paired. That happens when the peer refreshes
    // its page: the browser tears down the old WebSocket, opens a new one,
    // and re-sends `pair` with the same persistent deviceId before the host
    // has processed the old WS close event. Without this allowance the
    // refresh would be rejected as "another device already paired" and the
    // session would stall.
    const isSameDeviceRefresh =
      incomingDeviceId !== null && incomingDeviceId === this.lastPairedDeviceId
    if ((this.pendingDevice || this.status.kind === 'paired') && !isSameDeviceRefresh) {
      return { ok: false, reason: 'another device is already paired' }
    }

    const device: PendingDevice = {
      deviceId: incomingDeviceId ?? randomBytes(8).toString('hex'),
      deviceName:
        typeof msg.deviceName === 'string' && msg.deviceName.length > 0
          ? msg.deviceName
          : 'Remote device',
      fingerprint: '',
      publicKeyJwk: msg.publicKeyJwk,
    }
    device.fingerprint = device.deviceId.slice(0, 8)
    this.pendingDevice = device

    // Auto-accept ONLY for explicitly trusted devices (user checked
    // "Remember this device" during a previous pair). Same-session
    // reconnect from an untrusted device still goes through the manual
    // accept modal — this matches the user's mental model ("I haven't
    // marked this device as trusted, so I should see the prompt every
    // time").
    //
    // The `isSameDeviceRefresh` check above is separate from auto-accept:
    // it only bypasses the single-device *rejection* so a refresh isn't
    // rejected as "another device already paired" while the host is
    // still cleaning up the old WebSocket — the refresh still hits the
    // manual accept modal.
    //
    // For trusted devices we bypass the `peerArrived` state entirely
    // and transition straight to `paired`. Emitting `peerArrived` first
    // and flipping to `paired` on the next tick briefly flashed the
    // accept modal even though the user had asked for automatic trust
    // — this eliminates that flash by never reaching a state that
    // would cause the modal to render.
    if (isTrustedDevice && this.currentPairing) {
      this.trustedDevices.updateLastSeen(device.deviceId)
      this.pendingDevice = null
      this.lastPairedDeviceName = device.deviceName
      this.lastPairedDeviceId = device.deviceId
      this.clearExpiryTimer()
      this.clearReaperTimer()
      this.resetIdleTimer()
      const pairing = this.currentPairing
      this.setStatus({
        kind: 'paired',
        hostname: pairing.hostname,
        lanIp: pairing.lanIp,
        port: pairing.port,
        deviceName: device.deviceName,
        connectedAt: Date.now(),
      })
      // Don't call `sendToPeer({type:'accepted'})` here — the active-peer
      // slot in SignalingServer is set AFTER this handler returns, so
      // sendToPeer would fire into a null/stale socket. Instead we set
      // `autoAccept: true` in the response and let SignalingServer send
      // the `accepted` frame right after `paired`, once `activePeer`
      // points at the correct WebSocket.
      return { ok: true, sessionId: device.deviceId, autoAccept: true }
    }

    if (this.currentPairing) {
      this.setStatus({
        kind: 'peerArrived',
        pairingUrl: this.currentPairing.pairingUrl,
        hostname: this.currentPairing.hostname,
        lanIp: this.currentPairing.lanIp,
        port: this.currentPairing.port,
        device,
      })
    }

    return { ok: true, sessionId: device.deviceId }
  }

  private handlePeerSignal(msg: unknown): void {
    // Peer signals are forwarded *only* to the host renderer. Broadcasting to
    // every window would give other tabs a chance to race-reply to SDP/ICE,
    // which would confuse the peer. The host window is the one that called
    // `remote:start` and owns the RTCPeerConnection.
    if (this.hostWcId === null) return
    const wc = webContentsNs.fromId(this.hostWcId)
    if (!wc || wc.isDestroyed()) return
    wc.send(REMOTE_SIGNAL_CHANNEL, msg)
  }

  private handlePeerDisconnected(): void {
    const ts = new Date().toISOString().slice(11, 23)
    console.log(`[remote ${ts}] handlePeerDisconnected: currentStatus=${this.status.kind}`)
    if (this.status.kind === 'peerArrived') {
      const pairing = this.currentPairing
      this.pendingDevice = null
      if (pairing) {
        this.setStatus({
          kind: 'waiting',
          pairingUrl: pairing.pairingUrl,
          hostname: pairing.hostname,
          lanIp: pairing.lanIp,
          port: pairing.port,
          expiresAt: pairing.expiresAt,
        })
      }
    } else if (this.status.kind === 'paired') {
      // Phase 10: enter reconnect window instead of immediate teardown.
      // The peer side will retry the signaling WebSocket with the same
      // token within 30s. If it reconnects in time, we skip the accept
      // modal and transition back to paired.
      const pairing = this.currentPairing
      if (pairing) {
        this.setStatus({
          kind: 'reconnecting',
          hostname: pairing.hostname,
          lanIp: pairing.lanIp,
          port: pairing.port,
          deviceName: this.lastPairedDeviceName ?? 'Remote device',
          reconnectingSince: Date.now(),
        })
        this.clearReaperTimer()
        this.reaperTimer = setTimeout(() => {
          if (this.status.kind === 'reconnecting') {
            // Normally this reaper tore the session down entirely. But when
            // the session was established from `listening` mode (auto-listen
            // for trusted devices), we want the server to remain bound so
            // the phone can reconnect later without the user reopening any
            // UI on the desktop. If we still have trusted devices and the
            // feature is enabled, drop back to `listening` instead of
            // stopping — cheap, and the whole point of this feature.
            if (this.canReturnToListening()) {
              console.log('[remote] reaper fired — returning to listening for trusted reconnects')
              this.returnToListening()
            } else {
              console.log('[remote] reaper fired — session teardown after disconnect timeout')
              this.stop().match(
                () => {},
                () => {},
              )
            }
          }
        }, REAPER_WINDOW_MS)
      } else {
        this.stop().match(
          () => {},
          () => {},
        )
      }
    }
  }

  /**
   * Called from the IPC signal handler whenever any message flows through
   * the session. Resets the idle timeout so interactive sessions don't
   * expire while the user is actively chatting with an agent.
   */
  resetIdleTimer(): void {
    this.clearIdleTimer()
    if (this.status.kind !== 'paired') return
    this.idleTimer = setTimeout(() => {
      if (this.status.kind === 'paired') {
        // Same calculus as the disconnect reaper above: if the user has
        // trusted devices and opted in, drop back to `listening` instead of
        // fully tearing down so the phone can wake the session back up
        // without UI gymnastics on the desktop.
        if (this.canReturnToListening()) {
          console.log('[remote] idle timeout — returning to listening for trusted reconnects')
          this.returnToListening()
        } else {
          console.log('[remote] idle timeout — auto-closing session')
          this.stop().match(
            () => {},
            () => {},
          )
        }
      }
    }, IDLE_TIMEOUT_MS)
  }

  /**
   * True iff a reaped/idled session should drop back to `listening` rather
   * than fully stop. Requires: opted in, ≥1 trusted device, server still
   * bound, and a populated `currentPairing` so we know the host/port to
   * stay listening on.
   */
  private canReturnToListening(): boolean {
    return (
      this.isEnabledInPreferences() &&
      this.trustedDevices.list().length > 0 &&
      this.signalingServer.isRunning &&
      this.currentPairing !== null
    )
  }

  /**
   * Soft reset back to listening mode without releasing the signaling
   * server port. Clears per-session state (pending token, pending device,
   * idle/reaper timers) but keeps `currentPairing` populated with
   * host/port so a subsequent trusted-device pair attempt can still pluck
   * those fields from it. Caller must have verified `canReturnToListening`.
   */
  private returnToListening(): void {
    const pairing = this.currentPairing
    if (!pairing) {
      this.stop().match(
        () => {},
        () => {},
      )
      return
    }
    this.clearExpiryTimer()
    this.clearReaperTimer()
    this.clearIdleTimer()
    this.pendingToken = null
    this.pendingDevice = null
    this.lastPairedDeviceName = null
    this.lastPairedDeviceId = null
    const { hostname, lanIp, port } = pairing
    this.currentPairing = {
      pairingUrl: '',
      hostname,
      lanIp,
      port,
      expiresAt: Number.POSITIVE_INFINITY,
    }
    this.setStatus({ kind: 'listening', hostname, lanIp, port })
  }

  // ===== helpers =====

  private setStatus(next: RemoteSessionStatus): void {
    const ts = new Date().toISOString().slice(11, 23)
    console.log(`[remote ${ts}] setStatus: ${this.status.kind} → ${next.kind}`)
    this.status = next
    this.broadcastStatus()
  }

  private broadcastStatus(): void {
    const payload = this.status
    for (const win of BrowserWindow.getAllWindows()) {
      if (!win.isDestroyed()) win.webContents.send(REMOTE_STATUS_CHANNEL, payload)
    }
  }

  private scheduleExpiry(ms: number): void {
    this.clearExpiryTimer()
    this.pairingExpiryTimer = setTimeout(() => {
      // Only auto-expire if still waiting for a scan; if peer arrived or
      // paired, keep going. On expiry: if we can drop back to listening
      // (user has trusted devices + feature enabled), do so — that keeps
      // the signaling port bound so a previously trusted phone can still
      // reconnect after the QR times out. Otherwise fully stop.
      if (this.status.kind === 'waiting') {
        if (this.canReturnToListening()) {
          console.log('[remote] QR expiry — returning to listening for trusted reconnects')
          this.returnToListening()
        } else {
          this.stop().match(
            () => {},
            () => {},
          )
        }
      }
    }, ms)
  }

  private clearExpiryTimer(): void {
    if (this.pairingExpiryTimer) {
      clearTimeout(this.pairingExpiryTimer)
      this.pairingExpiryTimer = null
    }
  }

  private clearReaperTimer(): void {
    if (this.reaperTimer) {
      clearTimeout(this.reaperTimer)
      this.reaperTimer = null
    }
  }

  private clearIdleTimer(): void {
    if (this.idleTimer) {
      clearTimeout(this.idleTimer)
      this.idleTimer = null
    }
  }

  private cleanupSession(): void {
    this.clearExpiryTimer()
    this.clearReaperTimer()
    this.clearIdleTimer()
    this.pendingToken = null
    this.currentPairing = null
    this.pendingDevice = null
    this.lastPairedDeviceName = null
    this.lastPairedDeviceId = null
  }

  private resolveBundleRoot(): string {
    // In a packaged build, the renderer output is at <app>/out/renderer.
    // In `electron-vite preview`, the same path applies. In `electron-vite dev`
    // the renderer is served from Vite's dev server and `out/renderer/` may not
    // exist; the RemoteClientHost.isAvailable() check will return false in
    // that case and the host will return 404 to remote-client bundle requests. The
    // signaling WebSocket still works in dev for verifying the IPC wiring.
    return path.join(app.getAppPath(), 'out', 'renderer')
  }
}

function buildPairingUrl(opts: {
  host: string
  port: number
  token: string
  hostname: string
}): string {
  // Query string cache buster. Safari's HTTP cache key ignores the URL
  // fragment (everything after `#`), which means every pairing URL in
  // this session shares the same cache slot as `http://host:port/remote/`
  // even though the token keeps changing. That lets a stale cached HTML
  // from a previous build get served on a fresh scan, and the stale
  // HTML references old JS bundle filenames that the new server doesn't
  // have → 404s, silent failures, the works. A query param forces Safari
  // to treat every scanned URL as a distinct cache entry and re-fetch
  // the HTML from the server every time.
  const params = new URLSearchParams()
  params.set('t', opts.token)
  params.set('h', opts.hostname)
  const cacheBuster = Date.now().toString(36)
  return `http://${opts.host}:${opts.port}/remote/?v=${cacheBuster}#${params.toString()}`
}
