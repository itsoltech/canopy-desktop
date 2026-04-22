const MRU_LIMIT = 50

// In-memory Most-Recently-Used list per worktree. Persisted implicitly via
// layout store (open files are serialized), so we don't need our own storage.
const mruByWorktree: Record<string, string[]> = {}

export function recordFileOpen(worktreePath: string, relPath: string): void {
  const list = mruByWorktree[worktreePath] ?? []
  const next = [relPath, ...list.filter((p) => p !== relPath)].slice(0, MRU_LIMIT)
  mruByWorktree[worktreePath] = next
}

export function getMru(worktreePath: string): string[] {
  return mruByWorktree[worktreePath] ?? []
}
