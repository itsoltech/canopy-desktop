import { app, BrowserWindow, Notification } from 'electron'
import { join } from 'path'
import { writeFileSync, mkdirSync, unlinkSync, readdirSync, existsSync, chmodSync } from 'fs'
import { randomUUID } from 'crypto'
import { is } from '@electron-toolkit/utils'
import { ClaudeHookServer } from './ClaudeHookServer'
import type { HookEvent } from './types'

interface ClaudeSession {
  claudeSessionId: string
  ptySessionId: string
  settingsPath: string
  hookServer: ClaudeHookServer
  hookPort: number
  worktreePath: string
  workspaceName: string
  branch: string | null
  sessionRef: { ptySessionId: string }
  ownerWindow: BrowserWindow
}

const BUSY_EVENTS = new Set(['UserPromptSubmit', 'PreToolUse', 'PreCompact', 'PermissionRequest'])
const IDLE_EVENTS = new Set(['Stop', 'StopFailure', 'SessionEnd'])

export class ClaudeSessionManager {
  private sessions = new Map<string, ClaudeSession>()
  private busySessions = new Set<string>()
  private hooksDir: string

  constructor() {
    this.hooksDir = join(app.getPath('userData'), 'canopy', 'claude-hooks')
    mkdirSync(this.hooksDir, { recursive: true })
  }

  async createSession(
    worktreePath: string,
    workspaceName: string,
    branch: string | null,
    ownerWindow: BrowserWindow,
    settingsOverrides?: Record<string, unknown>,
  ): Promise<{ settingsPath: string; hookPort: number; tempId: string }> {
    const claudeSessionId = randomUUID()
    const tempId = `_claude_${claudeSessionId}`

    // Use a mutable ref so closures always see the current ptySessionId
    const sessionRef = { ptySessionId: tempId }

    const hookServer = new ClaudeHookServer(
      (event: HookEvent): Record<string, unknown> | void => {
        // Track busy state for close-warning checks
        if (BUSY_EVENTS.has(event.hook_event_name)) {
          this.busySessions.add(sessionRef.ptySessionId)
        } else if (IDLE_EVENTS.has(event.hook_event_name)) {
          this.busySessions.delete(sessionRef.ptySessionId)
        }

        if (ownerWindow.isDestroyed()) return

        ownerWindow.webContents.send('claude:hookEvent', {
          ptySessionId: sessionRef.ptySessionId,
          event,
        })

        if (event.hook_event_name === 'PermissionRequest') {
          this.showPermissionNotification(event, sessionRef.ptySessionId, ownerWindow)
        }

        if (event.hook_event_name === 'SessionStart') {
          return {
            hookSpecificOutput: {
              hookEventName: 'SessionStart',
              additionalContext: this.buildSessionContext(worktreePath, workspaceName, branch),
            },
          }
        }
      },
      (data: Record<string, unknown>) => {
        if (ownerWindow.isDestroyed()) return

        ownerWindow.webContents.send('claude:statusUpdate', {
          ptySessionId: sessionRef.ptySessionId,
          status: data,
        })
      },
    )

    const hookPort = await hookServer.start()
    const settingsPath = join(this.hooksDir, `session-${claudeSessionId}.json`)
    const settingsJson = this.buildSettingsJson(settingsOverrides)
    writeFileSync(settingsPath, settingsJson, 'utf-8')

    const session: ClaudeSession = {
      claudeSessionId,
      ptySessionId: tempId,
      settingsPath,
      hookServer,
      hookPort,
      worktreePath,
      workspaceName,
      branch,
      sessionRef,
      ownerWindow,
    }
    this.sessions.set(tempId, session)

    return { settingsPath, hookPort, tempId }
  }

  rekey(tempId: string, realPtySessionId: string): void {
    const session = this.sessions.get(tempId)
    if (!session) return
    this.sessions.delete(tempId)
    session.ptySessionId = realPtySessionId
    session.sessionRef.ptySessionId = realPtySessionId
    this.sessions.set(realPtySessionId, session)
  }

  isBusy(ptySessionId: string): boolean {
    return this.busySessions.has(ptySessionId)
  }

  isClaudeSession(ptySessionId: string): boolean {
    return this.sessions.has(ptySessionId)
  }

  destroySession(ptySessionId: string): void {
    const session = this.sessions.get(ptySessionId)
    if (!session) return

    session.hookServer.destroy()
    this.busySessions.delete(ptySessionId)

    try {
      unlinkSync(session.settingsPath)
    } catch {
      // File may already be deleted
    }

    this.sessions.delete(ptySessionId)
  }

  cleanupOrphans(): void {
    if (!existsSync(this.hooksDir)) return

    const files = readdirSync(this.hooksDir)
    for (const file of files) {
      if (file.startsWith('session-') && file.endsWith('.json')) {
        try {
          unlinkSync(join(this.hooksDir, file))
        } catch {
          // Ignore
        }
      }
    }
  }

