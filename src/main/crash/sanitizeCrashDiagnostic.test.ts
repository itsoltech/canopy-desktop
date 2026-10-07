import { describe, expect, it } from 'vitest'
import { sanitizeDiagnosticText } from './sanitizeCrashDiagnostic'

describe('sanitizeDiagnosticText user paths', () => {
  it('redacts a Windows profile folder that contains a space', () => {
    const out = sanitizeDiagnosticText(
      'at run (C:\\Users\\John Smith\\AppData\\Local\\Programs\\Canopy\\index.js:12:3)',
    )
    expect(out).not.toContain('John')
    expect(out).toContain('~/AppData\\Local')
  })

  it('redacts forward-slash Windows paths, as in file URLs', () => {
    const out = sanitizeDiagnosticText('at file:///C:/Users/John%20Smith/app/main.js:1:1')
    expect(out).not.toContain('John')
    expect(out).toContain('~/app/main.js')
  })

  it('keeps redacting single-word profile folders', () => {
    expect(sanitizeDiagnosticText('C:\\Users\\jdoe\\x.js')).toBe('~/x.js')
    expect(sanitizeDiagnosticText('open /home/jdoe/x failed')).toBe('open ~/x failed')
    expect(sanitizeDiagnosticText('/Users/jdoe/Library/x')).toBe('~/Library/x')
  })

  it('does not swallow prose after a POSIX home directory', () => {
    expect(sanitizeDiagnosticText('/home/jdoe and /tmp/x')).toBe('/home/jdoe and /tmp/x')
  })
})
