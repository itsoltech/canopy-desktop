import { afterAll, beforeAll, describe, it, expect, vi } from 'vitest'
import { formatDate, formatDateTime, parseSqliteUtc } from './formatDate'

describe('formatDate', () => {
  it('formats an ISO timestamp as YYYY-MM-DD', () => {
    // Local-time date: use a midday timestamp so no timezone flips the day.
    expect(formatDate('2026-03-21T12:00:00')).toBe('2026-03-21')
  })

  it('pads single-digit months and days', () => {
    expect(formatDate('2026-01-05T12:00:00')).toBe('2026-01-05')
  })

  it('returns empty string for missing or invalid input', () => {
    expect(formatDate(undefined)).toBe('')
    expect(formatDate(null)).toBe('')
    expect(formatDate('')).toBe('')
    expect(formatDate('not-a-date')).toBe('')
  })
})

describe('formatDateTime', () => {
  it('formats as YYYY-MM-DD HH:mm with zero-padding', () => {
    expect(formatDateTime('2026-03-21T09:05:00')).toBe('2026-03-21 09:05')
  })

  it('returns empty string for invalid input', () => {
    expect(formatDateTime('nope')).toBe('')
    expect(formatDateTime(undefined)).toBe('')
  })
})

describe('parseSqliteUtc', () => {
  beforeAll(() => {
    // Any non-UTC zone exposes reading the zone-less SQLite text as local time.
    vi.stubEnv('TZ', 'Europe/Warsaw')
  })
  afterAll(() => {
    vi.unstubAllEnvs()
  })

  it("reads SQLite datetime('now') text as UTC", () => {
    expect(parseSqliteUtc('2026-10-02 10:00:00')).toBe(Date.UTC(2026, 9, 2, 10, 0, 0))
  })

  it('passes ISO timestamps with a zone through unchanged', () => {
    expect(parseSqliteUtc('2026-10-02T10:00:00.000Z')).toBe(Date.UTC(2026, 9, 2, 10, 0, 0))
  })
})
