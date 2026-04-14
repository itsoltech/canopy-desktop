// Copied from src/renderer-shared/remote/signalingProtocol.ts
// Keep in sync when the wire protocol changes.

/**
 * Wire protocol for the WebRTC signaling WebSocket at `/signaling`.
 *
 * Terminology:
 *   - **host** = the Canopy desktop app with projects open (answerer)
 *   - **peer** = the remote client connecting in — could be a phone, a
 *                tablet, or another laptop
 *
 * The peer is the **offerer**: it creates the `RTCPeerConnection`, declares
 * the data channels, and sends the SDP offer first.
 *
 * Handshake:
 *
 *   peer → server   : { type: 'pair', token, deviceName, deviceId }
 *   server → peer   : { type: 'paired', sessionId }
 *   host user accepts in modal:
 *   server → peer   : { type: 'accepted' }
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