  dispose(): void {
    for (const [id] of this.sessions) {
      this.destroySession(id)
    }
    this.busySessions.clear()
  }

  private getHookScriptPath(): string {
    const scriptName =
      process.platform === 'win32' ? 'canopy-claude-hook.cmd' : 'canopy-claude-hook.sh'

    if (is.dev) {
      return join(process.cwd(), 'resources', scriptName)
    }
    return join(process.resourcesPath, 'app.asar.unpacked', 'resources', scriptName)
  }

  private buildSettingsJson(overrides?: Record<string, unknown>): string {
    const hookScript = this.getHookScriptPath()

    // Ensure the script is executable on Unix
    if (process.platform !== 'win32') {
      try {
        chmodSync(hookScript, 0o755)
      } catch {
        // May not have permission in packaged app
      }
    }

    const hookEvents = [
      'SessionStart',
      'UserPromptSubmit',
      'PreToolUse',
      'PostToolUse',
      'PostToolUseFailure',
      'PermissionRequest',
      'Stop',
      'StopFailure',
      'SubagentStart',
      'SubagentStop',
      'Notification',
      'TaskCompleted',
      'TeammateIdle',
      'PreCompact',
      'PostCompact',
      'SessionEnd',
    ]

    const hooks: Record<
      string,
      Array<{ matcher: string; hooks: Array<{ type: string; command: string }> }>
    > = {}
    for (const event of hookEvents) {
      hooks[event] = [{ matcher: '', hooks: [{ type: 'command', command: hookScript }] }]
    }

    const statusLineScript = this.getStatusLineScriptPath()
    if (process.platform !== 'win32') {
      try {
        chmodSync(statusLineScript, 0o755)
      } catch {
        // May not have permission in packaged app
      }
    }

    const settings: Record<string, unknown> = {
      ...(overrides ?? {}),
      hooks,
      statusLine: {
        type: 'command',
        command: statusLineScript,
      },
    }

    return JSON.stringify(settings, null, 2)
  }

  private getStatusLineScriptPath(): string {
    const scriptName = 'canopy-statusline.sh'
    if (is.dev) {
      return join(process.cwd(), 'resources', scriptName)
    }
    return join(process.resourcesPath, 'app.asar.unpacked', 'resources', scriptName)
  }

  private buildSessionContext(
    worktreePath: string,
    workspaceName: string,
    branch: string | null,
  ): string {
    let ctx = `Working in canopy workspace '${workspaceName}'`
    if (branch) {
      ctx += `, worktree '${branch}' (branch: ${branch})`
    }
    ctx += `.\nProject root: ${worktreePath}.`
    return ctx
  }

  private showPermissionNotification(
    event: HookEvent,
    ptySessionId: string,
    ownerWindow: BrowserWindow,
  ): void {
    if (!Notification.isSupported()) return

    const body = event.tool_name
      ? `${event.tool_name}: ${this.summarizeToolInput(event.tool_input)}`
      : 'A tool requires your approval'

    const n = new Notification({
      title: 'Claude Code — Permission Required',
      body,
    })
    n.on('click', () => {
      if (ownerWindow.isDestroyed()) return
      ownerWindow.focus()
      ownerWindow.webContents.send('claude:focusSession', { ptySessionId })
    })
    n.show()
  }

  private summarizeToolInput(input?: Record<string, unknown>): string {
    if (!input) return ''

    if (typeof input.command === 'string') {
      return this.truncate(input.command as string, 80)
    }
    if (typeof input.file_path === 'string') {
      return input.file_path as string
    }
    if (Array.isArray(input.questions) && input.questions.length > 0) {
      const first = input.questions[0] as Record<string, unknown> | undefined
      if (first && typeof first.question === 'string') {
        return this.truncate(first.question as string, 80)
      }
    }
    if (typeof input.query === 'string') {
      return this.truncate(input.query as string, 80)
    }
    if (typeof input.url === 'string') {
      return this.truncate(input.url as string, 80)
    }
    if (typeof input.pattern === 'string') {
      let summary = input.pattern as string
      if (typeof input.path === 'string') {
        summary += ` in ${input.path}`
      }
      return this.truncate(summary, 80)
    }
    if (typeof input.prompt === 'string') {
      return this.truncate(input.prompt as string, 80)
    }
    if (typeof input.description === 'string') {
      return this.truncate(input.description as string, 80)
    }
    if (typeof input.skill === 'string') {
      return input.skill as string
    }

    // Fallback: pick the first short string value instead of raw JSON
    for (const val of Object.values(input)) {
      if (typeof val === 'string' && val.length > 0) {
        return this.truncate(val, 80)
      }
    }

    return ''
  }

  private truncate(text: string, max: number): string {
    return text.length > max ? text.slice(0, max - 3) + '...' : text
  }
}
