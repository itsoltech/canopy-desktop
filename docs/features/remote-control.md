# Remote control

> Mirror and control a Canopy window from another device on the same local network.

**Status:** Beta
**Introduced:** v0.10.0
**Platforms:** All

## Overview

Remote control lets you interact with a Canopy window from a phone, tablet, or another laptop connected to the same WiFi network. The feature uses a WebRTC data channel for the peer connection, with a local HTTP + WebSocket signaling server running on the desktop. Device pairing works through a QR code that encodes a one-shot URL with a cryptographic token.

The pairing model is trust-on-first-use: when a new device connects, the desktop shows an accept/reject prompt displaying the device name and fingerprint. If the user checks "Remember this device," subsequent connections from that device skip the prompt. One device can be paired at a time.

The remote client is a single-page application served by the signaling server itself at `http://<lan-ip>:<port>/remote/`. The peer loads this page after scanning the QR code, then upgrades to a WebSocket for the signaling handshake before establishing the WebRTC data channel.

Remote control must be explicitly enabled in Settings before it appears in the command palette.

## Behavior

### Enabling the feature

1. Open Settings and navigate to Remote Control.
2. Toggle "Enable remote control" on. This sets the `remote.enabled` preference to `true`.
3. The "Remote Connection" command now appears in the command palette.

### Starting a pairing session

1. Open the command palette and select "Remote Connection."
2. Canopy calls `remote:start`, which triggers `RemoteSessionService.start()`. The session transitions to `starting`.
3. The service detects a usable LAN IPv4 interface by enumerating non-virtual network adapters. On macOS, interfaces named `en0`/`en1` (WiFi) are preferred. Virtual adapters (docker, vmnet, tailscale, utun, awdl, bridge, etc.) are filtered out.
4. A 32-byte random hex token is generated.
5. The `SignalingServer` starts an HTTP server bound on `0.0.0.0` with a preferred port (persisted in `remote.lastPort` preference). If the preferred port is taken, it falls back to an ephemeral port. Reusing the same port keeps the peer-client origin stable so that the peer's localStorage (device ID, trust flag) survives Canopy restarts.
6. The pairing URL is built as `http://<lan-ip>:<port>/remote/?v=<cache-buster>#t=<token>&h=<hostname>`.
7. The session transitions to `waiting` state with a 10-minute expiry. The QR code is displayed in the connection modal.
8. If no device connects within 10 minutes, the session auto-stops and returns to `idle`.

### Device pairing (new device)

1. The remote device scans the QR code and opens the URL. The browser loads the SPA from the signaling server.
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
4. If 30 seconds pass without reconnection, the reaper fires and calls `stop()`, tearing down the session and returning to `idle`.

### Rejecting a device

1. User clicks Reject on the accept prompt.
2. `rejectPendingDevice()` sends `{ type: "rejected", reason: "user rejected" }` to the peer, closes the WebSocket, and returns to `waiting` state. The QR code remains valid for another device to scan.

### Stopping a session

1. User closes the connection modal or explicitly stops the session.
2. `remote:stop` calls `RemoteSessionService.stop()`, which closes the peer WebSocket, stops the HTTP server, clears all timers (pairing expiry, reaper, idle), and returns to `idle`.

### Idle timeout

While paired, an idle timer of 15 minutes runs. Each signaling message (SDP, ICE, or data-channel relay) resets the timer. If no activity occurs for 15 minutes, the session auto-closes.

## Configuration

| Preference key          | Values                | Default   | Notes                                                                     |
| ----------------------- | --------------------- | --------- | ------------------------------------------------------------------------- |
| `remote.enabled`        | `"true"` / `"false"`  | `"false"` | Must be `true` for the command palette entry to appear                    |
| `remote.lastPort`       | port number as string | none      | Persisted automatically after first bind; keeps peer-client origin stable |
| `remote.trustedDevices` | JSON array            | `[]`      | Managed by TrustedDeviceStore; not user-editable                          |

Trusted devices can be viewed and removed in Settings. Each entry stores `deviceId`, `name`, `addedAt`, and `lastSeen`.

## Error states

| Error                | User sees                                      | Cause                                                                                       |
| -------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `AlreadyRunning`     | "Remote control session is already running"    | Calling `start()` when a session is already active                                          |
| `NotRunning`         | "Remote control session is not running"        | Calling `stop()` when no server is bound                                                    |
| `NoNetworkInterface` | "No usable network interface found on the LAN" | All detected interfaces are loopback or virtual; no WiFi/Ethernet adapter available         |
| `PortBindFailed`     | "Failed to bind signaling server: \<message\>" | Both the preferred port and ephemeral fallback failed to bind                               |
| `BundleNotFound`     | "Remote client bundle not found at \<path\>"   | The built remote-client SPA is missing (happens in dev mode where Vite serves the renderer) |
| `TokenInvalid`       | "Invalid pairing token"                        | Peer presented a token that does not match the active one                                   |
| `NoPendingPeer`      | "No peer is currently waiting to be accepted"  | Accept/reject called when no device is in the `peerArrived` state                           |
| `PeerLimitReached`   | "Another device is already paired"             | A second device tried to pair while one is already connected                                |
| `CertificateError`   | "Certificate error: \<message\>"               | Self-signed TLS certificate generation or caching failed                                    |

## Security and privacy

The signaling server binds on `0.0.0.0`, making it reachable from any device on the local network. It only listens while a session is explicitly active (user opened the modal) and shuts down when the session ends.

Pairing tokens are 32 random bytes (hex-encoded, 64 characters). Token comparison uses Node.js `timingSafeEqual` to prevent timing attacks. Tokens are single-use per session.

WebSocket messages are capped at 256 KB per frame. Oversized frames cause immediate disconnection (close code 1009).

The remote-client SPA is served with a Content-Security-Policy header (`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ws: wss:; font-src 'self' data:`) and `X-Frame-Options: DENY`.

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
- Components: `src/renderer/src/components/dialogs/RemoteConnectionModal.svelte`, `RemoteAcceptDeviceModal.svelte`, `src/renderer/src/components/preferences/RemoteControlPrefs.svelte`
