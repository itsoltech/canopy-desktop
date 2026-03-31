import { spawn } from 'child_process'
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
): Promise<string> {
  const loginEnv = getLoginEnv()
  const shellPath = loginEnv?.SHELL || process.env.SHELL || '/bin/sh'
  const env = loginEnv ?? (process.env as Record<string, string>)

  return new Promise((resolve, reject) => {
    const child = spawn(cmd, {
      cwd,
      shell: shellPath,
      env,
      stdio: ['ignore', 'pipe', 'pipe'],
    })

    let fullOutput = ''

    const timeout = setTimeout(() => {
      child.kill('SIGTERM')
      reject(new Error('Command timed out after 5 minutes'))
    }, 300_000)

    const abortHandler = (): void => {
      child.kill('SIGTERM')
    }
    signal?.addEventListener('abort', abortHandler, { once: true })

    function handleData(data: Buffer): void {
      const raw = data.toString()
      fullOutput += raw
      onChunk(raw)
    }

    child.stdout?.on('data', handleData)
    child.stderr?.on('data', handleData)

    child.on('close', (code) => {
      clearTimeout(timeout)
      signal?.removeEventListener('abort', abortHandler)
      if (signal?.aborted) {
        reject(new Error('Setup aborted'))
      } else if (code === 0 || code === null) {
        resolve(fullOutput)
      } else {
        reject(new Error(`Command exited with code ${code}`))
      }
    })

    child.on('error', (err) => {
      clearTimeout(timeout)
      signal?.removeEventListener('abort', abortHandler)
      reject(err)
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
        const output = await spawnCommand(
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
          output,
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
