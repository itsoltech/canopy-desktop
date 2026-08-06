import { describe, expect, it } from 'vitest'
import { ipcErrorMessage, isCiAuthFailure } from './errors'

describe('ipcErrorMessage', () => {
  it('drops the Electron transport wrapper the user cannot act on', () => {
    // Verbatim from the jobs-history window before this existed.
    const raw = new Error(
      "Error invoking remote method 'ci:activity': Error: TeamCity API error 401: Could not authenticate with provided token",
    )
    expect(ipcErrorMessage(raw)).toBe(
      'TeamCity API error 401: Could not authenticate with provided token',
    )
  })

  it('strips a class prefix repeated by nested rethrows', () => {
    expect(ipcErrorMessage(new Error('Error: TypeError: boom'))).toBe('boom')
  })

  it('leaves a plain main-process message alone', () => {
    expect(ipcErrorMessage(new Error('TeamCity API error 500: upstream down'))).toBe(
      'TeamCity API error 500: upstream down',
    )
  })

  it('falls back rather than showing an empty box', () => {
    expect(ipcErrorMessage(new Error("Error invoking remote method 'ci:activity': Error: "))).toBe(
      'Request failed',
    )
    expect(ipcErrorMessage(undefined, 'Could not load history')).toBe('Could not load history')
  })
})

describe('isCiAuthFailure', () => {
  it('recognises a rejected token, which is the one failure with an action attached', () => {
    expect(isCiAuthFailure('TeamCity API error 401: Could not authenticate with provided token')) //
      .toBe(true)
    expect(isCiAuthFailure('GitHub API error 403: Forbidden')).toBe(true)
    expect(isCiAuthFailure('Unauthorized')).toBe(true)
  })

  it('does not claim an auth problem for unrelated faults', () => {
    expect(isCiAuthFailure('TeamCity API error 500: upstream down')).toBe(false)
    expect(isCiAuthFailure('fetch failed')).toBe(false)
    expect(isCiAuthFailure('')).toBe(false)
    expect(isCiAuthFailure(undefined)).toBe(false)
  })

  it('does not fire on a number that merely contains 401', () => {
    // Build numbers reach these strings; \b keeps #14012 from reading as a 401.
    expect(isCiAuthFailure('Build #14012 failed')).toBe(false)
  })
})
