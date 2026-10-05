import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  agentSessions,
  handleHookEvent,
  initAgentSession,
  removeAgentSession,
} from './agentState.svelte'

const PTY = 'pty-tool-status'

function hook(event: string, toolName?: string): Parameters<typeof handleHookEvent>[1] {
  return { agentType: 'claude', sessionId: 'agent-1', event, rawEventName: event, toolName }
}

describe('agent status around tool use', () => {
  beforeEach(() => {
    initAgentSession(PTY, 'claude')
    handleHookEvent(PTY, hook('PromptSubmit'))
  })

  afterEach(() => removeAgentSession(PTY))

  it('returns to thinking once an approved tool finishes', () => {
    handleHookEvent(PTY, hook('BeforeToolUse', 'Bash'))
    handleHookEvent(PTY, hook('PermissionRequest', 'Bash'))
    handleHookEvent(PTY, hook('AfterToolUse', 'Bash'))

    expect(agentSessions[PTY].status).toEqual({ type: 'thinking' })
  })

  it('returns to thinking when a tool call fails or is denied', () => {
    handleHookEvent(PTY, hook('PermissionRequest', 'Edit'))
    handleHookEvent(PTY, hook('AfterToolUseFailure', 'Edit'))

    expect(agentSessions[PTY].status).toEqual({ type: 'thinking' })
  })

  it('does not revive a session that already went idle', () => {
    handleHookEvent(PTY, hook('BeforeToolUse', 'Read'))
    handleHookEvent(PTY, hook('Idle'))
    handleHookEvent(PTY, hook('AfterToolUse', 'Read'))

    expect(agentSessions[PTY].status).toEqual({ type: 'idle' })
  })
})
