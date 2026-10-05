import { describe, expect, it } from 'vitest'
import { parsePorcelainEntry } from './porcelainStatus'

describe('parsePorcelainEntry', () => {
  it('reads a plain path', () => {
    expect(parsePorcelainEntry(' M src/main/index.ts')).toEqual({
      xy: ' M',
      path: 'src/main/index.ts',
      origPath: null,
    })
  })

  it('unquotes paths git quotes because they contain spaces', () => {
    expect(parsePorcelainEntry(' M "docs/my notes.md"')?.path).toBe('docs/my notes.md')
  })

  it('decodes octal-escaped UTF-8 bytes and backslash escapes', () => {
    expect(parsePorcelainEntry(' M "za\\305\\274\\303\\263\\305\\202\\304\\207.md"')?.path).toBe(
      'zażółć.md',
    )
    expect(parsePorcelainEntry('?? "dir/say \\"hi\\"\\tnow.txt"')?.path).toBe(
      'dir/say "hi"\tnow.txt',
    )
  })

  it('keys renames by the destination path and keeps the source path', () => {
    expect(parsePorcelainEntry('R  plain.md -> "renamed file.md"')).toEqual({
      xy: 'R ',
      path: 'renamed file.md',
      origPath: 'plain.md',
    })
  })

  it('does not split a quoted rename source that contains the arrow', () => {
    expect(parsePorcelainEntry('R  "a -> b.txt" -> c.txt')).toEqual({
      xy: 'R ',
      path: 'c.txt',
      origPath: 'a -> b.txt',
    })
  })

  it('ignores lines too short to carry a path', () => {
    expect(parsePorcelainEntry('')).toBeNull()
    expect(parsePorcelainEntry(' M ')).toBeNull()
  })
})
