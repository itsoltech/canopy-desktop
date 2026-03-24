import { exec } from 'child_process'
import { copyFile, mkdir } from 'fs/promises'
import { join, dirname } from 'path'
import type { WorktreeSetupAction, WorktreeSetupProgress } from '../db/types'

export interface SetupContext {
  repoRoot: string
  mainWorktreePath: string
  newWorktreePath: string
}

function substituteVars(command: string, ctx: SetupContext): string {
  return command
    .replace(/\$MAIN_WORKTREE/g, ctx.mainWorktreePath)
    .replace(/\$NEW_WORKTREE/g, ctx.newWorktreePath)
    .replace(/\$REPO_ROOT/g, ctx.repoRoot)
}

function getLabel(action: WorktreeSetupAction): string {
  if (action.label) return action.label
  return action.type === 'command' ? action.command : `Copy ${action.source}`
}

function execCommand(cmd: string, cwd: string): Promise<string> {
  return new Promise((resolve, reject) => {
    exec(
      cmd,
      { cwd, shell: '/bin/sh', timeout: 60_000, maxBuffer: 1024 * 1024 },
      (err, stdout, stderr) => {
        if (err) {
          reject(new Error(stderr || err.message))
        } else {
          resolve(stdout)
        }
      },
    )
  })
}

export async function runWorktreeSetup(
  actions: WorktreeSetupAction[],
  context: SetupContext,
  onProgress: (progress: WorktreeSetupProgress) => void,
): Promise<{ success: boolean; errors: string[] }> {
  const errors: string[] = []

  for (let i = 0; i < actions.length; i++) {
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
        const output = await execCommand(cmd, context.newWorktreePath)
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
