import { describe, expect, it } from 'vitest'
import { isFetchableFaviconUrl, readCappedBody } from './favicon'

describe('isFetchableFaviconUrl', () => {
  it.each([
    'https://example.com/favicon.ico',
    'http://localhost:5173/icon.png',
    'data:image/png;base64,AAAA',
  ])('allows %s', (url) => {
    expect(isFetchableFaviconUrl(url)).toBe(true)
  })

  it.each([
    'file:///dev/zero',
    'file:///home/user/.ssh/id_ed25519',
    'chrome://version',
    'icon.png',
  ])('refuses %s', (url) => {
    expect(isFetchableFaviconUrl(url)).toBe(false)
  })
})

describe('readCappedBody', () => {
  it('returns a body that fits the limit', async () => {
    const body = await readCappedBody(new Response('icon-bytes'), 64)
    expect(body?.toString()).toBe('icon-bytes')
  })

  it('gives up on a body larger than the limit', async () => {
    expect(await readCappedBody(new Response('x'.repeat(100)), 64)).toBeNull()
  })
})
