import { describe, expect, it } from 'vitest'
import { settingsJsonError } from './profileSettingsJson'

describe('settingsJsonError', () => {
  it('accepts an empty override and a JSON object', () => {
    expect(settingsJsonError(undefined)).toBeNull()
    expect(settingsJsonError('   ')).toBeNull()
    expect(settingsJsonError('{"language": "japanese"}')).toBeNull()
  })

  it('rejects text that is not valid JSON', () => {
    expect(settingsJsonError('{"language": "japanese",}')).toMatch(/not valid JSON/)
  })

  it.each(['[1, 2]', 'null', '"text"', '42'])('rejects %s, which is not a JSON object', (raw) => {
    expect(settingsJsonError(raw)).toMatch(/must be a JSON object/)
  })
})
