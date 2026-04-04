export interface ParsedDiff {
  files: DiffFile[]
}

export interface DiffFile {
  path: string
  oldPath?: string
  status: 'added' | 'modified' | 'deleted' | 'renamed'
  hunks: DiffHunk[]
  additions: number
  deletions: number
}

export interface DiffHunk {
  oldStart: number
  oldLines: number
  newStart: number
  newLines: number
  header: string
  changes: DiffChange[]
}

export interface DiffChange {
  type: 'add' | 'delete' | 'context'
  content: string
  oldLine?: number
  newLine?: number
}
