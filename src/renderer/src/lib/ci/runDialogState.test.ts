import { describe, expect, it } from 'vitest'
import { ambiguousCiRefNames, nextCiRunStage, previousCiRunStage } from './runDialogState.svelte'

describe('CI run stage navigation', () => {
  it('always requires confirmation, including jobs without parameters', () => {
    expect(nextCiRunStage('select', false)).toBe('confirm')
    expect(nextCiRunStage('select', true)).toBe('configure')
    expect(nextCiRunStage('configure', true)).toBe('confirm')
    expect(nextCiRunStage('confirm', false)).toBeNull()
  })

  it('backs up one step while leaving select as the close boundary', () => {
    expect(previousCiRunStage('confirm', false)).toBe('select')
    expect(previousCiRunStage('confirm', true)).toBe('configure')
    expect(previousCiRunStage('configure', true)).toBe('select')
    expect(previousCiRunStage('select', false)).toBeNull()
  })
})

describe('ambiguousCiRefNames', () => {
  it('reports names shared by a branch and tag without flagging same-kind duplicates', () => {
    expect(
      ambiguousCiRefNames([
        { name: 'develop', kind: 'branch' },
        { name: 'release', kind: 'branch' },
        { name: 'release', kind: 'tag' },
        { name: 'v1', kind: 'tag' },
        { name: 'v1', kind: 'tag' },
      ]),
    ).toEqual(['release'])
  })
})
