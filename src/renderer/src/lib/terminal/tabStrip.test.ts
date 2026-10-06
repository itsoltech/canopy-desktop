import { describe, expect, it } from 'vitest'
import { splitTabStrip } from './tabStrip'

const tabs = ['a', 'b', 'c', 'd', 'e'].map((id) => ({ id }))
const ids = (list: Array<{ id: string }>): string[] => list.map((tab) => tab.id)

describe('splitTabStrip', () => {
  it('shows every tab when they all fit', () => {
    const { visible, overflow } = splitTabStrip(tabs, 5, 'e')
    expect(ids(visible)).toEqual(['a', 'b', 'c', 'd', 'e'])
    expect(overflow).toEqual([])
  })

  it('keeps tab order when the active tab is already visible', () => {
    const { visible, overflow } = splitTabStrip(tabs, 3, 'b')
    expect(ids(visible)).toEqual(['a', 'b', 'c'])
    expect(ids(overflow)).toEqual(['d', 'e'])
  })

  it('moves an overflowed active tab into the last visible slot', () => {
    const { visible, overflow } = splitTabStrip(tabs, 3, 'e')
    expect(ids(visible)).toEqual(['a', 'b', 'e'])
    expect(ids(overflow)).toEqual(['c', 'd'])
  })

  it('keeps the active tab visible when only one tab fits', () => {
    const { visible, overflow } = splitTabStrip(tabs, 1, 'd')
    expect(ids(visible)).toEqual(['d'])
    expect(ids(overflow)).toEqual(['a', 'b', 'c', 'e'])
  })

  it('shows every tab before the strip has been measured', () => {
    expect(ids(splitTabStrip(tabs, 0, 'e').visible)).toEqual(['a', 'b', 'c', 'd', 'e'])
  })
})
