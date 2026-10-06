import { describe, expect, it } from 'vitest'
import { validateFilePath } from './repoFilePath'

describe('validateFilePath', () => {
  it('accepts repo-relative paths whose names contain consecutive dots', () => {
    expect(() => validateFilePath('app/[...slug]/page.tsx')).not.toThrow()
    expect(() => validateFilePath('docs/release..notes.md')).not.toThrow()
    expect(() => validateFilePath('src/index.ts')).not.toThrow()
  })

  it('rejects parent-directory segments, absolute paths and option-like paths', () => {
    expect(() => validateFilePath('../outside.txt')).toThrow()
    expect(() => validateFilePath('src/../../outside.txt')).toThrow()
    expect(() => validateFilePath('src\\..\\outside.txt')).toThrow()
    expect(() => validateFilePath('..')).toThrow()
    expect(() => validateFilePath('/etc/passwd')).toThrow()
    expect(() => validateFilePath('--output=x')).toThrow()
  })
})
