/**
 * ICE servers used by both the Canopy host and the remote peer when establishing
 * the WebRTC peer connection.
 *
 * MVP is **LAN-only**, so both endpoints rely exclusively on host (LAN)
 * candidates. We deliberately pass an empty list instead of public STUN
 * servers because:
 *
 *   1. STUN isn't needed — phones on the same WiFi as the desktop can reach
 *      each other directly over their private IPs.
 *   2. Electron's renderer sandbox frequently can't resolve public STUN
 *      hostnames (`ERR_NAME_NOT_RESOLVED`, net errorcode `-105`), which
 *      leaks noisy `socket_manager.cc` errors into the console and delays
 *      ICE gathering completion.
 *
 * A future "remote over the internet" mode (Phase 14+) will re-introduce
 * STUN/TURN once the port-forwarding workflow is wired up.
 */
export const ICE_SERVERS: RTCIceServer[] = []
