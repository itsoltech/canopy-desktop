import { beforeEach, describe, expect, it, vi } from 'vitest'

const api = {
  setPref: vi.fn(async () => {}),
}

vi.stubGlobal('window', { api })

import { prefs } from '../stores/preferences.svelte'
import { getFavorites, updateFavorite } from './browserState.svelte'

describe('updateFavorite', () => {
  beforeEach(() => {
    prefs['browser.favorites'] = JSON.stringify([
      { url: 'https://a.example/', name: 'A', favicon: null },
      { url: 'https://b.example/', name: 'B', favicon: null },
    ])
  })

  it('keeps one entry per URL when an edit moves a favorite onto another one', () => {
    updateFavorite('https://a.example/', { url: 'https://b.example/', name: 'A2', favicon: null })

    expect(getFavorites()).toEqual([{ url: 'https://b.example/', name: 'A2', favicon: null }])
  })

  it('updates a favorite in place when its URL does not change', () => {
    updateFavorite('https://b.example/', { url: 'https://b.example/', name: 'B2', favicon: null })

    expect(getFavorites().map((f) => f.name)).toEqual(['A', 'B2'])
  })
})
