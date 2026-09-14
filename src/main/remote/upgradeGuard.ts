import { isIP } from 'node:net'

/**
 * Decide whether a WebSocket upgrade may proceed, from the request's `Host` and
 * `Origin` headers alone.
 *
 * The signaling server is deliberately bound to a LAN adapter so a phone can
 * reach it, which also puts it in reach of any page the user happens to have
 * open. Browsers send `Origin` on the WebSocket handshake but — unlike
 * `fetch`/XHR — do not block the handshake themselves, so without a check here
 * an arbitrary web page could open a socket to the LAN address and start
 * guessing pairing tokens or replaying a trusted `deviceId`.
 *
 * Two conditions, because they stop different attacks:
 *
 *   - `Host` must name an IP literal (or loopback). The peer client is always
 *     reached by address — the QR payload is `http://<lan-ip>:<port>/remote/` —
 *     so a hostname in `Host` means a DNS-rebound domain, which would otherwise
 *     present a self-consistent `Origin`/`Host` pair and sail through the check
 *     below.
 *   - `Origin`, when present, must be this same host. That stops an ordinary
 *     cross-site page, whose `Origin` is its own site.
 *
 * A missing `Origin` is allowed: the React Native peer in `mobile/` sends none,
 * and omitting the header gains an attacker nothing, since the whole point of
 * `Origin` is that browsers set it and pages cannot forge it.
 */
export function isAllowedUpgrade(headers: {
  host: string | undefined
  origin: string | undefined
}): boolean {
  const host = headers.host
  if (!host) return false

  const hostname = hostnameOf(host)
  if (!isIP(hostname) && hostname !== 'localhost') return false

  const origin = headers.origin
  if (origin === undefined) return true

  // Both schemes, so the documented HTTPS/wss toggle keeps working when it lands
  // — the scheme is whatever the client loaded this same bundle over.
  return origin === `http://${host}` || origin === `https://${host}`
}

/** `"192.168.1.5:8080"` → `"192.168.1.5"`, `"[::1]:8080"` → `"::1"`. */
function hostnameOf(host: string): string {
  if (host.startsWith('[')) {
    const end = host.indexOf(']')
    return end === -1 ? '' : host.slice(1, end)
  }
  const colon = host.indexOf(':')
  return colon === -1 ? host : host.slice(0, colon)
}
