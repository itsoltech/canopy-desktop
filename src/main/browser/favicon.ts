/** Favicons are tiny; anything larger is not worth buffering in the main process. */
export const MAX_FAVICON_BYTES = 256 * 1024

const FAVICON_PROTOCOLS = new Set(['http:', 'https:', 'data:'])

/**
 * Favicon URLs come from the page. `session.fetch` would also read `file:` URLs (e.g.
 * file:///dev/zero, which never ends), so only web and inline icons may be fetched.
 */
export function isFetchableFaviconUrl(url: string): boolean {
  return URL.canParse(url) && FAVICON_PROTOCOLS.has(new URL(url).protocol)
}

/** Read a response body, giving up (null) as soon as it exceeds `limit` bytes. */
export async function readCappedBody(response: Response, limit: number): Promise<Buffer | null> {
  const declaredLength = Number(response.headers.get('content-length'))
  if (declaredLength > limit || !response.body) return null

  const reader = response.body.getReader()
  const chunks: Uint8Array[] = []
  let total = 0
  for (;;) {
    const { done, value } = await reader.read()
    if (done) return Buffer.concat(chunks)
    total += value.byteLength
    if (total > limit) {
      await reader.cancel()
      return null
    }
    chunks.push(value)
  }
}
