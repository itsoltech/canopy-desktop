import { afterEach, describe, expect, it } from 'vitest'
import { prefs } from '../stores/preferences.svelte'
import { getAllViewports, getCustomViewports, getFavorites } from './browserState.svelte'

describe('stored browser favorites and viewports', () => {
  afterEach(() => {
    delete prefs['browser.favorites']
    delete prefs['viewports.custom']
  })

  it.each(['{}', 'null', '42', '"x"'])(
    'reads a favorites value that is valid JSON but not a list (%s) as empty',
    (raw) => {
      // Settings import accepts any string preference; the browser pane renders
      // `getFavorites().length`, so a non-array would break the whole pane.
      prefs['browser.favorites'] = raw
      expect(getFavorites()).toEqual([])
    },
  )

  it('keeps well-formed favorites and drops malformed entries', () => {
    prefs['browser.favorites'] = JSON.stringify([
      { url: 'https://a.test/', name: 'A', favicon: null },
      { url: 42, name: 'broken' },
      null,
      { url: 'https://b.test/' },
    ])
    expect(getFavorites()).toEqual([
      { url: 'https://a.test/', name: 'A', favicon: null },
      { url: 'https://b.test/', name: 'https://b.test/', favicon: null },
    ])
  })

  it('ignores custom viewports that are not presets', () => {
    prefs['viewports.custom'] = JSON.stringify({
      Good: { width: 400, height: 800, scaleFactor: 2, mobile: true },
      Bad: { width: 'wide' },
    })
    expect(getCustomViewports()).toEqual({
      Good: { width: 400, height: 800, scaleFactor: 2, mobile: true },
    })
    prefs['viewports.custom'] = '[1, 2]'
    expect(getAllViewports()).not.toHaveProperty('0')
  })
})
