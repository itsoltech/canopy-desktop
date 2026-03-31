import * as pty from 'node-pty'
import { copyFile, mkdir } from 'fs/promises'
import { join, dirname } from 'path'
import type { WorktreeSetupAction, WorktreeSetupProgress } from '../db/types'
import { getLoginEnv } from '../shell/loginEnv'

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
