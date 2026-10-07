import { describe, expect, it } from 'vitest'
import { MAX_DEVICE_NAME_LENGTH, sanitizeDeviceName } from './deviceName'

describe('sanitizeDeviceName', () => {
  it('keeps an ordinary name', () => {
    expect(sanitizeDeviceName("Anna's iPhone")).toBe("Anna's iPhone")
  })

  it('caps an oversized name sent in the pair frame', () => {
    const name = sanitizeDeviceName('x'.repeat(200_000))
    expect(name).not.toBeNull()
    expect(Array.from(name!).length).toBeLessThanOrEqual(MAX_DEVICE_NAME_LENGTH)
  })

  it('removes control and bidi override characters', () => {
    expect(sanitizeDeviceName('Pho\u202ene\n\u0007Evil')).toBe('Phone Evil')
  })

  it('falls back when nothing printable is left', () => {
    expect(sanitizeDeviceName('\u0000\u0001  ')).toBeNull()
    expect(sanitizeDeviceName(42)).toBeNull()
  })
})
