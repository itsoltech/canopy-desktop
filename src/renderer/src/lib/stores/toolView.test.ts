import { describe, it, expect, beforeEach, vi } from 'vitest'

// The store reads reactive prefs/tools stores (Svelte runes), which don't run
// in a plain node test env. Mock both with a mutable in-memory pref string and
// tool list so we can exercise the pure reconciliation and move/toggle logic.
const h = vi.hoisted(() => ({
  pref: { raw: '' },
  tools: { list: [] as { id: string }[] },
}))

vi.mock('./preferences.svelte', () => ({
  getPref: (_key: string, def = ''): string => h.pref.raw || def,
  setPref: (_key: string, value: string): Promise<void> => {
    h.pref.raw = value
    return Promise.resolve()
  },
}))

vi.mock('./tools.svelte', () => ({
  getTools: (): { id: string }[] => h.tools.list,
}))

import {
  getToolView,
  toggleToolVisibility,
  moveToolUp,
  moveToolDown,
  removeToolFromView,
} from './toolView.svelte'

function setTools(...ids: string[]): void {
  h.tools.list = ids.map((id) => ({ id }))
}

function setSaved(entries: { id: string; visible: boolean }[]): void {
  h.pref.raw = JSON.stringify(entries)
}

beforeEach(() => {
  h.pref.raw = ''
  h.tools.list = []
})

describe('getToolView reconciliation', () => {
  it('returns all tools visible in registry order when nothing is saved', () => {
    setTools('claude', 'gemini', 'codex')
    expect(getToolView()).toEqual([
      { id: 'claude', visible: true },
      { id: 'gemini', visible: true },
      { id: 'codex', visible: true },
    ])
  })

  it('keeps saved order and visibility for known ids', () => {
    setTools('claude', 'gemini', 'codex')
    setSaved([
      { id: 'codex', visible: false },
      { id: 'claude', visible: true },
      { id: 'gemini', visible: false },
    ])
    expect(getToolView()).toEqual([
      { id: 'codex', visible: false },
      { id: 'claude', visible: true },
      { id: 'gemini', visible: false },
    ])
  })

  it('appends newly added tools as visible at the end', () => {
    setTools('claude', 'gemini', 'newtool')
    setSaved([
      { id: 'gemini', visible: false },
      { id: 'claude', visible: true },
    ])
    expect(getToolView()).toEqual([
      { id: 'gemini', visible: false },
      { id: 'claude', visible: true },
      { id: 'newtool', visible: true },
    ])
  })

  it('drops saved ids no longer present in the tool set', () => {
    setTools('claude', 'gemini')
    setSaved([
      { id: 'removed', visible: false },
      { id: 'claude', visible: true },
      { id: 'gemini', visible: true },
    ])
    expect(getToolView()).toEqual([
      { id: 'claude', visible: true },
      { id: 'gemini', visible: true },
    ])
  })

  it('dedupes repeated ids in the saved config', () => {
    setTools('claude', 'gemini')
    setSaved([
      { id: 'claude', visible: false },
      { id: 'claude', visible: true },
      { id: 'gemini', visible: true },
    ])
    expect(getToolView()).toEqual([
      { id: 'claude', visible: false },
      { id: 'gemini', visible: true },
    ])
  })

  it('falls back to all-visible when the saved JSON is malformed', () => {
    setTools('claude', 'gemini')
    h.pref.raw = '{not valid json'
    expect(getToolView()).toEqual([
      { id: 'claude', visible: true },
      { id: 'gemini', visible: true },
    ])
  })

  it('ignores saved entries with the wrong shape', () => {
    setTools('claude', 'gemini')
    h.pref.raw = JSON.stringify([
      { id: 'claude' }, // missing visible
      { visible: true }, // missing id
      { id: 'gemini', visible: false },
    ])
    expect(getToolView()).toEqual([
      { id: 'gemini', visible: false },
      { id: 'claude', visible: true },
    ])
  })
})

describe('moveToolUp / moveToolDown', () => {
  beforeEach(() => {
    setTools('a', 'b', 'c')
    setSaved([
      { id: 'a', visible: true },
      { id: 'b', visible: true },
      { id: 'c', visible: true },
    ])
  })

  it('moveToolUp swaps a tool with its predecessor', () => {
    moveToolUp('b')
    expect(getToolView().map((e) => e.id)).toEqual(['b', 'a', 'c'])
  })

  it('moveToolUp on the first tool is a no-op', () => {
    moveToolUp('a')
    expect(getToolView().map((e) => e.id)).toEqual(['a', 'b', 'c'])
  })

  it('moveToolDown swaps a tool with its successor', () => {
    moveToolDown('b')
    expect(getToolView().map((e) => e.id)).toEqual(['a', 'c', 'b'])
  })

  it('moveToolDown on the last tool is a no-op', () => {
    moveToolDown('c')
    expect(getToolView().map((e) => e.id)).toEqual(['a', 'b', 'c'])
  })

  it('moving an unknown id is a no-op', () => {
    moveToolUp('zzz')
    moveToolDown('zzz')
    expect(getToolView().map((e) => e.id)).toEqual(['a', 'b', 'c'])
  })

  it('preserves visibility flags across a move', () => {
    setSaved([
      { id: 'a', visible: false },
      { id: 'b', visible: true },
      { id: 'c', visible: false },
    ])
    moveToolDown('a')
    expect(getToolView()).toEqual([
      { id: 'b', visible: true },
      { id: 'a', visible: false },
      { id: 'c', visible: false },
    ])
  })
})

describe('removeToolFromView', () => {
  it('prunes the id from the persisted config', () => {
    setTools('claude', 'custom')
    setSaved([
      { id: 'claude', visible: true },
      { id: 'custom', visible: false },
    ])
    removeToolFromView('custom')
    expect(JSON.parse(h.pref.raw)).toEqual([{ id: 'claude', visible: true }])
  })

  it('lets a re-added tool reconcile as visible at the end instead of restoring its old hidden slot', () => {
    // custom is hidden and ordered before claude.
    setTools('claude', 'custom')
    setSaved([
      { id: 'custom', visible: false },
      { id: 'claude', visible: true },
    ])
    removeToolFromView('custom')
    // Registry still (or again) reports custom: without a prune it would come
    // back hidden and first; after pruning it is a fresh visible entry appended
    // at the end.
    expect(getToolView()).toEqual([
      { id: 'claude', visible: true },
      { id: 'custom', visible: true },
    ])
  })

  it('does not write when the id is absent from the saved config', () => {
    h.pref.raw = ''
    removeToolFromView('custom')
    expect(h.pref.raw).toBe('')
  })
})

describe('toggleToolVisibility', () => {
  it('flips the visible flag and persists it', () => {
    setTools('a', 'b')
    toggleToolVisibility('a')
    expect(getToolView()).toEqual([
      { id: 'a', visible: false },
      { id: 'b', visible: true },
    ])
    toggleToolVisibility('a')
    expect(getToolView()).toEqual([
      { id: 'a', visible: true },
      { id: 'b', visible: true },
    ])
  })
})
