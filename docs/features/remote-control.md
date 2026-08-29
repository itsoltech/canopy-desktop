# Remote control

> Mirror and control a Canopy window from the mobile app on the same local network.

**Status:** Beta
**Introduced:** v0.10.0
**Platforms:** All

## Overview

Remote control lets you interact with a Canopy window from the mobile app on the same WiFi network. The feature uses a WebRTC data channel for the peer connection, with a local HTTP + WebSocket signaling server running on the desktop. Device pairing works through a QR code consumed by the mobile app; manually opening or sharing the underlying URL is not a supported pairing flow.

The pairing model is trust-on-first-use: when a new device connects, the desktop shows an accept/reject prompt displaying the device name and fingerprint. If the user checks "Remember this device," subsequent connections from that device skip the prompt. One device can be paired at a time.

The remote client is a single-page application served by the signaling server itself at `http://<lan-ip>:<port>/remote/`. The mobile app loads this client after scanning the QR code, then upgrades to a WebSocket for the signaling handshake before establishing the WebRTC data channel.

Remote control must be explicitly enabled in Settings before the Remote sidebar section appears.

## Behavior

### Enabling the feature

1. Open Settings and navigate to Remote Control.
2. Toggle "Enable remote control" on. This sets the `remote.enabled` preference to `true`.
3. The Remote section appears in the left sidebar with connection status, pairing controls, and basic configuration.
4. The `Listen on` control at the top of the section configures where Canopy listens for connections from trusted devices.

Once the feature is enabled **and** at least one trusted device exists, Canopy brings the signaling server up automatically on every app launch (see "Listen mode" below). Disabling the toggle or removing all trusted devices stops this auto-start on the next launch.

### Listen mode (trusted reconnect)

Listen mode lets a previously trusted device reconnect to Canopy after a desktop restart without the user having to start pairing first.

1. At app mount, the renderer calls `remote:ensureListening`. The main process silently no-ops unless `remote.enabled === 'true'`, the `TrustedDeviceStore` has at least one entry, and either `remote.selectedInterface` is set or `remote.listenAllInterfaces === 'true'`. If the selected-adapter listener cannot resolve its adapter yet (for example after OS restart while it is still coming up and has no routable IPv4), listen mode retries in the background with bounded backoff. Listen mode never surfaces errors to the user.
2. On success, the `SignalingServer` is bound either to the selected adapter's IPv4 address or to `0.0.0.0` when "All adapters" is selected, re-using the saved `remote.lastPort` when possible so the peer client's origin stays stable. The session transitions to `listening`. There is no pairing token in this state; only trusted devices whose `deviceId` already matches an entry in the store can pair. Untrusted pair attempts are rejected.
3. When a trusted peer connects, the service transitions directly from `listening` to `paired`, auto-accepts, and updates the `lastSeen` timestamp.
4. Clicking `Start listening` in the Remote sidebar starts manual listen mode even before a trusted device exists. Clicking `Pair device` while listen mode is active shows the `Adapter to embed in QR code` selector. The QR adapter starts empty and must be chosen explicitly. Pairing uses this transient QR adapter value rather than changing the listener setting.

Listen mode keeps the signaling server bound in the background for the lifetime of the app, not just while the pairing UI is open. The listener scope is user-controlled from the Remote sidebar. This is covered in "Security and privacy" below.

### Starting a pairing session

1. Open the Remote section in the left sidebar, click `Start listening`, then click `Pair device`. Select the adapter shown above the QR area; choosing an adapter generates the QR code for the mobile app.
2. Canopy calls `remote:start` with the chosen QR adapter, which triggers `RemoteSessionService.start()`. The session transitions to `starting`.
3. The service requires a QR adapter to be provided. Link-local APIPA addresses (`169.254.*`) are ignored. The named interface is used as-is (including normally-filtered virtual adapters like Tailscale — the user opted in explicitly); if that interface is not selected, no longer present, or has no routable IPv4 address, the service returns `NoNetworkInterface`.
4. A 32-byte random hex token is generated.
5. The `SignalingServer` starts an HTTP server with a preferred port (persisted in `remote.lastPort` preference), bound only on the selected interface's IPv4 address. If the preferred port is taken, it falls back to an ephemeral port. Reusing the same port keeps the peer-client origin stable so that the peer's localStorage (device ID, trust flag) survives Canopy restarts when the selected adapter address is unchanged.
6. The mobile-app QR payload is built as `http://<lan-ip>:<port>/remote/?v=<cache-buster>#t=<token>&h=<hostname>`.
7. The session transitions to `waiting` state with a 10-minute expiry. The QR code is displayed in the Remote sidebar section.
8. If no device connects within 10 minutes, the session auto-stops and returns to `idle`.

