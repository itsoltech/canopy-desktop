export interface PathMatch {
  start: number
  end: number
  raw: string
  absolutePath: string
  line?: number
  column?: number
}

const PATH_REGEX =
  /(\/[^\s:()[\]{}'"`<>|]+|\.{1,2}\/[^\s:()[\]{}'"`<>|]+|[\w\-_.]+(?:\/[^\s:()[\]{}'"`<>|]+)+)(?::(\d+))?(?::(\d+))?/g

function joinPath(cwd: string, relative: string): string {
  if (relative.startsWith('/')) return relative
  const cwdClean = cwd.endsWith('/') ? cwd.slice(0, -1) : cwd
  if (relative.startsWith('./')) return `${cwdClean}/${relative.slice(2)}`
  if (relative.startsWith('../')) {
    const parts = cwdClean.split('/')
    let rel = relative
    while (rel.startsWith('../')) {
      parts.pop()
      rel = rel.slice(3)
    }
    return `${parts.join('/')}/${rel}`
  }
  return `${cwdClean}/${relative}`
}

export function detectPathsInText(
  text: string,
  cwd: string,
  knownFiles: ReadonlySet<string>,
): PathMatch[] {
  // Without a known-file set, we'd need a sync existence check per candidate,
  // which isn't available from the renderer. Callers must pre-load the
  // workspace file list (see quickOpenStore.ensureLoaded) and pass it here.
  if (knownFiles.size === 0) return []

  const cwdPrefix = cwd.endsWith('/') ? cwd : cwd + '/'
  const matches: PathMatch[] = []

  for (const m of text.matchAll(PATH_REGEX)) {
    if (m.index === undefined) continue
    const raw = m[1]
    const line = m[2] ? parseInt(m[2], 10) : undefined
    const column = m[3] ? parseInt(m[3], 10) : undefined

    if (raw.length < 3) continue
    if (!raw.includes('/')) continue
    if (/^https?:\/\//.test(raw)) continue

    const absolutePath = joinPath(cwd, raw)
    // Resolve to a workspace-relative path; reject anything outside cwd.
    let relative: string
    if (absolutePath.startsWith(cwdPrefix)) {
      relative = absolutePath.slice(cwdPrefix.length)
    } else if (absolutePath === cwd) {
      continue
    } else {
      continue
    }

    // Only accept real files tracked in the workspace.
    if (!knownFiles.has(relative)) continue

    const fullMatch = m[0]
    matches.push({
      start: m.index,
      end: m.index + fullMatch.length,
      raw: fullMatch,
      absolutePath,
      line,
      column,
    })
  }
  return matches
}
