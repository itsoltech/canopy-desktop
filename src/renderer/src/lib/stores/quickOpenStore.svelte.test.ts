import { beforeEach, describe, expect, it, vi } from 'vitest'

const quickOpenListFiles = vi.fn<(worktreePath: string, force?: boolean) => Promise<string[]>>()
vi.stubGlobal('window', { api: { quickOpenListFiles } })

import { clearQuickOpenCache, ensureLoaded, forceReload, getFiles } from './quickOpenStore.svelte'

function deferred(): { promise: Promise<string[]>; resolve: (files: string[]) => void } {
  let resolve!: (files: string[]) => void
  const promise = new Promise<string[]>((r) => (resolve = r))
  return { promise, resolve }
}

describe('quick open file list loading', () => {
  beforeEach(() => {
    quickOpenListFiles.mockReset()
    clearQuickOpenCache('/repo')
  })

  it('shares one listing request between concurrent loads of the same worktree', async () => {
    // Terminal link detection asks for the list on every hovered line while it loads.
    const listing = deferred()
    quickOpenListFiles.mockReturnValueOnce(listing.promise)

    const loads = [ensureLoaded('/repo'), ensureLoaded('/repo'), ensureLoaded('/repo')]
    listing.resolve(['a.ts', 'b.ts'])

    expect(await Promise.all(loads)).toEqual([
      ['a.ts', 'b.ts'],
      ['a.ts', 'b.ts'],
      ['a.ts', 'b.ts'],
    ])
    expect(quickOpenListFiles).toHaveBeenCalledTimes(1)
    expect(getFiles('/repo')).toEqual(['a.ts', 'b.ts'])
  })

  it('still forces a fresh listing while a normal load is pending', async () => {
    const pending = deferred()
    quickOpenListFiles.mockReturnValueOnce(pending.promise)
    quickOpenListFiles.mockResolvedValueOnce(['fresh.ts'])

    const load = ensureLoaded('/repo')
    expect(await forceReload('/repo')).toEqual(['fresh.ts'])
    expect(quickOpenListFiles).toHaveBeenLastCalledWith('/repo', true)

    pending.resolve(['stale.ts'])
    await load
    expect(getFiles('/repo')).toEqual(['fresh.ts'])
  })
})
