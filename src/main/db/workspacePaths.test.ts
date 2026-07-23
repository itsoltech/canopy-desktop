import { describe, expect, it } from 'vitest'
import { normalizeWorkspacePath } from './workspacePaths'

describe('normalizeWorkspacePath', () => {
  it('converts backslashes to forward slashes', () => {
    expect(normalizeWorkspacePath('C:\\source\\GithubITSOL\\gakko')).toBe(
      'C:/source/GithubITSOL/gakko',
    )
  })

  it('leaves forward-slash paths untouched', () => {
    expect(normalizeWorkspacePath('C:/source/GithubITSOL/gakko')).toBe(
      'C:/source/GithubITSOL/gakko',
    )
  })

  it('normalizes mixed-style paths', () => {
    expect(normalizeWorkspacePath('C:/source\\GithubITSOL\\gakko')).toBe(
      'C:/source/GithubITSOL/gakko',
    )
  })

  it('maps both styles of the same path to one canonical form', () => {
    expect(normalizeWorkspacePath('C:\\source\\repo')).toBe(
      normalizeWorkspacePath('C:/source/repo'),
    )
  })

  it('normalizes UNC paths consistently', () => {
    expect(normalizeWorkspacePath('\\\\server\\share\\repo')).toBe('//server/share/repo')
  })

  it('handles POSIX paths as a no-op', () => {
    expect(normalizeWorkspacePath('/home/user/repo')).toBe('/home/user/repo')
  })
})
