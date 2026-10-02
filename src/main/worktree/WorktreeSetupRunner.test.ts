import { describe, expect, it, vi } from 'vitest'

vi.mock('node-pty', () => ({ spawn: vi.fn() }))

import { substituteVars } from './WorktreeSetupRunner'

describe('substituteVars', () => {
  it('quotes paths as PowerShell literals on Windows', () => {
    const ctx = {
      repoRoot: 'C:\\src\\repo',
      mainWorktreePath: "C:\\src\\my$app's",
      newWorktreePath: 'C:\\wt\\$(calc)',
    }

    expect(substituteVars('copy $MAIN_WORKTREE\\.env $NEW_WORKTREE', ctx, 'win32')).toBe(
      "copy 'C:\\src\\my$app''s'\\.env 'C:\\wt\\$(calc)'",
    )
  })

  it('doubles typographic single quotes, which PowerShell also treats as delimiters', () => {
    const ctx = { repoRoot: 'C:\\src\\it\u2019s; calc', mainWorktreePath: '', newWorktreePath: '' }

    expect(substituteVars('cd $REPO_ROOT', ctx, 'win32')).toBe(
      "cd 'C:\\src\\it\u2019\u2019s; calc'",
    )
  })

  it('inserts paths containing replacement patterns literally', () => {
    const ctx = {
      repoRoot: "/src/a$'b",
      mainWorktreePath: '/src/main',
      newWorktreePath: '/wt/x$&y',
    }

    expect(substituteVars('cp $REPO_ROOT/.env $NEW_WORKTREE', ctx, 'linux')).toBe(
      "cp '/src/a$'\\''b'/.env '/wt/x$&y'",
    )
  })
})