### Device pairing (new device)

1. The mobile app scans the QR code and loads the SPA from the signaling server.
2. The SPA opens a WebSocket to `/signaling` and sends a `pair` message containing the token, a device name, and a persistent device ID (from localStorage).
3. The signaling server validates the message format and forwards it to `RemoteSessionService.handlePairAttempt()`.
4. The service performs constant-time comparison of the token. If invalid, the peer receives `{ type: "rejected", reason: "invalid token" }` and the WebSocket closes.
5. Single-device policy: if another device is already paired (or pending), the attempt is rejected with "another device is already paired." An exception is made for same-device refresh (matching `deviceId`), which is allowed through to prevent stale-session lockouts.
6. On success, the session transitions to `peerArrived`. The desktop renderer shows the accept/reject prompt with the device name and an 8-character fingerprint (hex prefix of the device ID).
7. User clicks Accept (optionally checking "Remember this device").
8. `acceptPendingDevice()` transitions to `paired`, sends `{ type: "accepted" }` to the peer, and starts the idle timeout (15 minutes). If "Remember" was checked, the device is persisted in the `TrustedDeviceStore`.
9. The peer receives `accepted` and begins WebRTC offer/answer/ICE negotiation through the signaling WebSocket. The desktop renderer's `RemoteHostController` handles the SDP exchange.

### Device pairing (trusted device)

1. Steps 1-4 are the same as above.
2. At step 5, if the device ID matches an entry in the `TrustedDeviceStore`, the service skips the `peerArrived` state entirely and transitions directly to `paired`.
3. The signaling server sends `{ type: "paired" }` followed immediately by `{ type: "accepted" }` so the peer can start WebRTC negotiation without any manual approval.
4. The `lastSeen` timestamp on the trusted device record is updated.

### Peer disconnect and reconnection

1. When the peer WebSocket closes while in `paired` state, the session transitions to `reconnecting`.
2. A 30-second reaper timer starts. During this window, the peer can reconnect (e.g. after a page refresh) by re-sending a `pair` message with the same token and device ID.
3. If the peer reconnects within the window, the same-device-refresh check allows the pair attempt through, and the trust/accept flow runs again (auto-accept for trusted devices, manual accept otherwise).
4. If 30 seconds pass without reconnection, the reaper fires. When listen mode is eligible (feature enabled, ≥1 trusted device, server still bound, `currentPairing` populated with host/port), the session drops back to `listening` instead of fully stopping — the port stays bound so the trusted device can wake the session back up later without any UI on the desktop. Otherwise the reaper calls `stop()` and returns to `idle`.

### Rejecting a device

1. User clicks Reject on the accept prompt.
2. `rejectPendingDevice()` sends `{ type: "rejected", reason: "user rejected" }` to the peer, closes the WebSocket, and returns to `waiting` state. The QR code remains valid for another device to scan.

### Stopping a session

1. User explicitly stops the session from the Remote sidebar section.
2. `remote:stop` calls `RemoteSessionService.stop()`, which closes the peer WebSocket, stops the HTTP server, clears all timers (pairing expiry, reaper, idle), and returns to `idle`.

### Idle timeout

While paired, an idle timer of 15 minutes runs. Each signaling message (SDP, ICE, or data-channel relay) resets the timer. If no activity occurs for 15 minutes, the same listen-mode calculus from the disconnect reaper applies: when eligible, the session drops back to `listening` and keeps the port bound for trusted reconnects; otherwise it fully tears down to `idle`. QR expiry (the 10-minute `waiting` deadline) follows the same rule — it drops back to `listening` when eligible.

