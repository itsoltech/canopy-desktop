import { afterEach, describe, expect, it, vi } from 'vitest'
import { fileTree } from './fileTree.svelte'

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => (resolve = r))
  return { promise, resolve }
}

describe('fileTree store', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('drops a git status reply for the previous root after reset()', async () => {
    const status = deferred<{
      statuses: Record<string, string>
      changedDirs: string[]
      affectedPaths: string[]
    }>()
    vi.stubGlobal('window', {
      api: {
        fileTreeGetGitStatus: vi.fn(() => status.promise),
        fileTreeReadDir: vi.fn(async () => []),
      },
    })
    fileTree.reset('/work/a')
    const pending = fileTree.refreshGitStatus('/work/a')

    fileTree.reset('/work/b')
    status.resolve({ statuses: { 'src/app.ts': 'M' }, changedDirs: ['src'], affectedPaths: [] })
    await pending

    expect(fileTree.gitFileStatus.size).toBe(0)
    expect(fileTree.gitChangedDirs.size).toBe(0)
  })

  it('drops a directory listing for the previous root after reset()', async () => {
    const listing = deferred<Array<{ name: string; isDirectory: boolean; size: number }>>()
    vi.stubGlobal('window', { api: { fileTreeReadDir: vi.fn(() => listing.promise) } })
    fileTree.reset('/work/a')
    const pending = fileTree.expandDir('/work/a/src')

    fileTree.reset('/work/b')
    listing.resolve([{ name: 'app.ts', isDirectory: false, size: 1 }])
    await pending

    expect(fileTree.expandedDirs).toEqual({})
  })
})
