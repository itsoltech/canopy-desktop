import { describe, expect, it } from 'vitest'
import { formatDuration, formatWhen } from './format'

describe('formatDuration', () => {
  it('formats seconds, minutes and hours', () => {
    expect(formatDuration(45_000)).toBe('45 s')
    expect(formatDuration(73_000)).toBe('1 m 13 s')
    expect(formatDuration(60_000)).toBe('1 m 0 s')
    expect(formatDuration(2 * 3600_000 + 5 * 60_000)).toBe('2 h 5 m')
    expect(formatDuration(500)).toBe('0 s')
  })
})

describe('formatWhen', () => {
  const now = new Date(2026, 7, 1, 18, 30).getTime() // local 2026-08-01 18:30

  it('shows only the clock for same-day timestamps', () => {
    const ts = new Date(2026, 7, 1, 17, 23).getTime()
    expect(formatWhen(ts, now)).toBe('17:23')
  })

  it('adds the ISO date for older timestamps', () => {
    const ts = new Date(2026, 6, 30, 9, 5).getTime()
    expect(formatWhen(ts, now)).toBe('2026-07-30 09:05')
  })
})
