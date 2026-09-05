import { describe, expect, it } from 'vitest'
import { hasRemoteName } from './remoteNames'

describe('hasRemoteName', () => {
  it('detects origin from the data output of git remote', () => {
    expect(hasRemoteName('upstream\norigin\n', 'origin')).toBe(true)
  })

  it('does not infer a missing remote from an error message', () => {
    expect(hasRemoteName('fatal: No such remote origin', 'origin')).toBe(false)
  })
})
