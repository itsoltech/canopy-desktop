import { describe, expect, it } from 'vitest'
import { isValidResumeSessionId } from './utils'

describe('isValidResumeSessionId', () => {
  it('accepts the session id shapes the agent CLIs actually emit', () => {
    expect(isValidResumeSessionId('550e8400-e29b-41d4-a716-446655440000')).toBe(true)
    expect(isValidResumeSessionId('01JQ8XZ4K7NBM3VH2P')).toBe(true)
    expect(isValidResumeSessionId('session_abc123')).toBe(true)
    expect(isValidResumeSessionId('ses.2026-07-27.1')).toBe(true)
    expect(isValidResumeSessionId('a')).toBe(true)
  })

  it('rejects ids that would be parsed as a flag by the agent CLI', () => {
    expect(isValidResumeSessionId('--dangerously-skip-permissions')).toBe(false)
    expect(isValidResumeSessionId('-h')).toBe(false)
    expect(isValidResumeSessionId('--')).toBe(false)
  })

  it('rejects shell/path metacharacters and whitespace', () => {
    expect(isValidResumeSessionId('abc def')).toBe(false)
    expect(isValidResumeSessionId('abc;rm -rf /')).toBe(false)
    expect(isValidResumeSessionId('../../etc/passwd')).toBe(false)
    expect(isValidResumeSessionId('abc$(whoami)')).toBe(false)
    expect(isValidResumeSessionId('abc\nid')).toBe(false)
  })

  it('rejects empty and over-long ids', () => {
    expect(isValidResumeSessionId('')).toBe(false)
    expect(isValidResumeSessionId('a'.repeat(128))).toBe(true)
    expect(isValidResumeSessionId('a'.repeat(129))).toBe(false)
  })
})
