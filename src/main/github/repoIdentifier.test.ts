import { describe, expect, it } from 'vitest'
import { classifyRepoIdentifierLookupFailure } from './repoIdentifier'

describe('classifyRepoIdentifierLookupFailure', () => {
  it('reserves missing for an absent origin remote', () => {
    expect(
      classifyRepoIdentifierLookupFailure({
        _tag: 'GitCommandFailed',
        command: 'remote get-url',
        message: "fatal: No such remote 'origin'",
      }),
    ).toEqual({ status: 'missing' })
  })

  it('keeps other Git failures distinct from an absent remote', () => {
    expect(
      classifyRepoIdentifierLookupFailure({
        _tag: 'GitCommandFailed',
        command: 'remote get-url',
        message: 'fatal: unable to read repository metadata',
      }),
    ).toEqual({
      status: 'error',
      message: 'Git remote get-url failed: fatal: unable to read repository metadata',
    })
  })
})
