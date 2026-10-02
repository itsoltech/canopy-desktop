import { describe, expect, it } from 'vitest'
import { detectPathsInText } from './linkify'

const cwd = '/work/repo'
const known = new Set(['src/app.ts'])

describe('detectPathsInText', () => {
  it('links a path followed by sentence punctuation', () => {
    expect(detectPathsInText('Edited src/app.ts, done', cwd, known)).toMatchObject([
      { start: 7, end: 17, raw: 'src/app.ts', absolutePath: '/work/repo/src/app.ts' },
    ])
    expect(detectPathsInText('See src/app.ts.', cwd, known)).toMatchObject([{ raw: 'src/app.ts' }])
  })

  it('keeps line and column suffixes', () => {
    expect(detectPathsInText('at src/app.ts:12:4, then', cwd, known)).toMatchObject([
      { raw: 'src/app.ts:12:4', line: 12, column: 4 },
    ])
  })

  it('ignores paths that are not known workspace files', () => {
    expect(detectPathsInText('Edited src/other.ts, done', cwd, known)).toEqual([])
  })
})
