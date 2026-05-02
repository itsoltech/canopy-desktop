import * as pty from 'node-pty'
import { copyFile, mkdir } from 'fs/promises'
import { join, dirname } from 'path'
import { Data, Effect } from 'effect'
import type { WorktreeSetupAction, WorktreeSetupProgress } from '../db/types'
import { getLoginEnv } from '../shell/loginEnv'

export interface SetupContext {
  repoRoot: string
  mainWorktreePath: string
  newWorktreePath: string
}

export type WorktreeSetupStage = 'command' | 'copy' | 'timeout' | 'abort'

export class WorktreeSetupError extends Data.TaggedError('WorktreeSetupError')<{
  readonly stage: WorktreeSetupStage
  readonly label: string
  readonly message: string
  readonly cause?: unknown
}> {}

function worktreeSetupError(
  stage: WorktreeSetupStage,
  label: string,
  message: string,
  cause?: unknown,
): WorktreeSetupError {
  return new WorktreeSetupError({ stage, label, message, cause })
}

function errorMessage(cause: unknown): string {
  if (cause instanceof Error) return cause.message
  return String(cause)
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

// Test-only seam. Public production callers go through `runWorktreeSetupEffect`
// / `runWorktreeSetup` and never need to construct `WorktreeSetupDeps`.
export interface WorktreeSetupDeps {
  readonly spawnPty: typeof pty.spawn
  readonly mkdir: typeof mkdir
  readonly copyFile: typeof copyFile
  readonly getLoginEnv: typeof getLoginEnv
  readonly setTimeout: (handler: () => void, ms: number) => unknown
  readonly clearTimeout: (handle: unknown) => void
}

const liveDeps: WorktreeSetupDeps = {
  spawnPty: pty.spawn,
  mkdir,
  copyFile,
  getLoginEnv,
  setTimeout: (handler, ms) => globalThis.setTimeout(handler, ms),
  clearTimeout: (handle) =>
    globalThis.clearTimeout(handle as Parameters<typeof globalThis.clearTimeout>[0]),
}

function spawnCommandEffect(
  deps: WorktreeSetupDeps,
  cmd: string,
  cwd: string,
  label: string,
  onChunk: (raw: string) => void,
  signal?: AbortSignal,
): Effect.Effect<void, WorktreeSetupError> {
  return Effect.async<void, WorktreeSetupError>((resume) => {
    // Track each partially-acquired resource so a throw mid-registration
    // (timer scheduling, abort listener attach, pty.onData/onExit) cannot
    // leak a spawned pty or dangling listeners.
    let p: pty.IPty | null = null
    let timeoutHandle: unknown = undefined
    let timeoutAcquired = false
    let dataDisposable: pty.IDisposable | null = null
    let exitDisposable: pty.IDisposable | null = null
    let abortHandler: (() => void) | null = null
    let settled = false
    let killOnFinalize = true

    const safeKill = (): void => {
      if (!p) return
      try {
        p.kill()
      } catch {
        // Cleanup must not mask the original setup failure/interruption.
      }
    }

    const cleanup = (): void => {
      if (timeoutAcquired) {
        try {
          deps.clearTimeout(timeoutHandle)
        } catch {
          // ignore
        }
      }
      if (signal && abortHandler) {
        try {
          signal.removeEventListener('abort', abortHandler)
        } catch {
          // ignore
        }
      }
      if (dataDisposable) {
        try {
          dataDisposable.dispose()
        } catch {
          // ignore
        }
      }
      if (exitDisposable) {
        try {
          exitDisposable.dispose()
        } catch {
          // ignore
        }
      }
      if (killOnFinalize) safeKill()
    }

    const settle = (effect: Effect.Effect<void, WorktreeSetupError>): void => {
      if (settled) return
      settled = true
      killOnFinalize = false
      cleanup()
      resume(effect)
    }

    try {
      const loginEnv = deps.getLoginEnv()
      const isWin = process.platform === 'win32'
      const shellPath = isWin ? 'powershell.exe' : loginEnv?.SHELL || process.env.SHELL || '/bin/sh'
      const shellArgs = isWin ? ['-NoProfile', '-Command', cmd] : ['-c', cmd]
      const env = {
        ...(loginEnv ?? (process.env as Record<string, string>)),
        TERM: 'xterm-256color',
        COLORTERM: 'truecolor',
      }

      p = deps.spawnPty(shellPath, shellArgs, {
        name: 'xterm-256color',
        cols: 80,
        rows: 24,
        cwd,
        env,
      })

      timeoutHandle = deps.setTimeout(() => {
        safeKill()
        settle(
          Effect.fail(worktreeSetupError('timeout', label, 'Command timed out after 5 minutes')),
        )
      }, 300_000)
      timeoutAcquired = true

      abortHandler = (): void => {
        safeKill()
        settle(Effect.fail(worktreeSetupError('abort', label, 'Setup aborted')))
      }
      signal?.addEventListener('abort', abortHandler, { once: true })

      dataDisposable = p.onData((data) => {
        if (settled) return
        try {
          onChunk(data)
        } catch {
          // Progress reporting is best-effort; do not let renderer races escape
          // the typed setup error channel from a native event callback.
        }
      })

      exitDisposable = p.onExit(({ exitCode }) => {
        if (exitCode === 0) {
          settle(Effect.succeed(undefined))
        } else {
          settle(
            Effect.fail(
              worktreeSetupError('command', label, `Command exited with code ${exitCode}`),
            ),
          )
        }
      })

      return Effect.sync(cleanup)
    } catch (cause) {
      // Mid-registration failure: tear down whatever was already acquired
      // (pty, timer, abort listener, partial disposables) so nothing leaks.
      // Note: we deliberately leave `killOnFinalize` as true here so cleanup
      // kills the still-live pty — unlike the normal settle path, no
      // safeKill has run yet on this branch.
      if (!settled) {
        settled = true
        cleanup()
        resume(Effect.fail(worktreeSetupError('command', label, errorMessage(cause), cause)))
      }
      return Effect.void
    }
  })
}

function runActionEffect(
  deps: WorktreeSetupDeps,
  action: WorktreeSetupAction,
  ctx: SetupContext,
  label: string,
  onChunk: (raw: string) => void,
  signal?: AbortSignal,
): Effect.Effect<void, WorktreeSetupError> {
  if (action.type === 'command') {
    return spawnCommandEffect(
      deps,
      substituteVars(action.command, ctx),
      ctx.newWorktreePath,
      label,
      onChunk,
      signal,
    )
  }

  return Effect.gen(function* () {
    const sourcePath = join(ctx.mainWorktreePath, action.source)
    const destPath = join(ctx.newWorktreePath, action.dest ?? action.source)
    yield* Effect.tryPromise({
      try: () => deps.mkdir(dirname(destPath), { recursive: true }),
      catch: (cause) => worktreeSetupError('copy', label, errorMessage(cause), cause),
    })
    yield* Effect.tryPromise({
      try: () => deps.copyFile(sourcePath, destPath),
      catch: (cause) => worktreeSetupError('copy', label, errorMessage(cause), cause),
    })
  })
}

function runWorktreeSetupEffectWithDeps(
  deps: WorktreeSetupDeps,
  actions: WorktreeSetupAction[],
  context: SetupContext,
  onProgress: (progress: WorktreeSetupProgress) => void,
  signal?: AbortSignal,
): Effect.Effect<{ success: boolean; errors: string[] }> {
  // Progress reporting is best-effort. A throwing renderer-side listener must
  // never surface as an Effect defect that masks the typed setup result, nor
  // abort the rest of the action queue.
  const reportProgress = (progress: WorktreeSetupProgress): void => {
    try {
      onProgress(progress)
    } catch {
      // swallow — progress is observational, not part of the result contract
    }
  }

  return Effect.gen(function* () {
    const errors: string[] = []

    for (let i = 0; i < actions.length; i++) {
      if (signal?.aborted) {
        return { success: false, errors: ['Setup aborted'] }
      }

      const action = actions[i]
      const label = getLabel(action)

      yield* Effect.sync(() => {
        reportProgress({
          actionIndex: i,
          totalActions: actions.length,
          label,
          status: 'running',
        })
      })

      const result = yield* runActionEffect(
        deps,
        action,
        context,
        label,
        (raw) => {
          reportProgress({
            actionIndex: i,
            totalActions: actions.length,
            label,
            status: 'running',
            outputChunk: raw,
          })
        },
        signal,
      ).pipe(
        Effect.match({
          onFailure: (error) => ({ _tag: 'failure' as const, error }),
          onSuccess: () => ({ _tag: 'success' as const }),
        }),
      )

      if (result._tag === 'success') {
        yield* Effect.sync(() => {
          reportProgress({
            actionIndex: i,
            totalActions: actions.length,
            label,
            status: 'done',
          })
        })
      } else {
        errors.push(`${label}: ${result.error.message}`)
        yield* Effect.sync(() => {
          reportProgress({
            actionIndex: i,
            totalActions: actions.length,
            label,
            status: 'error',
            error: result.error.message,
          })
        })
      }
    }

    return { success: errors.length === 0, errors }
  })
}

export function runWorktreeSetupEffect(
  actions: WorktreeSetupAction[],
  context: SetupContext,
  onProgress: (progress: WorktreeSetupProgress) => void,
  signal?: AbortSignal,
): Effect.Effect<{ success: boolean; errors: string[] }> {
  return runWorktreeSetupEffectWithDeps(liveDeps, actions, context, onProgress, signal)
}

/**
 * Test-only constructor. Inject fakes for filesystem / pty / timers so
 * the Effect program can be exercised without real I/O. Production code
 * should call `runWorktreeSetupEffect` or `runWorktreeSetup` instead.
 */
export function createWorktreeSetupEffect(
  deps: WorktreeSetupDeps,
  actions: WorktreeSetupAction[],
  context: SetupContext,
  onProgress: (progress: WorktreeSetupProgress) => void,
  signal?: AbortSignal,
): Effect.Effect<{ success: boolean; errors: string[] }> {
  return runWorktreeSetupEffectWithDeps(deps, actions, context, onProgress, signal)
}

export async function runWorktreeSetup(
  actions: WorktreeSetupAction[],
  context: SetupContext,
  onProgress: (progress: WorktreeSetupProgress) => void,
  signal?: AbortSignal,
): Promise<{ success: boolean; errors: string[] }> {
  return Effect.runPromise(runWorktreeSetupEffect(actions, context, onProgress, signal))
}
