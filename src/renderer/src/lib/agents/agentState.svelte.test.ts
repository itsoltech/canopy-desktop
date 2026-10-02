import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  agentSessions,
  handleHookEvent,
  initAgentSession,
  removeAgentSession,
} from './agentState.svelte'

describe('agent notifications', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    removeAgentSession('pty-1')
  })

  it('gives notifications delivered in the same millisecond distinct ids', () => {
    vi.spyOn(Date, 'now').mockReturnValue(1_800_000_000_000)
    initAgentSession('pty-1', 'claude')

    const notification = (message: string): Parameters<typeof handleHookEvent>[1] => ({
      agentType: 'claude',
      sessionId: 'agent-1',
      event: 'Notification',
      rawEventName: 'Notification',
      message,
    })
    handleHookEvent('pty-1', notification('Needs input'))
    handleHookEvent('pty-1', notification('Still waiting'))

    const ids = agentSessions['pty-1'].notifications.map((n) => n.id)
    expect(new Set(ids).size).toBe(2)
  })
})
