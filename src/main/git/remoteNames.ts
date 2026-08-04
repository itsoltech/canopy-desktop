export function hasRemoteName(raw: string, remote: string): boolean {
  return raw.split(/\r?\n/).some((name) => name.trim() === remote)
}
