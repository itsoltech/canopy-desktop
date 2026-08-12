import { describe, expect, it } from 'vitest'
import { ciJobPickerCopy } from './jobPickerCopy'

describe('ciJobPickerCopy', () => {
  it('uses GitHub workflow terminology without leaking TeamCity copy', () => {
    const copy = ciJobPickerCopy('github-actions')

    expect(copy.description).toContain('GitHub repository')
    expect(copy.description).toContain('Run workflow')
    expect(copy.description).not.toContain('TeamCity')
    expect(copy.description).not.toContain('build configurations')
    expect(copy.sharedSelection).toBe('the same selection')
  })
})
