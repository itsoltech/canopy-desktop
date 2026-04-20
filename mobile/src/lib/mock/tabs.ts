import type { TabSnapshot } from './snapshot-types'

function shellTab(id: string, name: string, worktreePath: string, sessionId: string): TabSnapshot {
  return {
    id,
    toolId: 'shell',
    toolName: 'Shell',
    name,
    worktreePath,
    paneType: 'terminal',
    focusedSessionId: sessionId,
  }
}

function claudeTab(id: string, worktreePath: string, sessionId: string): TabSnapshot {
  return {
    id,
    toolId: 'claude',
    toolName: 'Claude',
    name: 'Claude',
    worktreePath,
    paneType: 'terminal',
    focusedSessionId: sessionId,
  }
}

export const mockTabsByWorktree: Record<string, TabSnapshot[]> = {
  '/Users/nix/canopy': [
    shellTab('t-canopy-1', 'Shell', '/Users/nix/canopy', 's-canopy-1'),
    shellTab('t-canopy-2', 'Shell #2', '/Users/nix/canopy', 's-canopy-2'),
    claudeTab('t-canopy-3', '/Users/nix/canopy', 's-canopy-3'),
  ],
  '/Users/nix/canopy/worktrees/next': [
    shellTab('t-next-1', 'Shell', '/Users/nix/canopy/worktrees/next', 's-next-1'),
    claudeTab('t-next-2', '/Users/nix/canopy/worktrees/next', 's-next-2'),
  ],
  '/Users/nix/canopy/worktrees/fix-crash': [
    shellTab('t-fix-1', 'Shell', '/Users/nix/canopy/worktrees/fix-crash', 's-fix-1'),
  ],
  '/Users/nix/party-perfect': [
    shellTab('t-pp-1', 'Shell', '/Users/nix/party-perfect', 's-pp-1'),
    shellTab('t-pp-2', 'Shell #2', '/Users/nix/party-perfect', 's-pp-2'),
  ],
  '/Users/nix/party-perfect/worktrees/feat-api': [
    shellTab('t-pp-api-1', 'Shell', '/Users/nix/party-perfect/worktrees/feat-api', 's-pp-api-1'),
    claudeTab('t-pp-api-2', '/Users/nix/party-perfect/worktrees/feat-api', 's-pp-api-2'),
  ],
  '/Users/nix/notes': [shellTab('t-notes-1', 'Shell', '/Users/nix/notes', 's-notes-1')],
  '/Users/nix/notes/wt/draft': [
    shellTab('t-draft-1', 'Shell', '/Users/nix/notes/wt/draft', 's-draft-1'),
  ],
  '/Users/nix/notes/wt/refactor': [
    shellTab('t-refactor-1', 'Shell', '/Users/nix/notes/wt/refactor', 's-refactor-1'),
    claudeTab('t-refactor-2', '/Users/nix/notes/wt/refactor', 's-refactor-2'),
  ],
  '/Users/nix/notes/wt/archive': [],
}

function prompt(cwd: string, branch: string): string {
  return `\x1b[32mnix@canopy\x1b[0m \x1b[34m${cwd}\x1b[0m (\x1b[33m${branch}\x1b[0m) $ `
}

export const mockTerminalOutput: Record<string, string> = {
  's-canopy-1':
    prompt('canopy', 'main') +
    'ls\n' +
    'apps      docs         package.json   src\n' +
    'CLAUDE.md package-lock electron.vite  tsconfig.json\n' +
    prompt('canopy', 'main'),
  's-canopy-2':
    prompt('canopy', 'main') +
    'git status\n' +
    'On branch \x1b[32mmain\x1b[0m\n' +
    "Your branch is up to date with 'origin/main'.\n\n" +
    'nothing to commit, working tree clean\n' +
    prompt('canopy', 'main'),
  's-canopy-3': '\x1b[36mClaude Code\x1b[0m — session ready\n\n\x1b[90m> \x1b[0m',
  's-next-1':
    prompt('next', 'next') +
    'npm run dev\n' +
    '\x1b[90m[vite] dev server running at:\x1b[0m\n' +
    '  \x1b[36m> Local:\x1b[0m   http://localhost:5173/\n' +
    '  \x1b[36m> Network:\x1b[0m use --host to expose\n',
  's-next-2': '\x1b[36mClaude Code\x1b[0m — session ready\n\n\x1b[90m> \x1b[0m',
  's-fix-1':
    prompt('fix-crash', 'fix-crash-23') +
    'rg "crash" src/main\n' +
    'src/main/remote/RemoteSessionService.ts:142:  // crash recovery\n' +
    'src/main/crash/CrashReporter.ts:28:  reportCrash()\n' +
    prompt('fix-crash', 'fix-crash-23'),
  's-pp-1':
    prompt('party-perfect', 'main') +
    'bun run test\n' +
    '\x1b[32m✓\x1b[0m 42 tests passing\n' +
    '\x1b[32m✓\x1b[0m 0 failures\n' +
    prompt('party-perfect', 'main'),
  's-pp-2': prompt('party-perfect', 'main'),
  's-pp-api-1':
    prompt('feat-api', 'feat-api') +
    'curl http://localhost:3000/api/events\n' +
    '\x1b[33m[\x1b[0m\n' +
    '  { "id": 1, "name": "Launch" },\n' +
    '  { "id": 2, "name": "Rehearsal" }\n' +
    '\x1b[33m]\x1b[0m\n' +
    prompt('feat-api', 'feat-api'),
  's-pp-api-2': '\x1b[36mClaude Code\x1b[0m — reviewing changes\n\n\x1b[90m> \x1b[0m',
  's-notes-1':
    prompt('notes', 'main') + 'ls wt/\n' + 'archive  draft  refactor\n' + prompt('notes', 'main'),
  's-draft-1':
    prompt('draft', 'draft-launch') + 'vim launch.md\n' + prompt('draft', 'draft-launch'),
  's-refactor-1':
    prompt('refactor', 'refactor') +
    'cargo build\n' +
    '\x1b[32m   Compiling\x1b[0m notes v0.1.0\n' +
    '\x1b[32m    Finished\x1b[0m dev [unoptimized] in 1.2s\n' +
    prompt('refactor', 'refactor'),
  's-refactor-2': '\x1b[36mClaude Code\x1b[0m — planning refactor\n\n\x1b[90m> \x1b[0m',
}
