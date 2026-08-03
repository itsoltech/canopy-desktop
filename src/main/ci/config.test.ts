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
    }).config
    expect(dupes?.buildTypes).toEqual([
      { id: 'A', label: 'first' },
      { id: 'B', label: 'B' },
    ])
    const many = parseCiConfig({
      provider: 'teamcity',
      baseUrl: 'https://x',
      buildTypes: Array.from({ length: 200 }, (_, i) => ({ id: `Bt_${i}` })),
    }).config
    expect(many?.buildTypes).toHaveLength(50)
    expect(many?.buildTypes[0]?.id).toBe('Bt_0')
    // NOT silent: the configurator announces the dropped entries (a capped sample
    // of names, exact count) before a Save would delete them from the file.
    expect(many?.droppedOverCap?.count).toBe(150)
    expect(many?.droppedOverCap?.ids).toHaveLength(10)
    expect(many?.droppedOverCap?.ids[0]).toBe('Bt_50')
    expect(dupes?.droppedOverCap).toBeUndefined()
    expect(dupes?.droppedInvalid).toBeUndefined()
  })

  it('keeps the two drop populations separate - their recoveries are opposite', () => {
    // Ten invalid ids must not crowd the over-cap names out of the sample: the
    // invalid ones need a fix-the-file message, the over-cap ones a re-tick.
    const parsed = parseCiConfig({
      provider: 'teamcity',
      baseUrl: 'https://x',
      buildTypes: [
        ...Array.from({ length: 12 }, (_, i) => ({ id: `bad id ${i}` })),
        ...Array.from({ length: 55 }, (_, i) => ({ id: `Bt_${i}` })),
      ],
    }).config
    expect(parsed?.buildTypes).toHaveLength(50)
    expect(parsed?.droppedInvalid?.count).toBe(12)
    expect(parsed?.droppedInvalid?.ids).toHaveLength(10)
    expect(parsed?.droppedOverCap?.count).toBe(5)
    expect(parsed?.droppedOverCap?.ids).toEqual(['Bt_50', 'Bt_51', 'Bt_52', 'Bt_53', 'Bt_54'])
  })

  it('keeps over-cap ids intact - they are match keys, not just display text', () => {
    // The configurator filters these against the server's build types; a
    // truncated id can never match and would report a live job as deleted.
    const longId = `Deep_${'Nested_'.repeat(20)}Job`
    const parsed = parseCiConfig({
      provider: 'teamcity',
      baseUrl: 'https://x',
      buildTypes: [...Array.from({ length: 50 }, (_, i) => ({ id: `Bt_${i}` })), { id: longId }],
    }).config
    expect(longId.length).toBeGreaterThan(80)
    expect(parsed?.droppedOverCap?.ids).toEqual([longId])
  })

  it('names the invalid ids even when NOTHING survives', () => {
    // A bulk rename typos every id: the block yields no config, but the names
    // must still reach the block-scope error instead of a generic shape message
    // steering the user at a Save that deletes them unnamed.
    const parsed = parseCiConfig({
      provider: 'teamcity',
      baseUrl: 'https://x',
      buildTypes: [{ id: 'Gakko-Build' }, { id: 'Gakko-Tests' }],
    })
    expect(parsed.config).toBeUndefined()
    expect(parsed.invalidIds).toEqual(['Gakko-Build', 'Gakko-Tests'])
  })

  it('caps labels at 100 chars, like the IPC save path', () => {
    const parsed = parseCiConfig({
      provider: 'teamcity',
      baseUrl: 'https://x',
      buildTypes: [{ id: 'A', label: 'x'.repeat(500) }],
    }).config
    expect(parsed?.buildTypes[0]?.label).toHaveLength(100)
  })

  it('accepts a valid teamcity config and normalizes the base URL', () => {
    expect(
      parseCiConfig({
        provider: 'teamcity',
        baseUrl: 'https://tc.example.com/',
        buildTypes: [{ id: 'Gakko_Build', label: 'Build' }],
      }).config,
    ).toEqual({
      provider: 'teamcity',
      baseUrl: 'https://tc.example.com',
      buildTypes: [{ id: 'Gakko_Build', label: 'Build' }],
    })
  })

  it('yields no config for missing or non-object values', () => {
    expect(parseCiConfig(undefined).config).toBeUndefined()
    expect(parseCiConfig(null).config).toBeUndefined()
    expect(parseCiConfig('teamcity').config).toBeUndefined()
  })

  it('rejects unknown providers and non-http(s) base URLs', () => {
    expect(
      parseCiConfig({ provider: 'jenkins', baseUrl: 'https://x', buildTypes: [{ id: 'A' }] })
        .config,
    ).toBeUndefined()
    expect(
      parseCiConfig({
        provider: 'teamcity',
        baseUrl: 'file:///etc',
        buildTypes: [{ id: 'A' }],
      }).config,
    ).toBeUndefined()
  })

  it('drops build types with invalid ids and falls back to id as label', () => {
    const { config: parsed } = parseCiConfig({
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
    // A string id that fails the charset is a TYPO carrying user intent — it must
    // be counted and named, or a Save deletes it from the git-shared file
    // unannounced. Non-object entries and id-less objects lose nothing.
    expect(parsed?.droppedInvalid?.count).toBe(2)
    expect(parsed?.droppedInvalid?.ids).toEqual(['has spaces', 'evil),locator:(injection'])
  })

  it('yields no config when no valid build types remain', () => {
    expect(
      parseCiConfig({ provider: 'teamcity', baseUrl: 'https://tc.example.com', buildTypes: [] })
        .config,
    ).toBeUndefined()
  })
})
