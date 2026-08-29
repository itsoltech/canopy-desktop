import { describe, expect, it } from 'vitest'
import { UNTRACKED_MAX_FILES, splitUntrackedForDiff } from './untrackedDiff'

function fileList(count: number): string[] {
  return Array.from({ length: count }, (_, i) => `file-${i}.ts`)
}

describe('splitUntrackedForDiff', () => {
  it('reads every file when the list is under the cap', () => {
    const files = fileList(10)
    expect(splitUntrackedForDiff(files)).toEqual({ read: files, listOnly: [] })
  })

  it('reads every file when the list is exactly at the cap', () => {
    const files = fileList(UNTRACKED_MAX_FILES)
    const { read, listOnly } = splitUntrackedForDiff(files)
    expect(read).toHaveLength(UNTRACKED_MAX_FILES)
    expect(listOnly).toEqual([])
  })

  it('caps the files it reads once the list exceeds the cap', () => {
    const { read, listOnly } = splitUntrackedForDiff(fileList(UNTRACKED_MAX_FILES + 50))
    expect(read).toHaveLength(UNTRACKED_MAX_FILES)
    expect(listOnly).toHaveLength(50)
  })

  it('still accounts for every file so none disappear from the changes list', () => {
    const files = fileList(UNTRACKED_MAX_FILES + 7)
    const { read, listOnly } = splitUntrackedForDiff(files)
    expect([...read, ...listOnly]).toEqual(files)
  })

  it('handles an empty list', () => {
    expect(splitUntrackedForDiff([])).toEqual({ read: [], listOnly: [] })
  })
})
