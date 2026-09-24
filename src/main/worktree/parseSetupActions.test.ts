import { describe, expect, it } from 'vitest'
import { parseWorktreeSetupActions } from './parseSetupActions'

describe('parseWorktreeSetupActions', () => {
  it('accepts a well-formed command action', () => {
    const json = JSON.stringify([{ type: 'command', command: 'npm install', label: 'Install' }])
    expect(parseWorktreeSetupActions(json)).toEqual([
      { type: 'command', command: 'npm install', label: 'Install' },
    ])
  })

  it('accepts a well-formed copy action with an optional dest', () => {
    const json = JSON.stringify([{ type: 'copy', source: '.env' }])
    expect(parseWorktreeSetupActions(json)).toEqual([{ type: 'copy', source: '.env' }])
  })

  it('returns null for malformed JSON', () => {
    expect(parseWorktreeSetupActions('{not json')).toBeNull()
  })

  it('returns null when the payload is not an array', () => {
    expect(parseWorktreeSetupActions(JSON.stringify({ type: 'command', command: 'ls' }))).toBeNull()
  })

  it('rejects an unknown action type rather than passing it through', () => {
    expect(parseWorktreeSetupActions(JSON.stringify([{ type: 'exec', command: 'ls' }]))).toBeNull()
  })

  it('rejects a command action whose command is not a string', () => {
    expect(parseWorktreeSetupActions(JSON.stringify([{ type: 'command' }]))).toBeNull()
    expect(parseWorktreeSetupActions(JSON.stringify([{ type: 'command', command: 7 }]))).toBeNull()
  })

  it('rejects a copy action whose source is not a string', () => {
    expect(parseWorktreeSetupActions(JSON.stringify([{ type: 'copy' }]))).toBeNull()
  })

  it('rejects a non-string label or dest', () => {
    const badLabel = JSON.stringify([{ type: 'command', command: 'ls', label: 3 }])
    expect(parseWorktreeSetupActions(badLabel)).toBeNull()
    const badDest = JSON.stringify([{ type: 'copy', source: '.env', dest: [] }])
    expect(parseWorktreeSetupActions(badDest)).toBeNull()
  })

  it('rejects a null or primitive array entry', () => {
    expect(parseWorktreeSetupActions(JSON.stringify([null]))).toBeNull()
    expect(parseWorktreeSetupActions(JSON.stringify(['npm install']))).toBeNull()
  })

  it('accepts an empty action list', () => {
    expect(parseWorktreeSetupActions('[]')).toEqual([])
  })
})
