import * as pty from 'node-pty'
import type { IPty } from 'node-pty'
import { randomUUID } from 'crypto'
import { execFile, execFileSync } from 'child_process'
import os from 'os'
import { getLoginEnv } from '../shell/loginEnv'
import { BLOCKED_ENV_VARS } from '../security/envBlocklist'

interface PtySession {
  id: string
  pty: IPty
  tmuxSessionName?: string
}

interface ClientSize {
  cols: number
  rows: number
}

/**
 * Per-session record of each attached client's desired terminal size. A PTY
 * session is shared between the desktop window(s) and (at most one) remote
 * peer. The effective PTY size is the MINIMUM across attached clients
 * (tmux/zellij "smallest client wins"), so the width the shell/agent renders
 * at is never larger than any viewer's grid — otherwise the narrower viewer
 * rewraps and cursor-positioning escapes land in the wrong cells (duplicated
 * / lost text on scroll).
 *
 * The peer entry is STICKY: it survives WebSocket disconnects (iOS suspends
 * the app after ~30s of backgrounding while agent tasks run longer, so peers
 * disconnect/reconnect constantly). Dropping the cap on every disconnect would
 * flap the PTY size and force the agent to redraw repeatedly. The cap is only
 * released by an explicit desktop reclaim (focus/click) while the peer is
 * disconnected, or when the session is destroyed.
 */
interface SizeArbiterEntry {
  desktop: ClientSize | null
  peer: (ClientSize & { connected: boolean }) | null
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
  private sizeArbiter = new Map<string, SizeArbiterEntry>()

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

    const session: PtySession = { id, pty: p, tmuxSessionName: options?.tmuxSessionName }
    this.sessions.set(id, session)

    // Remove the session when the PTY exits on its own (shell `exit`, agent CLI
    // completion, crash). Without this, only explicit kill()/dispose() prune the
    // map, so self-terminating PTYs leave dead IPty handles in `sessions` for the
    // life of the process. delete() is idempotent, so this is safe alongside kill().
    p.onExit(() => {
      this.sessions.delete(id)
      this.sizeArbiter.delete(id)
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
   * Record a client's desired terminal size and resize the shared PTY to the
   * MINIMUM across all attached clients ("smallest client wins"). Returns the
   * effective dims actually applied so the caller can broadcast them to every
   * viewer, or null if the session is gone. See {@link SizeArbiterEntry}.
   */
  requestResize(
    id: string,
    cols: number,
    rows: number,
    source: 'desktop' | 'peer',
  ): ClientSize | null {
    // Guard before getArbiterEntry: a resize can arrive after the PTY exited
    // (onExit already pruned sessions + sizeArbiter). Without this, the late
    // call would recreate an orphaned arbiter entry that nothing prunes.
    if (!this.sessions.has(id)) return null
    const size = this.clampSize(cols, rows)
    const entry = this.getArbiterEntry(id)
    if (source === 'desktop') {
      entry.desktop = size
    } else {
      entry.peer = { ...size, connected: true }
    }
    return this.applyEffectiveSize(id)
  }

  /**
   * Mark the remote peer as disconnected from a session WITHOUT releasing its
   * size contribution. The cap stays sticky so a returning peer (the common
   * iOS background→foreground case) keeps the same dimensions and the PTY does
   * not flap. The PTY size is intentionally left unchanged here.
   */
  setPeerDisconnected(id: string): void {
    const entry = this.sizeArbiter.get(id)
    if (entry?.peer) {
      entry.peer.connected = false
    }
  }

  /**
   * Explicit desktop reclaim: the user returned to the desktop (terminal
   * focus / keydown / pointerdown). If a peer's size cap is still sticky from a
   * previous disconnect, release it so the PTY grows back to the desktop size.
   * A currently-connected peer is left untouched — a live phone still wins.
   * Returns the new effective dims if they changed (to broadcast), else null.
   */
  reclaim(id: string): ClientSize | null {
    const entry = this.sizeArbiter.get(id)
    if (!entry?.peer || entry.peer.connected) return null
    entry.peer = null
    return this.applyEffectiveSize(id)
  }

  private clampSize(cols: number, rows: number): ClientSize {
    // Canonical clamp shared by every client so min() compares like-for-like
    // (matches the remote RPC bounds). Never below xterm's 10×3 floor.
    return {
      cols: Math.max(10, Math.min(Math.round(cols), 500)),
      rows: Math.max(3, Math.min(Math.round(rows), 200)),
    }
  }

  private getArbiterEntry(id: string): SizeArbiterEntry {
    let entry = this.sizeArbiter.get(id)
    if (!entry) {
      entry = { desktop: null, peer: null }
      this.sizeArbiter.set(id, entry)
    }
    return entry
  }

  private computeEffectiveSize(entry: SizeArbiterEntry): ClientSize | null {
    const candidates: ClientSize[] = []
    if (entry.desktop) candidates.push(entry.desktop)
    if (entry.peer) candidates.push({ cols: entry.peer.cols, rows: entry.peer.rows })
    if (candidates.length === 0) return null
    return {
      cols: Math.max(10, Math.min(...candidates.map((c) => c.cols))),
      rows: Math.max(3, Math.min(...candidates.map((c) => c.rows))),
    }
  }

  private applyEffectiveSize(id: string): ClientSize | null {
    const entry = this.sizeArbiter.get(id)
    const session = this.sessions.get(id)
    if (!entry || !session) return null
    const eff = this.computeEffectiveSize(entry)
    if (!eff) return null
    // Idempotent: only touch node-pty when the size actually changes, so
    // desktop focus/resize spam doesn't fire redundant SIGWINCH redraws.
    if (session.pty.cols !== eff.cols || session.pty.rows !== eff.rows) {
      try {
        session.pty.resize(eff.cols, eff.rows)
      } catch {
        // PTY already closed (EBADF) — ignore
      }
    }
    return eff
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
      this.sizeArbiter.delete(id)
    }
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
    this.sizeArbiter.clear()
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