### Mobile terminal text selection

The native mobile app (`mobile/src/components/terminal/terminal-view.tsx`) enables xterm's `screenReaderMode`, which creates a DOM-based accessibility tree with selectable text overlaying the canvas. The overlay has `pointer-events: auto` so touch events reach it. The gesture detector distinguishes three interactions: short tap (< 500 ms) focuses the terminal and opens the soft keyboard, swipe scrolls the terminal buffer, and long-press (> 400 ms stationary) yields to the browser's native text selection flow. Selection-edge auto-scroll keeps extending the selection when the drag handle reaches the top or bottom of the terminal viewport.

### Mobile terminal keyboard toolbar

When the mobile soft keyboard is open, the terminal renders its action toolbar inside the WebView instead of using a separate native action bar. Keeping the toolbar in the WebView preserves xterm focus and selected terminal text while the user taps toolbar controls. The toolbar includes Hide, Copy, Paste, Esc, Tab, Shift+Tab, Ctrl, Alt, Left, Right, Up, Down, Home, End, and Enter.

Hide blurs xterm's hidden textarea to dismiss the soft keyboard. Copy sends only the current DOM terminal selection to the native app for clipboard writing; if there is no selection, the terminal shows a short transient notice and does not change the clipboard. Paste reads text from the native clipboard, sanitizes it with the mobile PTY paste wrapper, wraps it in bracketed-paste markers, and writes it to the active PTY without submitting Enter. The old native paste action bar below the terminal tabs is no longer shown.

## Configuration

| Preference key               | Values                 | Default   | Notes                                                                                                                                                                                                                          |
| ---------------------------- | ---------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `remote.enabled`             | `"true"` / `"false"`   | `"false"` | Must be `true` for the Remote sidebar section and trusted-device listen mode to appear                                                                                                                                         |
| `remote.lastPort`            | port number as string  | none      | Persisted automatically after first bind; keeps peer-client origin stable                                                                                                                                                      |
| `remote.selectedInterface`   | interface name or `""` | `""`      | Listener adapter used when `remote.listenAllInterfaces` is not `"true"`. Empty means selected-adapter listening is not configured. Missing or APIPA-only interface yields background retries for selected-adapter listen mode. |
| `remote.listenAllInterfaces` | `"true"` / `"false"`   | `"false"` | When `"true"`, trusted-device listen mode binds to `0.0.0.0` so known devices can reconnect through any active adapter. New QR pairing still asks for an explicit QR adapter and does not change this setting.                 |
| `remote.trustedDevices`      | JSON array             | `[]`      | Managed by TrustedDeviceStore; device names can be edited and devices can be removed in Settings                                                                                                                               |

Trusted devices can be viewed, renamed, and removed in Settings. Each entry stores `deviceId`, `name`, `addedAt`, and `lastSeen`.

## Error states

| Error                | User sees                                              | Cause                                                                                       |
| -------------------- | ------------------------------------------------------ | ------------------------------------------------------------------------------------------- |
| `AlreadyRunning`     | "Remote control session is already running"            | Calling `start()` when a session is already active                                          |
| `NotRunning`         | "Remote control session is not running"                | Calling `stop()` when no server is bound                                                    |
| `NoNetworkInterface` | "Select a usable network interface for remote control" | No adapter was selected, or the selected adapter is unavailable / APIPA-only                |
| `PortBindFailed`     | "Failed to bind signaling server: \<message\>"         | Both the preferred port and ephemeral fallback failed to bind                               |
| `BundleNotFound`     | "Remote client bundle not found at \<path\>"           | The built remote-client SPA is missing (happens in dev mode where Vite serves the renderer) |
| `TokenInvalid`       | "Invalid pairing token"                                | Peer presented a token that does not match the active one                                   |
| `NoPendingPeer`      | "No peer is currently waiting to be accepted"          | Accept/reject called when no device is in the `peerArrived` state                           |
| `PeerLimitReached`   | "Another device is already paired"                     | A second device tried to pair while one is already connected                                |
| `CertificateError`   | "Certificate error: \<message\>"                       | Self-signed TLS certificate generation or caching failed                                    |

