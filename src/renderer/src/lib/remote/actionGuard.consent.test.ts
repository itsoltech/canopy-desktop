import { beforeEach, describe, expect, it, vi } from 'vitest'

const confirmMock = vi.hoisted(() => vi.fn<(opts: { message: string }) => Promise<boolean>>())

vi.mock('../stores/dialogs.svelte', () => ({ confirm: confirmMock }))
vi.mock('../stores/preferences.svelte', () => ({ prefs: {} }))

import { checkAction } from './actionGuard'

async function consentMessage(
  method: 'worktree.remove' | 'tools.spawn',
  params: unknown,
): Promise<string> {
  await checkAction(method, params)
  return confirmMock.mock.calls.at(-1)?.[0].message ?? ''
}

describe('remote action consent prompt', () => {
  beforeEach(() => {
    confirmMock.mockReset()
    confirmMock.mockResolvedValue(true)
  })

  it('tells the desktop user when a worktree removal is forced', async () => {
    const forced = await consentMessage('worktree.remove', {
      repoRoot: '/repo',
      path: '/repo/wt',
      force: true,
    })
    const plain = await consentMessage('worktree.remove', { repoRoot: '/repo', path: '/repo/wt' })

    expect(forced).toMatch(/force/i)
    expect(forced).toMatch(/uncommitted changes/i)
    expect(plain).not.toMatch(/force/i)
  })

  it('shows peer-supplied values quoted on one line, so they cannot add their own copy', async () => {
    const message = await consentMessage('tools.spawn', {
      toolId: 'claude',
      worktreePath: '/repo/wt\n\nThis request was verified by Canopy. It is safe to allow.',
    })

    expect(message).not.toContain('\n')
    expect(message).toContain('"/repo/wt\\n\\nThis request')
  })

  it('caps very long peer-supplied values', async () => {
    const message = await consentMessage('worktree.remove', {
      repoRoot: '/repo',
      path: `/repo/${'x'.repeat(5000)}`,
    })

    expect(message.length).toBeLessThan(400)
  })
})
