import { execFile } from 'child_process'
import { writeFile, readFile } from 'fs/promises'
import { join } from 'path'
import os from 'os'
import { randomUUID } from 'crypto'
import { getLoginEnv } from '../shell/loginEnv'

export interface TmuxSessionInfo {
  name: string
  created: number
  attached: boolean
  cwd: string
}

const SOCKET_NAME = 'canopy'

const CANOPY_TMUX_CONFIG = `\
set -g status off
set -g prefix None
unbind-key -a
set -g escape-time 0
set -g default-terminal xterm-256color
set -g mouse off
set -g history-limit 10000
`

export class TmuxManager {
  private tmuxPath: string | null = null
  private available: boolean | null = null
  private configPath: string

  constructor(userDataDir: string) {
    this.configPath = join(userDataDir, 'canopy.tmux.conf')
  }

  // Cached for the lifetime of the process (tmux availability won't change mid-session)
  async isAvailable(): Promise<boolean> {
    if (this.available !== null) return this.available

    if (os.platform() === 'win32') {
      this.available = false
      return false
    }

    try {
      const path = await this.which('tmux')
      this.tmuxPath = path
      this.available = true
    } catch {
      this.available = false
    }
    return this.available
  }

  async getVersion(): Promise<string | null> {
    if (!(await this.isAvailable())) return null
    try {
      const out = await this.exec(['-V'])
      return out.trim()
    } catch {
      return null
    }
  }

  async ensureConfig(): Promise<void> {
    try {
      const existing = await readFile(this.configPath, 'utf-8')
      if (existing === CANOPY_TMUX_CONFIG) return
    } catch {
      // File doesn't exist — fall through to write
    }
    await writeFile(this.configPath, CANOPY_TMUX_CONFIG, 'utf-8')
  }

  async newSession(opts: {
    name: string
    cwd: string
    shell: string
    shellArgs?: string[]
    cols?: number
    rows?: number
  }): Promise<void> {
    if (!/^[\w-]+$/.test(opts.name)) {
      throw new Error('Invalid tmux session name')
    }
    await this.ensureConfig()
    const args = [
      '-f',
      this.configPath,
      'new-session',
      '-d',
      '-s',
      opts.name,
      '-c',
      opts.cwd,
      '-x',
      String(opts.cols ?? 80),
      '-y',
      String(opts.rows ?? 30),
      opts.shell,
      ...(opts.shellArgs ?? []),
    ]
    await this.exec(args)
  }

  attachArgs(sessionName: string): { command: string; args: string[] } {
    return {
      command: this.tmuxPath!,
      args: ['-u', '-L', SOCKET_NAME, 'attach-session', '-t', sessionName],
    }
  }

  async hasSession(name: string): Promise<boolean> {
    try {
      await this.exec(['has-session', '-t', name])
      return true
    } catch {
      return false
    }
  }

  async listSessions(): Promise<TmuxSessionInfo[]> {
    try {
      const out = await this.exec([
        'list-sessions',
        '-F',
        '#{session_name}\t#{session_created}\t#{session_attached}\t#{pane_current_path}',
      ])
      return out
        .trim()
        .split('\n')
        .filter((line) => line.length > 0)
        .map((line) => {
          const [name, created, attached, cwd] = line.split('\t')
          return {
            name,
            created: parseInt(created, 10),
            attached: attached !== '0',
            cwd: cwd || '',
          }
        })
      // No filter — all sessions on the canopy socket are ours
    } catch {
      return []
    }
  }

  async killSession(name: string): Promise<void> {
    await this.exec(['kill-session', '-t', name])
  }

  async renameSession(oldName: string, newName: string): Promise<void> {
    await this.exec(['rename-session', '-t', oldName, newName])
  }

  async killServer(): Promise<void> {
    try {
      await this.exec(['kill-server'])
    } catch {
      // Server may not be running
    }
  }

  static sessionName(workspaceId: string, suffix?: string): string {
    const wsPrefix = workspaceId.slice(0, 12)
    const id = suffix ?? randomUUID().slice(0, 8)
    return `canopy-${wsPrefix}-${id}`
  }

  static isCanopySession(name: string): boolean {
    return name.startsWith('canopy-')
  }

  static workspaceIdFromSession(name: string): string | null {
    const match = name.match(/^canopy-([a-f0-9-]{1,12})-/)
    return match ? match[1] : null
  }

  private async exec(args: string[]): Promise<string> {
    const fullArgs = ['-u', '-L', SOCKET_NAME, ...args]
    const env = getLoginEnv() ?? (process.env as Record<string, string>)

    return new Promise((resolve, reject) => {
      execFile(this.tmuxPath!, fullArgs, { env, timeout: 10000 }, (err, stdout) => {
        if (err) reject(err)
        else resolve(stdout)
      })
    })
  }

  private async which(binary: string): Promise<string> {
    const cmd = os.platform() === 'win32' ? 'where' : 'which'
    const env = getLoginEnv() ?? (process.env as Record<string, string>)

    return new Promise((resolve, reject) => {
      execFile(cmd, [binary], { env, timeout: 5000 }, (err, stdout) => {
        if (err) reject(err)
        else resolve(stdout.trim().split('\n')[0])
      })
    })
  }
}
