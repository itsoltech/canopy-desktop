import { describe, expect, it } from 'vitest'
import { parseCiConfig } from './config'

describe('parseCiConfig', () => {
  it('collapses duplicate ids (first wins) and enforces the 50-entry cap', () => {
    // A hand-edited or committed file is untrusted input: without the cap, every
    // entry becomes an authenticated status fetch on every poll.
    const dupes = parseCiConfig({
      provider: 'teamcity',
      baseUrl: 'https://x',
      buildTypes: [{ id: 'A', label: 'first' }, { id: 'A', label: 'second' }, { id: 'B' }],
    })
    expect(dupes?.buildTypes).toEqual([
      { id: 'A', label: 'first' },
      { id: 'B', label: 'B' },
    ])
    const many = parseCiConfig({
      provider: 'teamcity',
      baseUrl: 'https://x',
      buildTypes: Array.from({ length: 200 }, (_, i) => ({ id: `Bt_${i}` })),
    })
    expect(many?.buildTypes).toHaveLength(50)
    expect(many?.buildTypes[0]?.id).toBe('Bt_0')
  })

  it('caps labels at 100 chars, like the IPC save path', () => {
    const parsed = parseCiConfig({
      provider: 'teamcity',
      baseUrl: 'https://x',
      buildTypes: [{ id: 'A', label: 'x'.repeat(500) }],
    })
    expect(parsed?.buildTypes[0]?.label).toHaveLength(100)
  })

  it('accepts a valid teamcity config and normalizes the base URL', () => {
    expect(
      parseCiConfig({
        provider: 'teamcity',
        baseUrl: 'https://tc.example.com/',
        buildTypes: [{ id: 'Gakko_Build', label: 'Build' }],
      }),
    ).toEqual({
      provider: 'teamcity',
      baseUrl: 'https://tc.example.com',
      buildTypes: [{ id: 'Gakko_Build', label: 'Build' }],
    })
  })

  it('returns undefined for missing or non-object values', () => {
    expect(parseCiConfig(undefined)).toBeUndefined()
    expect(parseCiConfig(null)).toBeUndefined()
    expect(parseCiConfig('teamcity')).toBeUndefined()
  })

  it('rejects unknown providers and non-http(s) base URLs', () => {
    expect(
      parseCiConfig({ provider: 'jenkins', baseUrl: 'https://x', buildTypes: [{ id: 'A' }] }),
    ).toBeUndefined()
    expect(
      parseCiConfig({
        provider: 'teamcity',
        baseUrl: 'file:///etc',
        buildTypes: [{ id: 'A' }],
      }),
    ).toBeUndefined()
  })

  it('drops build types with invalid ids and falls back to id as label', () => {
    const parsed = parseCiConfig({
      provider: 'teamcity',
      baseUrl: 'https://tc.example.com',
      buildTypes: [
        { id: 'Gakko_Build' },
        { id: 'has spaces' },
        { id: 'evil),locator:(injection' },
        null,
        { label: 'no id' },
      ],
    })
    expect(parsed?.buildTypes).toEqual([{ id: 'Gakko_Build', label: 'Gakko_Build' }])
  })

  it('returns undefined when no valid build types remain', () => {
    expect(
      parseCiConfig({ provider: 'teamcity', baseUrl: 'https://tc.example.com', buildTypes: [] }),
    ).toBeUndefined()
  })
})
