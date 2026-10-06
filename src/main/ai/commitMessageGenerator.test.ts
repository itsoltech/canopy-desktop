import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { PreferencesStore } from '../db/PreferencesStore'

const query = vi.fn()
vi.mock('@anthropic-ai/claude-agent-sdk', () => ({
  query: (...args: unknown[]) => query(...args),
}))

import { generateCommitMessage } from './commitMessageGenerator'

const prefs = { get: () => null } as unknown as PreferencesStore

// The SDK's message stream for one CLI run: yields `messages`, then throws `error` if given.
async function* run(
  messages: Record<string, unknown>[],
  error?: Error,
): AsyncGenerator<Record<string, unknown>> {
  yield* messages
  if (error) throw error
}

function result(fields: Record<string, unknown>): Record<string, unknown> {
  return { type: 'result', subtype: 'success', is_error: false, result: '', ...fields }
}

describe('generateCommitMessage', () => {
  beforeEach(() => {
    query.mockReset()
  })

  it('returns the subject and body of the structured output', async () => {
    query.mockReturnValue(
      run([result({ structured_output: { subject: 'feat: add x', body: 'Details.' } })]),
    )
    await expect(generateCommitMessage('diff', prefs)).resolves.toBe('feat: add x\n\nDetails.')
  })

  // Claude Code before 2.1.290 marks a turn whose last request fails after the structured
  // output was delivered as `is_error` on a `success` result and exits non-zero, so the
  // SDK throws "Claude Code returned an error result" after yielding that result.
  it('keeps a delivered structured output when the stream fails after it', async () => {
    query.mockReturnValue(
      run(
        [
          result({
            is_error: true,
            result: 'API Error: Connection error.',
            structured_output: { subject: 'fix: keep the delivered message', body: '' },
          }),
        ],
        new Error('Claude Code returned an error result: API Error: Connection error.'),
      ),
    )
    await expect(generateCommitMessage('diff', prefs)).resolves.toBe(
      'fix: keep the delivered message',
    )
  })

  it('returns null when the stream fails before any structured output', async () => {
    query.mockReturnValue(run([], new Error('Claude Code process exited with code 1')))
    await expect(generateCommitMessage('diff', prefs)).resolves.toBeNull()
  })
})
