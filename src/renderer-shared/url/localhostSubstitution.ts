/**
 * Replace the host portion of a URL with `hostLanIp` when the URL points
 * at `localhost`, `127.0.0.1`, `0.0.0.0`, or `::1`. Used by the remote
 * peer's "open externally" flow: if the Canopy host has
 * `http://localhost:3000` open in an embedded browser pane, the remote peer
 * (phone, laptop, etc.) can't reach it directly, but it *can* reach
 * `http://<host-lan-ip>:3000` as long as the dev server also binds on
 * `0.0.0.0`.
 *
 * Invalid URLs are returned unchanged so callers don't have to add another
 * try/catch layer.
 */
const LOCALHOST_HOSTS = new Set(['localhost', '127.0.0.1', '0.0.0.0', '::1'])

export function substituteLocalhost(rawUrl: string, hostLanIp: string): string {
  if (!hostLanIp) return rawUrl
  try {
    const url = new URL(rawUrl)
    if (LOCALHOST_HOSTS.has(url.hostname)) {
      url.hostname = hostLanIp
    }
    return url.toString()
  } catch {
    return rawUrl
  }
}
