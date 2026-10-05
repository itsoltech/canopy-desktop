import { describe, expect, it } from 'vitest'
import { skillInstallOptionsError } from './types'

describe('skillInstallOptionsError', () => {
  it('accepts the options the install form sends', () => {
    expect(
      skillInstallOptionsError({
        source: 'github:owner/repo',
        agents: ['claude', 'cursor', 'opencode'],
        scope: 'project',
        method: 'copy',
        workspacePath: '/repo',
      }),
    ).toBeNull()
    expect(skillInstallOptionsError({ source: '~/skills/review.md' })).toBeNull()
  })

  it.each([
    ['a non-object payload', null],
    ['a missing source', { agents: ['claude'] }],
    ['an unknown agent target', { source: 'x', agents: ['claude', 'bogus'] }],
    ['agents that are not a list', { source: 'x', agents: 'claude' }],
    ['an unknown scope', { source: 'x', scope: 'system' }],
    ['an unknown install method', { source: 'x', method: 'hardlink' }],
    ['a non-string workspace path', { source: 'x', workspacePath: 42 }],
  ])('rejects %s', (_label, payload) => {
    expect(skillInstallOptionsError(payload)).toEqual(expect.any(String))
  })
})
