import type { Href } from 'expo-router'

/**
 * Typed navigation targets for the app's routes. Centralizes pathname strings
 * and param shaping (e.g. encoding the worktree path) so screens call
 * `router.push(AppRoutes.terminal(id, path))` instead of repeating literals.
 */
export const AppRoutes = {
  scan: (): Href => '/scan',
  instance: (id: string): Href => ({ pathname: '/instance/[id]', params: { id } }),
  terminal: (instanceId: string, worktreePath: string): Href => ({
    pathname: '/terminal',
    params: { instanceId, worktreePath: encodeURIComponent(worktreePath) },
  }),
  projectNew: (): Href => '/project/new',
  worktreeNew: (projectId: string): Href => ({
    pathname: '/worktree/new',
    params: { projectId },
  }),
  settingsAppearance: (): Href => '/settings/appearance',
} as const
