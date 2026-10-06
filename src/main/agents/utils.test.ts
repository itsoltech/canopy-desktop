import { describe, expect, it } from 'vitest'
import { isAgentSessionId } from './utils'

describe('isAgentSessionId', () => {
  it('accepts the session id formats the agent CLIs emit', () => {
    expect(isAgentSessionId('2f1d6c3e-8b7a-4c2d-9e1f-0a1b2c3d4e5f')).toBe(true)
    expect(isAgentSessionId('ses_5f2c1a9e8ffeAbC123')).toBe(true)
  })

  it('rejects values a CLI would parse as an option or that are not ids', () => {
    expect(isAgentSessionId('--dangerously-skip-permissions')).toBe(false)
    expect(isAgentSessionId('-x')).toBe(false)
    expect(isAgentSessionId('')).toBe(false)
    expect(isAgentSessionId('abc def')).toBe(false)
    expect(isAgentSessionId('a'.repeat(129))).toBe(false)
    expect(isAgentSessionId(42)).toBe(false)
    expect(isAgentSessionId(undefined)).toBe(false)
  })
})
