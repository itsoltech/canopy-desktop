import { describe, it, expect } from 'vitest'
import { extractTaskKey, extractTaskKeys } from './branchTaskKey'

describe('extractTaskKeys', () => {
  it('returns every key in order of appearance (parent/subtask branches)', () => {
    expect(extractTaskKeys('s115/GAKKO-100/GAKKO-123-poprawka')).toEqual(['GAKKO-100', 'GAKKO-123'])
  })

  it('deduplicates repeated keys', () => {
    expect(extractTaskKeys('GAKKO-1/GAKKO-1-retry')).toEqual(['GAKKO-1'])
  })

  it('returns a single-element list for one key', () => {
    expect(extractTaskKeys('s102/GAKKO-2754/nowy-panel-wpisow')).toEqual(['GAKKO-2754'])
  })

  it('returns an empty list when no key is present', () => {
    expect(extractTaskKeys('master')).toEqual([])
    expect(extractTaskKeys('')).toEqual([])
  })
})

describe('extractTaskKey', () => {
  it('finds the key in template-shaped branch names', () => {
    expect(extractTaskKey('s102/GAKKO-2754/nowy-panel-wpisow')).toBe('GAKKO-2754')
    expect(extractTaskKey('feature/ABC-1-fix-login')).toBe('ABC-1')
    expect(extractTaskKey('feat/PROJ-42-something')).toBe('PROJ-42')
  })

  it('finds the key when it is the whole name or at the start', () => {
    expect(extractTaskKey('GAKKO-5262')).toBe('GAKKO-5262')
    expect(extractTaskKey('ISSUE-123-hotfix')).toBe('ISSUE-123')
  })

  it('returns the FIRST key when several appear', () => {
    expect(extractTaskKey('s115/GAKKO-100/GAKKO-123')).toBe('GAKKO-100')
  })

  it('supports underscores and digits in the project prefix', () => {
    expect(extractTaskKey('x/AB_2C-77-title')).toBe('AB_2C-77')
  })

  it('returns null when no key is present', () => {
    expect(extractTaskKey('master')).toBeNull()
    expect(extractTaskKey('develop')).toBeNull()
    expect(extractTaskKey('s113-foo')).toBeNull()
    expect(extractTaskKey('teamcity-db-override-matrix')).toBeNull()
    expect(extractTaskKey('')).toBeNull()
  })

  it('does not treat lowercase segments as keys', () => {
    expect(extractTaskKey('fix/abc-123-lowercase')).toBeNull()
  })
})
