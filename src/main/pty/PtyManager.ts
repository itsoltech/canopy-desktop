import * as pty from 'node-pty'
import type { IPty } from 'node-pty'
import { randomUUID } from 'crypto'
import { execFile, execFileSync } from 'child_process'
import os from 'os'
import { getLoginEnv } from '../shell/loginEnv'
import { BLOCKED_ENV_VARS } from '../security/envBlocklist'
import { comparableWorkspacePath } from '../db/workspacePaths'

interface PtySession {
  id: string
  pty: IPty
  cwd: string
  tmuxSessionName?: string
}

interface KillOptions {
  killProcessTree?: boolean
}

export interface SpawnOptions {
  cols?: number
  rows?: number
  cwd?: string
  command?: string
  args?: string[]
  env?: Record<string, string>
  tmuxSessionName?: string
}

export function resolveShell(): { command: string; args: string[] } {
  if (os.platform() === 'win32') {
    return { command: 'powershell.exe', args: [] }
  }
  const shell = process.env.SHELL || '/bin/bash'
  return { command: shell, args: ['--login'] }
}

export class PtyManager {
  private sessions = new Map<string, PtySession>()

  spawn(options?: SpawnOptions): PtySession {
    const id = randomUUID()
    const shell = resolveShell()
    const command = options?.command ?? shell.command
    const args = options?.args ?? (options?.command ? [] : shell.args)

    // On Windows, non-exe commands must go through cmd.exe
    // so that .cmd/.bat wrappers (e.g. npm global installs) resolve correctly.
    let finalCommand = command
    let finalArgs = args
    if (os.platform() === 'win32' && options?.command && !/\.(exe|com)$/i.test(command)) {
      finalCommand = 'cmd.exe'
      finalArgs = ['/c', command, ...args]
    }

    const baseEnv = {
      ...(getLoginEnv() ?? process.env),
      TERM_PROGRAM: 'canopy',
      COLORTERM: 'truecolor',
      TERM: 'xterm-256color',
    } as Record<string, string>
    // Defense-in-depth: never let caller-supplied env override protected
    // variables (PATH, LD_PRELOAD, NODE_OPTIONS, GIT_SSH_COMMAND, …) at the
    // spawn boundary, even though callers are expected to pre-filter. Keys are
    // normalized to uppercase to match how BLOCKED_ENV_VARS is declared.
    let env = baseEnv
    if (options?.env) {
      const safeEnv: Record<string, string> = {}
      for (const [key, value] of Object.entries(options.env)) {
        if (typeof value === 'string' && !BLOCKED_ENV_VARS.has(key.toUpperCase())) {
          safeEnv[key] = value
        }
      }
      env = { ...baseEnv, ...safeEnv }
    }

    const p = pty.spawn(finalCommand, finalArgs, {
      name: 'xterm-256color',
      cols: options?.cols ?? 80,
      rows: options?.rows ?? 30,
      cwd: options?.cwd ?? os.homedir(),
      env,
    })

    const session: PtySession = {
      id,
      pty: p,
      cwd: options?.cwd ?? os.homedir(),
      tmuxSessionName: options?.tmuxSessionName,
    }
    this.sessions.set(id, session)

    // Remove the session when the PTY exits on its own (shell `exit`, agent CLI
    // completion, crash). Without this, only explicit kill()/dispose() prune the
    // map, so self-terminating PTYs leave dead IPty handles in `sessions` for the
    // life of the process. delete() is idempotent, so this is safe alongside kill().
    p.onExit(() => {
      this.sessions.delete(id)
    })

    return session
  }

  get(id: string): PtySession | undefined {
    return this.sessions.get(id)
  }

  getTmuxSessionName(id: string): string | undefined {
    return this.sessions.get(id)?.tmuxSessionName
  }

  getSessionIdsForTmuxSession(name: string): string[] {
    const ids: string[] = []
    for (const [id, session] of this.sessions) {
      if (session.tmuxSessionName === name) {
        ids.push(id)
      }
    }
    return ids
  }

  updateTmuxSessionName(oldName: string, newName: string): void {
    for (const session of this.sessions.values()) {
      if (session.tmuxSessionName === oldName) {
        session.tmuxSessionName = newName
      }
    }
  }

  isTmuxSession(id: string): boolean {
    return !!this.sessions.get(id)?.tmuxSessionName
  }

  resize(id: string, cols: number, rows: number): void {
    const session = this.sessions.get(id)
    if (session) {
      try {
        session.pty.resize(cols, rows)
      } catch {
        // PTY already closed (EBADF) — ignore
      }
    }
  }

  /**
   * Report the current cols/rows the PTY is running at. Used by the remote
   * peer so its xterm renders at the same dimensions as the host terminal —
   * otherwise shell/agent output (which is laid out for the host's cols)
   * wraps and corrupts on a narrower viewer, and cursor positioning escape
   * sequences end up in the wrong column.
   */
  getDimensions(id: string): { cols: number; rows: number } | null {
    const session = this.sessions.get(id)
    if (!session) return null
    try {
      return { cols: session.pty.cols, rows: session.pty.rows }
    } catch {
      return null
    }
  }

  write(id: string, data: string): void {
    const session = this.sessions.get(id)
    if (session) {
      session.pty.write(data)
    }
  }

