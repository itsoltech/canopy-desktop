import { lookup } from 'dns/promises'
import { isIP, isIPv4, isIPv6 } from 'net'

const ALLOWED_EXTERNAL_SCHEMES = new Set(['http:', 'https:', 'mailto:'])

export function isSafeExternalUrl(url: string): boolean {
  try {
    const parsed = new URL(url)
    return ALLOWED_EXTERNAL_SCHEMES.has(parsed.protocol)
  } catch {
    return false
  }
}

function isPrivateIpv4(addr: string): boolean {
  const [a, b] = addr.split('.').map(Number)
  if (a === 0 || a === 10 || a === 127) return true
  if (a === 169 && b === 254) return true // link-local incl. 169.254.169.254 metadata
  if (a === 172 && b >= 16 && b <= 31) return true
  if (a === 192 && b === 168) return true
  if (a === 100 && b >= 64 && b <= 127) return true // CGNAT 100.64.0.0/10
  if (a >= 224) return true // multicast 224.0.0.0/4, reserved 240.0.0.0/4, broadcast
  return false
}

// The eight 16-bit groups of an IPv6 literal, with `::` expanded and a trailing
// dotted IPv4 folded into the last two groups.
function ipv6Groups(ip: string): number[] {
  let text = ip.toLowerCase().replace(/%.*$/, '')
  const dotted = text.match(/^(.*:)(\d+)\.(\d+)\.(\d+)\.(\d+)$/)
  if (dotted) {
    const [a, b, c, d] = dotted.slice(2).map(Number)
    text = `${dotted[1]}${((a << 8) | b).toString(16)}:${((c << 8) | d).toString(16)}`
  }
  const groups = (part: string): number[] =>
    part ? part.split(':').map((group) => parseInt(group, 16)) : []
  const [head, tail] = text.split('::')
  if (tail === undefined) return groups(head)
  const left = groups(head)
  const right = groups(tail)
  return [...left, ...Array<number>(8 - left.length - right.length).fill(0), ...right]
}

export function isPrivateIp(ip: string): boolean {
  if (isIPv4(ip)) return isPrivateIpv4(ip)
  if (!isIPv6(ip)) return false

  const g = ipv6Groups(ip)
  const zeros = (from: number, to: number): boolean => g.slice(from, to).every((x) => x === 0)
  const v4 = (hi: number, lo: number): string => `${hi >> 8}.${hi & 0xff}.${lo >> 8}.${lo & 0xff}`
  // Formats that carry an IPv4 address reach that address: IPv4-mapped (::ffff:a.b.c.d),
  // IPv4-translated (::ffff:0:a.b.c.d), NAT64 (64:ff9b::a.b.c.d) and 6to4 (2002:aabb:ccdd::).
  if (zeros(0, 5) && g[5] === 0xffff) return isPrivateIpv4(v4(g[6], g[7]))
  if (zeros(0, 4) && g[4] === 0xffff && g[5] === 0) return isPrivateIpv4(v4(g[6], g[7]))
  if (g[0] === 0x64 && g[1] === 0xff9b && zeros(2, 6)) return isPrivateIpv4(v4(g[6], g[7]))
  if (g[0] === 0x2002) return isPrivateIpv4(v4(g[1], g[2]))

  if (zeros(0, 7) && (g[7] === 0 || g[7] === 1)) return true // unspecified / loopback
  if ((g[0] & 0xfe00) === 0xfc00) return true // unique-local fc00::/7
  if ((g[0] & 0xffc0) === 0xfe80) return true // link-local fe80::/10
  if ((g[0] & 0xffc0) === 0xfec0) return true // site-local fec0::/10 (deprecated)
  if ((g[0] & 0xff00) === 0xff00) return true // multicast ff00::/8
  return false
}

export type HttpUrlNetworkClass = 'public' | 'private' | 'unresolved' | 'invalid'

/** Classifies the network reached by an HTTP URL without making the HTTP request. */
export async function classifyHttpUrl(rawUrl: string): Promise<HttpUrlNetworkClass> {
  let parsed: URL
  try {
    parsed = new URL(rawUrl)
  } catch {
    return 'invalid'
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return 'invalid'

  const host = parsed.hostname.replace(/^\[/, '').replace(/\]$/, '')
  if (isIP(host)) return isPrivateIp(host) ? 'private' : 'public'

  try {
    const records = await lookup(host, { all: true })
    if (records.length === 0) return 'unresolved'
    return records.some((record) => isPrivateIp(record.address)) ? 'private' : 'public'
  } catch {
    return 'unresolved'
  }
}

/**
 * Guards against SSRF for renderer-supplied URLs that the main process will
 * fetch (e.g. `skills:install` with an `http(s):` source). Rejects non-http(s)
 * schemes and any host that resolves to a private, loopback, link-local, or
 * cloud-metadata address. Callers must additionally pass `redirect: 'error'`
 * to `fetch` so a 3xx cannot bounce past this check to an internal address.
 */
export async function isPublicHttpUrl(rawUrl: string): Promise<boolean> {
  return (await classifyHttpUrl(rawUrl)) === 'public'
}
