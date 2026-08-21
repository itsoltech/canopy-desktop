import { describe, expect, it } from 'vitest'
import { isAllowedWsOrigin } from './SignalingServer'

const PORT = 7788

describe('isAllowedWsOrigin', () => {
  it('allows a peer that sends no Origin header', () => {
    // The Expo mobile client opens a bare WebSocket, which sends no Origin.
    // A browser can never omit the header, so allowing this case does not
    // weaken the cross-site check below.
    expect(isAllowedWsOrigin(undefined, PORT)).toBe(true)
  })

  it('allows the opaque "null" origin so non-browser clients keep working', () => {
    expect(isAllowedWsOrigin('null', PORT)).toBe(true)
  })

  it('allows the LAN URL the host actually advertises', () => {
    expect(isAllowedWsOrigin(`http://192.168.1.5:${PORT}`, PORT)).toBe(true)
  })

  it('allows a loopback and an IPv6 literal origin on the same port', () => {
    expect(isAllowedWsOrigin(`http://127.0.0.1:${PORT}`, PORT)).toBe(true)
    expect(isAllowedWsOrigin(`http://[::1]:${PORT}`, PORT)).toBe(true)
  })

  it('rejects a foreign site attempting cross-site WebSocket hijacking', () => {
    expect(isAllowedWsOrigin('https://evil.example', PORT)).toBe(false)
    expect(isAllowedWsOrigin(`https://evil.example:${PORT}`, PORT)).toBe(false)
  })

  it('rejects a hostname origin that resolves to this machine (DNS rebinding)', () => {
    // The advertised URL is always an IP literal, so a name-based origin on
    // our port is never a genuine peer.
    expect(isAllowedWsOrigin(`http://rebind.evil.example:${PORT}`, PORT)).toBe(false)
  })

  it('rejects a matching host on a different port', () => {
    expect(isAllowedWsOrigin(`http://192.168.1.5:${PORT + 1}`, PORT)).toBe(false)
  })

  it('rejects non-http schemes and unparseable origins', () => {
    expect(isAllowedWsOrigin(`file://192.168.1.5:${PORT}`, PORT)).toBe(false)
    expect(isAllowedWsOrigin('not a url', PORT)).toBe(false)
  })
})
