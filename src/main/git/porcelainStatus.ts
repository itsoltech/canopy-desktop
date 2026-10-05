export interface PorcelainEntry {
  /** The two-letter `XY` status code. */
  xy: string
  /** Working-tree path (for renames/copies: the destination). */
  path: string
  /** Source path of a rename/copy, otherwise null. */
  origPath: string | null
}

const C_ESCAPES: Record<string, string> = {
  a: '\x07',
  b: '\b',
  t: '\t',
  n: '\n',
  v: '\v',
  f: '\f',
  r: '\r',
  '"': '"',
  '\\': '\\',
}

/**
 * `git status --porcelain` (v1) wraps a path in C-style quotes when it contains whitespace,
 * quotes, control characters or — unless core.quotePath=false — non-ASCII bytes, which are
 * written as octal escapes of their UTF-8 encoding.
 */
function unquoteGitPath(raw: string): string {
  if (raw.length < 2 || raw[0] !== '"' || raw[raw.length - 1] !== '"') return raw
  const body = raw.slice(1, -1)
  const parts: Buffer[] = []
  let literalStart = 0
  for (const m of body.matchAll(/\\([0-7]{3}|[abtnvfr"\\])/g)) {
    parts.push(Buffer.from(body.slice(literalStart, m.index), 'utf8'))
    const escape = m[1]
    parts.push(
      escape.length === 3
        ? Buffer.from([parseInt(escape, 8)])
        : Buffer.from(C_ESCAPES[escape], 'utf8'),
    )
    literalStart = m.index + m[0].length
  }
  parts.push(Buffer.from(body.slice(literalStart), 'utf8'))
  return Buffer.concat(parts).toString('utf8')
}

/** End index (exclusive) of the path token starting at `start`, honouring C-style quoting. */
function tokenEnd(text: string, start: number): number {
  if (text[start] !== '"') {
    const space = text.indexOf(' ', start)
    return space === -1 ? text.length : space
  }
  let i = start + 1
  while (i < text.length && text[i] !== '"') i += text[i] === '\\' ? 2 : 1
  return Math.min(i + 1, text.length)
}

/** Parses one line of `git status --porcelain` (v1) output. */
export function parsePorcelainEntry(line: string): PorcelainEntry | null {
  if (line.length < 4) return null
  const xy = line.slice(0, 2)
  const rest = line.slice(3)
  if (xy.includes('R') || xy.includes('C')) {
    const end = tokenEnd(rest, 0)
    if (rest.startsWith(' -> ', end)) {
      return {
        xy,
        path: unquoteGitPath(rest.slice(end + 4)),
        origPath: unquoteGitPath(rest.slice(0, end)),
      }
    }
  }
  return { xy, path: unquoteGitPath(rest), origPath: null }
}
