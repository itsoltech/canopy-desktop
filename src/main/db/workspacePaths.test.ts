import { describe, expect, it } from 'vitest'
import { comparableWorkspacePath, normalizeWorkspacePath } from './workspacePaths'

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

describe('comparableWorkspacePath', () => {
  it('folds case on win32 (case-insensitive filesystem)', () => {
    expect(comparableWorkspacePath('C:\\Source\\Repo', 'win32')).toBe('c:/source/repo')
  })

  it('maps case- and separator-divergent spellings of one directory to one key on win32', () => {
    expect(comparableWorkspacePath('C:\\Source\\Repo', 'win32')).toBe(
      comparableWorkspacePath('c:/source/repo', 'win32'),
    )
  })

  it('preserves case on case-sensitive platforms', () => {
    expect(comparableWorkspacePath('/home/User/Repo', 'linux')).toBe('/home/User/Repo')
  })

  it('still normalizes separators on non-win32 platforms', () => {
    expect(comparableWorkspacePath('C:\\Source\\Repo', 'darwin')).toBe('C:/Source/Repo')
  })
})
