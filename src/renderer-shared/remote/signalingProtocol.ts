/**
 * Wire protocol for the WebRTC signaling WebSocket at `/signaling`.
 *
 * Terminology:
 *   - **host** = the Canopy desktop app with projects open (answerer)
 *   - **peer** = the remote client connecting in — could be a phone, a
 *                tablet, or another laptop; the protocol is device-agnostic
 *
 * The peer is the **offerer**: it creates the `RTCPeerConnection`, declares
 * the data channels, and sends the SDP offer first. The host
 * (via `RemoteHostController`) is the **answerer**.
 *
 * The handshake layers the WebRTC dance on top of an app-level pairing gate:
 *
 *   peer → server   : { type: 'pair', token, deviceName, deviceId }
 *   server → peer   : { type: 'paired', sessionId }           // token validated
 *   host user accepts in modal:
 *   server → peer   : { type: 'accepted' }                    // WebRTC may start
 *   peer → server → renderer : { type: 'offer', sdp }
 *   renderer → server → peer : { type: 'answer', sdp }
 *   both directions          : { type: 'ice', candidate }
 *
 * On rejection, server sends `{ type: 'rejected', reason }` and closes the WS.
 * Either side may send `{ type: 'bye' }` to indicate a clean disconnect.
 */

export type PeerSignal =
  | {
      type: 'pair'
      token: string
      deviceName: string
      deviceId: string
      publicKeyJwk?: JsonWebKey
    }
  | { type: 'offer'; sdp: RTCSessionDescriptionInit }
  | { type: 'answer'; sdp: RTCSessionDescriptionInit }
  | { type: 'ice'; candidate: RTCIceCandidateInit }
  | { type: 'bye'; reason?: string }

export type HostSignal =
  | { type: 'paired'; sessionId: string }
  | { type: 'rejected'; reason: string }
  | { type: 'accepted' }
  | { type: 'offer'; sdp: RTCSessionDescriptionInit }
  | { type: 'answer'; sdp: RTCSessionDescriptionInit }
  | { type: 'ice'; candidate: RTCIceCandidateInit }
  | { type: 'bye'; reason?: string }

/**
 * Subset of {@link PeerSignal} that the server forwards to the desktop host
 * renderer through the `remote:signal` IPC event. The `pair` message is
 * consumed entirely by `SignalingServer` + `RemoteSessionService`, so it
 * never reaches the renderer.
 */
export type InboundSignalForRenderer = Exclude<PeerSignal, { type: 'pair' }>

/**
 * Subset of {@link HostSignal} that the host renderer may produce and send
 * outbound via `window.api.remote.sendSignal`. Server-originated handshake
 * messages (`paired`, `accepted`, `rejected`) stay on the main-process side.
 */
export type OutboundSignalFromRenderer =
  | { type: 'offer'; sdp: RTCSessionDescriptionInit }
  | { type: 'answer'; sdp: RTCSessionDescriptionInit }
  | { type: 'ice'; candidate: RTCIceCandidateInit }
  | { type: 'bye'; reason?: string }
