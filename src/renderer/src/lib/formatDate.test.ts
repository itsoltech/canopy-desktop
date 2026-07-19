import { describe, it, expect } from 'vitest'
import { formatDate, formatDateTime } from './formatDate'

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
