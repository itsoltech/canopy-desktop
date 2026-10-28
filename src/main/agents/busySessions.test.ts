import { describe, expect, it } from 'vitest'
import { claudeAdapter } from './adapters/claude'
import { codexAdapter } from './adapters/codex'
import { BusySessions } from './busySessions'
import type { AgentAdapter } from './types'

// Feeds hook events the way AgentSessionManager does, and returns what each `track` call said.
function feed(
  busy: BusySessions,
  adapter: AgentAdapter,
  rawNames: string[],
  ptySessionId = 'pty-1',
): boolean[] {
  return rawNames.map((rawName) =>
    busy.track(
      ptySessionId,
      adapter,
      rawName,
      adapter.normalizeEvent({ hook_event_name: rawName }).event,
    ),
  )
}

describe('BusySessions', () => {
  // Claude Code 2.1.299 compacts a parked conversation before its one-hour prompt cache expires:
  // PreCompact, PostCompact, then SessionStart with source "compact", and no Stop.
  it('settles a Claude compaction that starts while the pane is idle', () => {
    const busy = new BusySessions()
    feed(busy, claudeAdapter, ['SessionStart', 'UserPromptSubmit', 'Stop'])

    expect(feed(busy, claudeAdapter, ['PreCompact'])).toEqual([true])
    expect(busy.has('pty-1')).toBe(true)
    expect(feed(busy, claudeAdapter, ['PostCompact', 'SessionStart'])).toEqual([true, false])
    expect(busy.has('pty-1')).toBe(false)
  })

  it('leaves a compaction inside a turn to the Stop that ends the turn', () => {
    const busy = new BusySessions()
    const turn = ['UserPromptSubmit', 'PreCompact', 'PostCompact', 'SessionStart', 'PreToolUse']

    expect(feed(busy, claudeAdapter, turn)).toEqual([false, false, false, false, false])
    expect(busy.has('pty-1')).toBe(true)
    feed(busy, claudeAdapter, ['Stop'])
    expect(busy.has('pty-1')).toBe(false)
  })

  it('does not let an idle compaction that never ended settle a later turn', () => {
    const busy = new BusySessions()
    feed(busy, claudeAdapter, ['PreCompact', 'UserPromptSubmit', 'PreCompact'])

    expect(feed(busy, claudeAdapter, ['PostCompact'])).toEqual([false])
    expect(busy.has('pty-1')).toBe(true)
  })

  it('keeps Codex settling a compaction inside a turn on PostCompact', () => {
    const busy = new BusySessions()

    expect(feed(busy, codexAdapter, ['UserPromptSubmit', 'PreCompact', 'PostCompact'])).toEqual([
      false,
      false,
      false,
    ])
    expect(busy.has('pty-1')).toBe(false)
  })

  it('carries an open idle compaction across a rekey', () => {
    const busy = new BusySessions()
    feed(busy, claudeAdapter, ['PreCompact'], 'temp')
    busy.rekey('temp', 'pty-1')

    expect(busy.has('temp')).toBe(false)
    expect(busy.has('pty-1')).toBe(true)
    expect(feed(busy, claudeAdapter, ['PostCompact'])).toEqual([true])
    expect(busy.has('pty-1')).toBe(false)
  })
})
