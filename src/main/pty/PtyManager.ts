import * as pty from 'node-pty'
import type { IPty } from 'node-pty'
import { randomUUID } from 'crypto'
import os from 'os'

interface PtySession {
  id: string
  pty: IPty
}

export interface SpawnOptions {
  cols?: number
  rows?: number
  cwd?: string
}

function resolveShell(): string {
  if (os.platform() === 'win32') {
    return 'powershell.exe'
  }
  return process.env.SHELL || '/bin/bash'
}

export class PtyManager {
  private sessions = new Map<string, PtySession>()

  spawn(options?: SpawnOptions): PtySession {
    const id = randomUUID()
    const shell = resolveShell()

    const p = pty.spawn(shell, [], {
      name: 'xterm-256color',
      cols: options?.cols ?? 80,
      rows: options?.rows ?? 30,
      cwd: options?.cwd ?? os.homedir(),
      env: { ...process.env, TERM_PROGRAM: 'nixtty' } as Record<string, string>
    })

    const session: PtySession = { id, pty: p }
    this.sessions.set(id, session)
    return session
  }

  get(id: string): PtySession | undefined {
    return this.sessions.get(id)
  }

  resize(id: string, cols: number, rows: number): void {
    const session = this.sessions.get(id)
    if (session) {
      session.pty.resize(cols, rows)
    }
  }

  kill(id: string): void {
    const session = this.sessions.get(id)
    if (session) {
      session.pty.kill()
      this.sessions.delete(id)
    }
  }

  dispose(): void {
    for (const [id, session] of this.sessions) {
      session.pty.kill()
      this.sessions.delete(id)
    }
  }
}
