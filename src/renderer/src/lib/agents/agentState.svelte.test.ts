import { beforeEach, describe, expect, it } from 'vitest'
import {
  agentSessions,
  handleStatusUpdate,
  initAgentSession,
  removeAgentSession,
} from './agentState.svelte'

const PTY = 'pty-status'

// The Claude adapter forwards the status line's `rate_limits` whole as
// `extra.rateLimits`. The spend_limit shape is the 2.1.284 build's own
// status-line schema.
describe('handleStatusUpdate — gateway spend limit', () => {
  beforeEach(() => {
    removeAgentSession(PTY)
    initAgentSession(PTY, 'claude')
  })

  it('flattens the spend limit with its dollar amounts and period', () => {
    handleStatusUpdate(PTY, {
      extra: {
        rateLimits: {
          spend_limit: {
            used_percentage: 54.28,
            resets_at: 1_790_000_000,
            used_usd: 271.4,
            limit_usd: 500,
            period: 'monthly',
          },
        },
      },
    })

    expect(agentSessions[PTY].extra).toMatchObject({
      rateLimitSpend: 54.28,
      rateLimitSpendResetsAt: 1_790_000_000_000,
      rateLimitSpendUsedUsd: 271.4,
      rateLimitSpendLimitUsd: 500,
      rateLimitSpendPeriod: 'monthly',
    })
  })

  it('drops dollar amounts a later update no longer carries', () => {
    // The CLI attaches them only from a USD meter that is not dated to a
    // different window, so an earlier figure would misstate this one.
    handleStatusUpdate(PTY, {
      extra: {
        rateLimits: {
          spend_limit: {
            used_percentage: 10,
            resets_at: 1_790_000_000,
            used_usd: 50,
            limit_usd: 500,
          },
        },
      },
    })
    handleStatusUpdate(PTY, {
      extra: { rateLimits: { spend_limit: { used_percentage: 12, resets_at: 1_790_000_000 } } },
    })

    const extra = agentSessions[PTY].extra
    expect(extra.rateLimitSpend).toBe(12)
    expect(extra.rateLimitSpendUsedUsd).toBeUndefined()
    expect(extra.rateLimitSpendLimitUsd).toBeUndefined()
  })
})
