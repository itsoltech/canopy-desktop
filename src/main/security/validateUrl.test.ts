import { beforeEach, describe, expect, it, vi } from 'vitest'

const lookupMock = vi.hoisted(() => vi.fn())

vi.mock('dns/promises', () => ({ lookup: lookupMock }))

import { classifyHttpUrl, isPublicHttpUrl } from './validateUrl'

describe('HTTP URL network classification', () => {
  beforeEach(() => lookupMock.mockReset())

  it.each([
    'http://127.0.0.1',
    'http://10.0.0.5',
    'https://169.254.169.254/latest/meta-data',
    'https://[::1]',
    'https://[fd00::1234]',
  ])('classifies the private literal %s without DNS', async (url) => {
    expect(await classifyHttpUrl(url)).toBe('private')
    expect(lookupMock).not.toHaveBeenCalled()
  })

  it('classifies a hostname as private when any DNS answer reaches a private network', async () => {
    lookupMock.mockResolvedValue([
      { address: '203.0.113.10', family: 4 },
      { address: '192.168.10.20', family: 4 },
    ])

    expect(await classifyHttpUrl('https://teamcity.example.test')).toBe('private')
  })

  it('keeps public, unresolved, and invalid destinations distinct', async () => {
    lookupMock.mockResolvedValueOnce([{ address: '203.0.113.10', family: 4 }])
    expect(await classifyHttpUrl('https://ci.example.test')).toBe('public')

    lookupMock.mockRejectedValueOnce(new Error('DNS unavailable'))
    expect(await classifyHttpUrl('https://split-dns.example.test')).toBe('unresolved')
    expect(await classifyHttpUrl('file:///etc/passwd')).toBe('invalid')
  })

  it('continues to reject private origins from the public-only URL guard', async () => {
    expect(await isPublicHttpUrl('http://192.168.1.10')).toBe(false)
  })

  it.each([
    'http://[64:ff9b::a00:1]', // NAT64 of 10.0.0.1
    'http://[64:ff9b::7f00:1]', // NAT64 of 127.0.0.1
    'http://[2002:a9fe:a9fe::1]', // 6to4 carrying 169.254.169.254
    'http://[::ffff:0:c0a8:101]', // IPv4-translated 192.168.1.1
    'http://[fec0::1]', // site-local
    'http://[ff02::1]', // multicast
    'http://224.0.0.1', // multicast
    'http://255.255.255.255', // limited broadcast
  ])('classifies %s, which reaches a non-public network, as private', async (url) => {
    expect(await classifyHttpUrl(url)).toBe('private')
  })

  it.each([
    'http://[64:ff9b::808:808]', // NAT64 of 8.8.8.8
    'http://[2002:808:808::1]', // 6to4 carrying 8.8.8.8
    'http://[2001:4860:4860::8888]',
  ])('keeps %s, which embeds or is a public address, public', async (url) => {
    expect(await classifyHttpUrl(url)).toBe('public')
  })

  it('classifies a NAT64 DNS answer by the IPv4 address it embeds', async () => {
    lookupMock.mockResolvedValueOnce([{ address: '64:ff9b::c0a8:a14', family: 6 }])
    expect(await classifyHttpUrl('https://ci.example.test')).toBe('private')
  })
})
