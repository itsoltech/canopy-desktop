export type ParsedPairingUrl = {
  lanIp: string
  port: number
  token: string
  hostname: string
}

export function parsePairingUrl(input: string): ParsedPairingUrl | null {
  let url: URL
  try {
    url = new URL(input)
  } catch {
    return null
  }

  if (url.protocol !== 'http:' && url.protocol !== 'https:') return null
  if (url.pathname !== '/remote/' && url.pathname !== '/remote') return null
  if (!url.hostname) return null

  const port = Number(url.port)
  if (!Number.isInteger(port) || port <= 0 || port > 65535) return null

  const hash = url.hash.startsWith('#') ? url.hash.slice(1) : url.hash
  if (!hash) return null

  const params = new URLSearchParams(hash)
  const token = params.get('t')
  const hostnameRaw = params.get('h')
  if (!token || !hostnameRaw) return null

  if (!/^[0-9a-f]+$/i.test(token) || token.length < 16) return null

  let hostname: string
  try {
    hostname = decodeURIComponent(hostnameRaw)
  } catch {
    return null
  }
  if (!hostname) return null

  return {
    lanIp: url.hostname,
    port,
    token,
    hostname,
  }
}