## Security and privacy

Pairing sessions bind only on the explicitly chosen QR adapter's routable IPv4 address, and the QR URL always advertises that chosen address. Link-local APIPA addresses are ignored so a restarted adapter does not advertise an unreachable host. Listen mode defaults to the configured listener adapter, but the user can opt into "All adapters"; in that case the trusted-device listener binds to `0.0.0.0` and accepts reconnects through any active interface. It still accepts only trusted `deviceId`s; any other pair attempt is rejected before reaching the accept prompt.

The trusted-device list (`remote.trustedDevices`) is the sole auth factor for trusted-device auto-accept, so it is treated as a credential: it is encrypted at rest via the OS keychain (`safeStorage`) and excluded from the renderer-facing `db:prefs:getAll` blob. The renderer only ever reads it through the dedicated `remote.listTrustedDevices()` IPC, never as part of the bulk preferences read.

The signaling server shuts down when the session is explicitly stopped or when the reaper / idle / expiry paths tear the session down. Changing the effective listener scope stops and rebinds any in-flight session so the new bind takes effect on the next pairing or trusted reconnect. Changing `remote.selectedInterface` while `remote.listenAllInterfaces` is `"true"` does not rebind because the selected adapter is ignored in that mode. With **listen mode** (feature enabled, listener scope configured, and ≥1 trusted device), the server also comes up automatically at app launch and stays bound in the background so trusted devices can reconnect without the user starting pairing first — see "Listen mode" under Behavior.

Pairing tokens are 32 random bytes (hex-encoded, 64 characters). Token comparison uses Node.js `timingSafeEqual` to prevent timing attacks. Tokens are single-use per session.

WebSocket messages are capped at 256 KB per frame. Oversized frames cause immediate disconnection (close code 1009).

The remote-client SPA is served with a Content-Security-Policy header (`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ws: wss:; font-src 'self' data:; base-uri 'none'; object-src 'none'`) and `X-Frame-Options: DENY`. `base-uri` and `object-src` are stated explicitly because neither falls back to `default-src`.

Self-signed TLS certificates are generated with `selfsigned`, scoped to the current LAN IP as a subject-alt-name, cached at `<userData>/remote/cert.pem` and `key.pem` with restricted file permissions (0o600 for the private key, 0o700 for the directory). Certificates are regenerated when the LAN IP changes. HTTPS support via these certificates is not yet enabled; the signaling server currently operates over plain HTTP on the local network.

The trusted device store currently uses device ID matching only. Cryptographic challenge-response verification via Web Crypto (public key JWK) is planned but not yet implemented.

## Source files

- Service: `src/main/remote/RemoteSessionService.ts`
- Signaling server: `src/main/remote/SignalingServer.ts`
- Remote client host: `src/main/remote/RemoteClientHost.ts`
- Network discovery: `src/main/remote/discovery.ts`
- Certificate provider: `src/main/remote/CertificateProvider.ts`
- Trusted device store: `src/main/remote/TrustedDeviceStore.ts`
- Types: `src/main/remote/types.ts`
- Errors: `src/main/remote/errors.ts`
- Store: `src/renderer/src/lib/stores/remoteSession.svelte.ts`
- Preload: `src/preload/index.ts` (remote section)
- Components: `src/renderer/src/components/sidebar/RemoteSection.svelte`, `src/renderer/src/components/sidebar/RemoteControls.svelte`, `src/renderer/src/components/sidebar/RemoteSelectField.svelte`, `src/renderer/src/components/sidebar/RemoteSessionNotice.svelte`, `src/renderer/src/components/sidebar/RemoteStatusSummary.svelte`, `src/renderer/src/components/sidebar/RemotePairingQr.svelte`, `src/renderer/src/components/dialogs/RemoteAcceptDeviceModal.svelte`, `src/renderer/src/components/preferences/RemoteControlPrefs.svelte`
- Renderer helpers: `src/renderer/src/lib/remote/interfaceOptions.ts`
