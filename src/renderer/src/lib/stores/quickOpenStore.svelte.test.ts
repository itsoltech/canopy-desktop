import { describe, expect, it, vi } from 'vitest'

const api = {
  quickOpenListFiles: vi.fn(),
}

vi.stubGlobal('window', { api })

import { ensureLoaded } from './quickOpenStore.svelte'

describe('quickOpenStore', () => {
  it('shares one listing request between concurrent loads of a worktree', async () => {
    let resolveListing: (files: string[]) => void = () => {}
    api.quickOpenListFiles.mockImplementation(
      () =>
        new Promise<string[]>((resolve) => {
          resolveListing = resolve
        }),
    )

    const loads = [ensureLoaded('/repo'), ensureLoaded('/repo'), ensureLoaded('/repo')]
    resolveListing(['src/a.ts'])

    expect(await Promise.all(loads)).toEqual([['src/a.ts'], ['src/a.ts'], ['src/a.ts']])
    expect(api.quickOpenListFiles).toHaveBeenCalledOnce()
  })
})
