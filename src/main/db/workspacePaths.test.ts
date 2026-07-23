import { describe, expect, it } from 'vitest'
import { comparableWorkspacePath, normalizeWorkspacePath } from './workspacePaths'

describe('normalizeWorkspacePath', () => {
  it('converts backslashes to forward slashes on win32', () => {
    expect(normalizeWorkspacePath('C:\\source\\GithubITSOL\\gakko', 'win32')).toBe(
      'C:/source/GithubITSOL/gakko',
    )
  })

  it('leaves forward-slash paths untouched on win32', () => {
    expect(normalizeWorkspacePath('C:/source/GithubITSOL/gakko', 'win32')).toBe(
      'C:/source/GithubITSOL/gakko',
    )
  })

  it('normalizes mixed-style paths on win32', () => {
    expect(normalizeWorkspacePath('C:/source\\GithubITSOL\\gakko', 'win32')).toBe(
      'C:/source/GithubITSOL/gakko',
    )
  })

  it('maps both styles of the same path to one canonical form on win32', () => {
    expect(normalizeWorkspacePath('C:\\source\\repo', 'win32')).toBe(
      normalizeWorkspacePath('C:/source/repo', 'win32'),
    )
  })

  it('normalizes UNC paths consistently on win32', () => {
    expect(normalizeWorkspacePath('\\\\server\\share\\repo', 'win32')).toBe('//server/share/repo')
  })

  it('preserves a literal backslash on POSIX (legal filename character)', () => {
    expect(normalizeWorkspacePath('/tmp/repo\\name', 'linux')).toBe('/tmp/repo\\name')
    expect(normalizeWorkspacePath('/tmp/repo\\name', 'darwin')).toBe('/tmp/repo\\name')
  })

  it('keeps backslash-divergent POSIX paths distinct', () => {
    expect(normalizeWorkspacePath('/tmp/repo\\name', 'linux')).not.toBe(
      normalizeWorkspacePath('/tmp/repo/name', 'linux'),
    )
  })

  it('handles plain POSIX paths as a no-op', () => {
    expect(normalizeWorkspacePath('/home/user/repo', 'linux')).toBe('/home/user/repo')
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

  it('preserves backslashes and case on POSIX platforms', () => {
    expect(comparableWorkspacePath('/tmp/Repo\\Name', 'linux')).toBe('/tmp/Repo\\Name')
    expect(comparableWorkspacePath('/tmp/Repo\\Name', 'darwin')).toBe('/tmp/Repo\\Name')
  })
})
