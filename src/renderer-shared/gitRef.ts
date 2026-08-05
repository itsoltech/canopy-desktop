/**
 * Provider-neutral safe Git ref contract for renderer/IPC input. Besides Git's ref-name rules,
 * URL path/query metacharacters are rejected because renderer-supplied branch names are inserted
 * into authenticated provider API paths, including `gh api` DELETE requests; validation is safer
 * than encoding while preserving legitimate `/` path segments.
 */
export function isSafeGitRefName(value: unknown): value is string {
  if (typeof value !== 'string' || value === '' || value.length > 255 || value !== value.trim()) {
    return false
  }
  if (
    value === '@' ||
    value.startsWith('-') ||
    value.startsWith('/') ||
    value.endsWith('/') ||
    value.endsWith('.') ||
    value.includes('//') ||
    value.includes('..') ||
    value.includes('@{')
  ) {
    return false
  }
  if (
    [...value].some((char) => {
      const code = char.charCodeAt(0)
      return code <= 0x20 || code === 0x7f || '~^:?*[\\#%'.includes(char)
    })
  ) {
    return false
  }
  return !value.split('/').some((part) => part.startsWith('.') || part.endsWith('.lock'))
}
