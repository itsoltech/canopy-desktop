import * as pty from 'node-pty'
import type { IPty } from 'node-pty'
import { randomUUID } from 'crypto'
import { execFileSync } from 'child_process'
import os from 'os'
import { getLoginEnv } from '../shell/loginEnv'

interface PtySession {
  id: string
  pty: IPty
  tmuxSessionName?: string
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
    const resourcesPath = baseEnv.CANOPY_SKILLS_PATH
    if (resourcesPath) {
      baseEnv.PATH = `${resourcesPath}${os.platform() === 'win32' ? ';' : ':'}${baseEnv.PATH || ''}`
    }
    const env = options?.env ? { ...baseEnv, ...options.env } : baseEnv

    const p = pty.spawn(finalCommand, finalArgs, {
      name: 'xterm-256color',
      cols: options?.cols ?? 80,
      rows: options?.rows ?? 30,
      cwd: options?.cwd ?? os.homedir(),
      env,
    })

    const session: PtySession = { id, pty: p, tmuxSessionName: options?.tmuxSessionName }
    this.sessions.set(id, session)
    return session
  }

  get(id: string): PtySession | undefined {
    return this.sessions.get(id)
  }

  getTmuxSessionName(id: string): string | undefined {
    return this.sessions.get(id)?.tmuxSessionName
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

  write(id: string, data: string): void {
    const session = this.sessions.get(id)
    if (session) {
      session.pty.write(data)
    }
  }

  kill(id: string): void {
    const session = this.sessions.get(id)
    if (session) {
      try {
        session.pty.kill()
      } catch {
        // PTY already exited — ignore
      }
      this.sessions.delete(id)
    }
  }

  hasChildProcess(id: string): boolean {
    const session = this.sessions.get(id)
    if (!session) return false
    try {
      const pid = String(session.pty.pid)
      if (process.platform === 'win32') {
        const out = execFileSync(
          'wmic',
          ['process', 'where', `(ParentProcessId=${pid})`, 'get', 'ProcessId', '/FORMAT:CSV'],
          { encoding: 'utf-8', timeout: 2000 },
        )
        return out.trim().split('\n').length > 1
      }
      execFileSync('pgrep', ['-P', pid], { timeout: 2000 })
      return true
    } catch {
      return false
    }
  }

  get sessionCount(): number {
    return this.sessions.size
  }

  dispose(): void {
    for (const [id, session] of this.sessions) {
      try {
        session.pty.kill()
      } catch {
        // PTY already exited — ignore
      }
      this.sessions.delete(id)
    }
  }
}
