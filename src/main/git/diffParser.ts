import type { ParsedDiff, DiffFile, DiffHunk, DiffChange } from './types'

export function parseDiff(raw: string): ParsedDiff {
  if (!raw.trim()) return { files: [] }

  const files: DiffFile[] = []
  const lines = raw.split('\n')
  let i = 0

  while (i < lines.length) {
    // Look for "diff --git" header
    if (!lines[i].startsWith('diff --git ')) {
      i++
      continue
    }

    const file = parseFile(lines, i)
    files.push(file.result)
    i = file.nextIndex
  }

  return { files }
}

function parseFile(lines: string[], start: number): { result: DiffFile; nextIndex: number } {
  let i = start
  const diffLine = lines[i]

  // Extract paths from "diff --git a/path b/path"
  const pathMatch = diffLine.match(/^diff --git a\/(.+) b\/(.+)$/)
  const bPath = pathMatch ? pathMatch[2] : ''

  i++

  let status: DiffFile['status'] = 'modified'
  let oldPath: string | undefined
  let isBinary = false

  // Parse extended headers
  while (i < lines.length && !lines[i].startsWith('diff --git ')) {
    const line = lines[i]

    if (line.startsWith('new file mode')) {
      status = 'added'
    } else if (line.startsWith('deleted file mode')) {
      status = 'deleted'
    } else if (line.startsWith('rename from ')) {
      status = 'renamed'
      oldPath = line.slice('rename from '.length)
    } else if (line.startsWith('Binary files')) {
      isBinary = true
    } else if (line.startsWith('@@') || line.startsWith('--- ')) {
      break
    }

    i++
  }

  const hunks: DiffHunk[] = []
  let additions = 0
  let deletions = 0

  if (!isBinary) {
    // Skip "--- a/file" and "+++ b/file"
    if (i < lines.length && lines[i].startsWith('--- ')) i++
    if (i < lines.length && lines[i].startsWith('+++ ')) i++

    // Parse hunks
    while (i < lines.length && !lines[i].startsWith('diff --git ')) {
      if (lines[i].startsWith('@@')) {
        const hunk = parseHunk(lines, i)
        hunks.push(hunk.result)
        additions += hunk.additions
        deletions += hunk.deletions
        i = hunk.nextIndex
      } else {
        i++
      }
    }
  }

  const result: DiffFile = {
    path: bPath,
    status,
    hunks,
    additions,
    deletions,
  }
  if (oldPath) result.oldPath = oldPath

  return { result, nextIndex: i }
}

function parseHunk(
  lines: string[],
  start: number,
): { result: DiffHunk; nextIndex: number; additions: number; deletions: number } {
  const header = lines[start]
  const match = header.match(/^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/)

  const oldStart = match ? parseInt(match[1], 10) : 0
  const oldLines = match ? parseInt(match[2] ?? '1', 10) : 0
  const newStart = match ? parseInt(match[3], 10) : 0
  const newLines = match ? parseInt(match[4] ?? '1', 10) : 0

  const changes: DiffChange[] = []
  let additions = 0
  let deletions = 0
  let oldLine = oldStart
  let newLine = newStart
  let i = start + 1

  while (i < lines.length) {
    const line = lines[i]

    if (line.startsWith('diff --git ') || line.startsWith('@@')) {
      break
    }

    if (line.startsWith('+')) {
      changes.push({ type: 'add', content: line.slice(1), newLine })
      newLine++
      additions++
    } else if (line.startsWith('-')) {
      changes.push({ type: 'delete', content: line.slice(1), oldLine })
      oldLine++
      deletions++
    } else if (line.startsWith(' ') || line === '') {
      // Context line (or empty trailing line within a hunk)
      const isTrailingEmpty = line === '' && i === lines.length - 1
      if (!isTrailingEmpty) {
        changes.push({ type: 'context', content: line.slice(1), oldLine, newLine })
        oldLine++
        newLine++
      } else {
        break
      }
    } else if (line.startsWith('\\')) {
      // "\ No newline at end of file" — skip
    } else {
      break
    }

    i++
  }

  return {
    result: { oldStart, oldLines, newStart, newLines, header, changes },
    nextIndex: i,
    additions,
    deletions,
  }
}
