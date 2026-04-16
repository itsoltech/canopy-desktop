// Keep in sync with src/renderer-shared/remote/iceConfig.ts when the wire protocol changes.

/**
 * ICE servers used by both the Canopy host and the remote peer when
 * establishing the WebRTC peer connection.
 *
 * MVP is **LAN-only**, so both endpoints rely exclusively on host (LAN)
 * candidates. We deliberately pass an empty list instead of public STUN
 * servers — phones on the same WiFi as the desktop can reach each other
 * directly over their private IPs, and STUN resolution can leak noisy errors.
 *
 * A future "remote over the internet" mode will re-introduce STUN/TURN.
 */
export const ICE_SERVERS: RTCIceServer[] = []
