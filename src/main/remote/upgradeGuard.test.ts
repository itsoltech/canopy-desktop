import { describe, it, expect } from 'vitest'
import { isAllowedUpgrade } from './upgradeGuard'

const LAN = '192.168.1.50:8080'

describe('isAllowedUpgrade', () => {
  it('allows the bundled peer client, whose Origin is the server it loaded from', () => {
    expect(isAllowedUpgrade({ host: LAN, origin: `http://${LAN}` })).toBe(true)
  })

  it('allows a client that sends no Origin at all', () => {
    // The React Native peer in mobile/ opens a bare `ws://<lanIp>:<port>/signaling`
    // without an Origin header. Omitting it wins an attacker nothing: the whole
    // point of the header is that browsers set it and pages cannot forge it.
    expect(isAllowedUpgrade({ host: LAN, origin: undefined })).toBe(true)
  })

  it('rejects a cross-site page opening a socket to the LAN address', () => {
    // Browsers send Origin on the WebSocket handshake but — unlike fetch/XHR —
    // do not block the handshake themselves, so this check is the only barrier.
    expect(isAllowedUpgrade({ host: LAN, origin: 'http://evil.example' })).toBe(false)
  })

  it('rejects a DNS-rebound domain even though its Origin matches its Host', () => {
    // evil.example re-resolves to the LAN IP, so Origin and Host agree and an
    // equality check alone would pass. Requiring an IP literal is what stops it.
    expect(
      isAllowedUpgrade({ host: 'evil.example:8080', origin: 'http://evil.example:8080' }),
    ).toBe(false)
  })

  it('rejects an opaque Origin from a sandboxed iframe or file:// page', () => {
    expect(isAllowedUpgrade({ host: LAN, origin: 'null' })).toBe(false)
  })

  it('rejects a request with no Host header', () => {
    expect(isAllowedUpgrade({ host: undefined, origin: `http://${LAN}` })).toBe(false)
  })

  it('allows loopback so local development and self-tests keep working', () => {
    expect(isAllowedUpgrade({ host: 'localhost:8080', origin: 'http://localhost:8080' })).toBe(true)
    expect(isAllowedUpgrade({ host: '127.0.0.1:8080', origin: 'http://127.0.0.1:8080' })).toBe(true)
  })

  it('handles a bracketed IPv6 host', () => {
    expect(isAllowedUpgrade({ host: '[::1]:8080', origin: 'http://[::1]:8080' })).toBe(true)
    expect(isAllowedUpgrade({ host: '[::1]:8080', origin: 'http://evil.example' })).toBe(false)
  })

  it('accepts an https Origin on the same host, for the planned TLS toggle', () => {
    expect(isAllowedUpgrade({ host: LAN, origin: `https://${LAN}` })).toBe(true)
  })

  it('rejects an Origin that only shares a prefix with the host', () => {
    expect(isAllowedUpgrade({ host: LAN, origin: `http://${LAN}.evil.example` })).toBe(false)
  })
})
