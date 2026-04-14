import type { ProjectSnapshot } from './snapshot-types'

export const mockProjects: ProjectSnapshot[] = [
  {
    id: '1',
    path: '/Users/nix/canopy',
    name: 'canopy',
    isGitRepo: true,
    repoRoot: '/Users/nix/canopy',
    worktrees: [
      { path: '/Users/nix/canopy', branch: 'main', isMain: true },
      { path: '/Users/nix/canopy/worktrees/next', branch: 'next', isMain: false },
      { path: '/Users/nix/canopy/worktrees/fix-crash', branch: 'fix-crash-23', isMain: false },
    ],
  },
  {
    id: '2',
    path: '/Users/nix/party-perfect',
    name: 'party-perfect',
    isGitRepo: true,
    repoRoot: '/Users/nix/party-perfect',
    worktrees: [
      { path: '/Users/nix/party-perfect', branch: 'main', isMain: true },
      { path: '/Users/nix/party-perfect/worktrees/feat-api', branch: 'feat-api', isMain: false },
    ],
  },
  {
    id: '3',
    path: '/Users/nix/notes',
    name: 'notes',
    isGitRepo: true,
    repoRoot: '/Users/nix/notes',
    worktrees: [
      { path: '/Users/nix/notes', branch: 'main', isMain: true },
      { path: '/Users/nix/notes/wt/draft', branch: 'draft-launch', isMain: false },
      { path: '/Users/nix/notes/wt/refactor', branch: 'refactor', isMain: false },
      { path: '/Users/nix/notes/wt/archive', branch: 'archive', isMain: false },
    ],
  },
  {
    id: '4',
    path: '/Users/nix/experiments',
    name: 'experiments',
    isGitRepo: false,
    repoRoot: null,
    worktrees: [],
  },
]

export function makeMockInstance() {
  const rand = Math.floor(Math.random() * 1000)
  return {
    nickname: `Mock #${rand}`,
    hostname: `mock-${rand}.local`,
    lanIp: '192.168.1.50',
    port: 41234,
    token: 'a'.repeat(32),
  }
}
