import { lookup } from 'dns/promises'
import { isIP, isIPv4 } from 'net'

const ALLOWED_EXTERNAL_SCHEMES = new Set(['http:', 'https:', 'mailto:'])

export function isSafeExternalUrl(url: string): boolean {
  try {
    const parsed = new URL(url)
    return ALLOWED_EXTERNAL_SCHEMES.has(parsed.protocol)
  } catch {
    return false
  }
}

function isPrivateIp(ip: string): boolean {
  // Unwrap IPv4-mapped IPv6. URL.hostname serializes embedded IPv4 as two
  // hex hextets (e.g. ::ffff:a9fe:a9fe), so handle both forms.
  const lowerIp = ip.toLowerCase()
  let addr = ip
  const dotted = lowerIp.match(/^::ffff:(\d+\.\d+\.\d+\.\d+)$/)
  const hex = lowerIp.match(/^::ffff:([0-9a-f]{1,4}):([0-9a-f]{1,4})$/)
  if (dotted) {
    addr = dotted[1]
  } else if (hex) {
    const hi = parseInt(hex[1], 16)
    const lo = parseInt(hex[2], 16)
    addr = `${hi >> 8}.${hi & 0xff}.${lo >> 8}.${lo & 0xff}`
  }

  if (isIPv4(addr)) {
    const [a, b] = addr.split('.').map(Number)
    if (a === 0 || a === 10 || a === 127) return true
    if (a === 169 && b === 254) return true // link-local incl. 169.254.169.254 metadata
    if (a === 172 && b >= 16 && b <= 31) return true
    if (a === 192 && b === 168) return true
    if (a === 100 && b >= 64 && b <= 127) return true // CGNAT 100.64.0.0/10
    return false
  }

  const lower = lowerIp
  if (lower === '::1' || lower === '::') return true // loopback / unspecified
  if (lower.startsWith('fc') || lower.startsWith('fd')) return true // unique-local fc00::/7
  if (/^fe[89ab]/.test(lower)) return true // link-local fe80::/10
  return false
}

/**
 * Guards against SSRF for renderer-supplied URLs that the main process will
 * fetch (e.g. `skills:install` with an `http(s):` source). Rejects non-http(s)
 * schemes and any host that resolves to a private, loopback, link-local, or
 * cloud-metadata address. Callers must additionally pass `redirect: 'error'`
 * to `fetch` so a 3xx cannot bounce past this check to an internal address.
 */
export async function isPublicHttpUrl(rawUrl: string): Promise<boolean> {
  let parsed: URL
  try {
    parsed = new URL(rawUrl)
  } catch {
    return false
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return false

  const host = parsed.hostname.replace(/^\[/, '').replace(/\]$/, '')
  if (isIP(host) && isPrivateIp(host)) return false

  try {
    const records = await lookup(host, { all: true })
    if (records.length === 0) return false
    return !records.some((r) => isPrivateIp(r.address))
  } catch {
    return false
  }
}
