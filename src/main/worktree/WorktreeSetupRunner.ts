import * as pty from 'node-pty'
import { copyFile, lstat, mkdir, realpath } from 'fs/promises'
import type { Stats } from 'fs'
import { join, dirname, resolve, relative, isAbsolute } from 'path'
import type { WorktreeSetupAction, WorktreeSetupProgress } from '../db/types'
import { getLoginEnv } from '../shell/loginEnv'

/** True when `target` is `root` itself or a path nested inside it. */
function isWithinRoot(root: string, target: string): boolean {
  const rel = relative(root, target)
  return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel))
}

async function assertSourceWithinRoot(root: string, target: string): Promise<void> {
  const [rootReal, targetReal] = await Promise.all([realpath(root), realpath(target)])
  if (!isWithinRoot(rootReal, targetReal)) {
    throw new Error('Copy action source path escapes its worktree root')
  }
}

async function assertDestinationWithinRoot(root: string, target: string): Promise<void> {
  const rootResolved = resolve(root)
  const targetResolved = resolve(target)
  if (!isWithinRoot(rootResolved, targetResolved)) {
    throw new Error('Copy action destination path escapes its worktree root')
  }

  const rel = relative(rootResolved, dirname(targetResolved))
  let current = rootResolved
  for (const segment of rel.split(/[\\/]+/).filter(Boolean)) {
    current = join(current, segment)
    // Only the lstat belongs in the error path: keeping the symlink rejection
    // inside a try meant this guard's own throw was caught by the handler
    // meant for fs errors, and survived only because a plain Error carries no
    // `.code`. A future error that did would silently skip the check.
    const stat = await lstatOrNull(current)
    if (!stat) break
    if (stat.isSymbolicLink()) {
      throw new Error('Copy action destination path crosses a symlink')
    }
  }

  const targetStat = await lstatOrNull(targetResolved)
  if (targetStat?.isSymbolicLink()) {
    throw new Error('Copy action destination path is a symlink')
  }
}

/** lstat, mapping "does not exist" to null and re-throwing every other error. */
async function lstatOrNull(target: string): Promise<Stats | null> {
  return lstat(target).catch((err: unknown) => {
    if (err instanceof Error && 'code' in err && err.code === 'ENOENT') return null
    throw err
  })
}

export interface SetupContext {
  repoRoot: string
  mainWorktreePath: string
  newWorktreePath: string
}

function shellQuote(s: string): string {
  if (process.platform === 'win32') {
    return '"' + s.replace(/"/g, '\\"') + '"'
  }
  return "'" + s.replace(/'/g, "'\\''") + "'"
}

function substituteVars(command: string, ctx: SetupContext): string {
  return command
    .replace(/\$MAIN_WORKTREE/g, shellQuote(ctx.mainWorktreePath))
    .replace(/\$NEW_WORKTREE/g, shellQuote(ctx.newWorktreePath))
    .replace(/\$REPO_ROOT/g, shellQuote(ctx.repoRoot))
}

function getLabel(action: WorktreeSetupAction): string {
  if (action.label) return action.label
  return action.type === 'command' ? action.command : `Copy ${action.source}`
}

function spawnCommand(
  cmd: string,
  cwd: string,
  onChunk: (raw: string) => void,
  signal?: AbortSignal,
): Promise<void> {
  const loginEnv = getLoginEnv()
  const isWin = process.platform === 'win32'
  const shellPath = isWin ? 'powershell.exe' : loginEnv?.SHELL || process.env.SHELL || '/bin/sh'
  const shellArgs = isWin ? ['-NoProfile', '-Command', cmd] : ['-c', cmd]
  const env = {
    ...(loginEnv ?? (process.env as Record<string, string>)),
    TERM: 'xterm-256color',
    COLORTERM: 'truecolor',
  }

  return new Promise((resolve, reject) => {
    const p = pty.spawn(shellPath, shellArgs, {
      name: 'xterm-256color',
      cols: 80,
      rows: 24,
      cwd,
      env,
    })

    let settled = false
    function settle(fn: () => void): void {
      if (settled) return
      settled = true
      clearTimeout(timeout)
      signal?.removeEventListener('abort', abortHandler)
      fn()
    }

    const timeout = setTimeout(() => {
      p.kill()
      settle(() => reject(new Error('Command timed out after 5 minutes')))
    }, 300_000)

    const abortHandler = (): void => {
      p.kill()
      settle(() => reject(new Error('Setup aborted')))
    }
    signal?.addEventListener('abort', abortHandler, { once: true })

    p.onData((data) => {
      if (!settled) onChunk(data)
    })

    p.onExit(({ exitCode }) => {
      settle(() => {
        if (exitCode === 0) {
          resolve()
        } else {
          reject(new Error(`Command exited with code ${exitCode}`))
        }
      })
    })
  })
}

export async function runWorktreeSetup(
  actions: WorktreeSetupAction[],
  context: SetupContext,
  onProgress: (progress: WorktreeSetupProgress) => void,
  signal?: AbortSignal,
): Promise<{ success: boolean; errors: string[] }> {
  const errors: string[] = []

  for (let i = 0; i < actions.length; i++) {
    if (signal?.aborted) {
      return { success: false, errors: ['Setup aborted'] }
    }

    const action = actions[i]
    const label = getLabel(action)

    onProgress({
      actionIndex: i,
      totalActions: actions.length,
      label,
      status: 'running',
    })

    try {
      if (action.type === 'command') {
        const cmd = substituteVars(action.command, context)
        await spawnCommand(
          cmd,
          context.newWorktreePath,
          (raw) => {
            onProgress({
              actionIndex: i,
              totalActions: actions.length,
              label,
              status: 'running',
              outputChunk: raw,
            })
          },
          signal,
        )
        onProgress({
          actionIndex: i,
          totalActions: actions.length,
          label,
          status: 'done',
        })
      } else {
        const sourcePath = join(context.mainWorktreePath, action.source)
        const destPath = join(context.newWorktreePath, action.dest ?? action.source)
        // Confine copy actions to their real worktree roots. copyFile follows
        // symlinks, so lexical ".." checks alone are not enough here.
        await assertSourceWithinRoot(context.mainWorktreePath, sourcePath)
        await assertDestinationWithinRoot(context.newWorktreePath, destPath)
        await mkdir(dirname(destPath), { recursive: true })
        await copyFile(sourcePath, destPath)
        onProgress({
          actionIndex: i,
          totalActions: actions.length,
          label,
          status: 'done',
        })
      }
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err)
      errors.push(`${label}: ${errorMsg}`)
      onProgress({
        actionIndex: i,
        totalActions: actions.length,
        label,
        status: 'error',
        error: errorMsg,
      })
    }
  }

  return { success: errors.length === 0, errors }
}