  kill(id: string, options?: KillOptions): void {
    const session = this.sessions.get(id)
    if (session) {
      if (options?.killProcessTree) {
        this.terminateProcessTree(session.pty.pid)
      }
      try {
        session.pty.kill()
      } catch {
        // PTY already exited — ignore
      }
      this.sessions.delete(id)
    }
  }

  /**
   * Kill every PTY whose cwd sits inside `dirPath` (a worktree about to be removed)
   * and wait — bounded — until the underlying processes actually exit. Windows keeps
   * a shell's cwd directory handle alive until the process is fully gone, so
   * deleting the directory immediately after a fire-and-forget kill() races the
   * teardown and fails with lock errors. Kills the whole process tree: children
   * spawned inside a directory being deleted must not outlive it.
   *
   * Known limitation: `session.cwd` is the SPAWN cwd — there is no OSC7/cwd
   * tracking, so a shell spawned elsewhere that later `cd`s into the worktree is
   * not matched (its lock is still handled by the caller's retry/cleanup path),
   * and one that `cd`s out is killed unnecessarily. Acceptable best-effort: the
   * dominant case is the tab auto-opened with the worktree as cwd that stays there.
   */
  killUnderPathAndWait(dirPath: string, timeoutMs = 4000): Promise<void> {
    const base = comparableWorkspacePath(dirPath).replace(/\/+$/, '')
    const prefix = `${base}/`
    const waits: Promise<void>[] = []
    for (const [id, session] of [...this.sessions]) {
      const cwd = comparableWorkspacePath(session.cwd)
      if (cwd !== base && !cwd.startsWith(prefix)) continue
      waits.push(this.killSessionTreeAndWait(id, session, timeoutMs))
    }
    return Promise.all(waits).then(() => undefined)
  }

  /**
   * Tree-kill one PTY and wait (bounded) until the process actually exits — the
   * removal-flow variant of kill(): a fire-and-forget kill returns while the dying
   * shell still holds its cwd handle, which blocks directory deletion on Windows.
   */
  killAndWait(id: string, timeoutMs = 4000): Promise<void> {
    const session = this.sessions.get(id)
    if (!session) return Promise.resolve()
    return this.killSessionTreeAndWait(id, session, timeoutMs)
  }

  private killSessionTreeAndWait(
    id: string,
    session: PtySession,
    timeoutMs: number,
  ): Promise<void> {
    return new Promise<void>((resolve) => {
      const timer = setTimeout(resolve, timeoutMs)
      session.pty.onExit(() => {
        clearTimeout(timer)
        resolve()
      })
      this.terminateProcessTree(session.pty.pid)
      try {
        session.pty.kill()
      } catch {
        // PTY already exited — the timeout resolves the wait.
      }
      this.sessions.delete(id)
    })
  }

  hasChildProcess(id: string): Promise<boolean> {
    const session = this.sessions.get(id)
    if (!session) return Promise.resolve(false)
    // Windows process-tree probes (`wmic`, PowerShell CIM queries, etc.) are
    // too expensive for close-warning preflights and can visibly stall the app.
    // Windows shell tabs therefore never raise running-process close warnings;
    // agent-busy state is still checked separately by AgentSessionManager.
    if (process.platform === 'win32') return Promise.resolve(false)

    const pid = String(session.pty.pid)
    const cmd = 'pgrep'
    const args = ['-P', pid]
    return new Promise<boolean>((resolve) => {
      execFile(cmd, args, { encoding: 'utf-8', timeout: 2000 }, (err) => {
        resolve(!err)
      })
    })
  }

  get sessionCount(): number {
    return this.sessions.size
  }

  dispose(): void {
    for (const [id, session] of this.sessions) {
      this.terminateProcessTree(session.pty.pid)
      try {
        session.pty.kill()
      } catch {
        // PTY already exited — ignore
      }
      this.sessions.delete(id)
    }
  }

  private terminateProcessTree(pid: number): void {
    if (!Number.isFinite(pid) || pid <= 0) return

    if (process.platform === 'win32') {
      try {
        execFileSync('taskkill', ['/PID', String(pid), '/T', '/F'], {
          stdio: 'ignore',
          timeout: 2000,
        })
      } catch {
        // Process may already be gone.
      }
      return
    }

    const pids = [...this.collectDescendantPids(pid), pid]
    for (const targetPid of pids) {
      this.signalPid(targetPid, 'SIGTERM')
    }

    const forceKill = setTimeout(() => {
      for (const targetPid of pids) {
        this.signalPid(targetPid, 'SIGKILL')
      }
    }, 750)
    forceKill.unref?.()
  }

  private collectDescendantPids(pid: number): number[] {
    const children = this.collectChildPids(pid)
    return children.flatMap((childPid) => [...this.collectDescendantPids(childPid), childPid])
  }

  private collectChildPids(pid: number): number[] {
    try {
      return execFileSync('pgrep', ['-P', String(pid)], {
        encoding: 'utf-8',
        timeout: 1000,
        stdio: ['ignore', 'pipe', 'ignore'],
      })
        .split(/\s+/)
        .map((value) => Number(value))
        .filter((value) => Number.isInteger(value) && value > 0)
    } catch {
      return []
    }
  }

  private signalPid(pid: number, signal: NodeJS.Signals): void {
    try {
      process.kill(pid, signal)
    } catch {
      // Process may already be gone or owned by another user.
    }
  }
}
